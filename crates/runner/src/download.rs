//! Downloading model weights, verified against a pinned manifest (#49).
//! `tests/download.rs` says what this must do, and is where a change in
//! behaviour should be argued first.
//!
//! **If a test looks wrong, stop and say so rather than editing it.**
//! A test that turns out to be wrong is a good outcome and worth
//! reporting; a test quietly bent to fit an implementation is how a
//! contract stops meaning anything.
//!
//! This began as a handoff contract with `todo!()` bodies and a note
//! that the shapes must not drift. The bodies landed, and the shape
//! then drifted on purpose: `ModelDownload` gained `parts`, because the
//! floor tier turned out to be published as several files and "one
//! model, one file" could not describe it. That is the difference worth
//! keeping — a shape changed because reality contradicted it, with the
//! tests rewritten first to say so.
//!
//! ## Why verification is the whole feature
//!
//! Kettle asks somebody to download several gigabytes and then runs it
//! as a program that reads their bank statements. The digest is not a
//! nicety about corrupted downloads — it is the only thing standing
//! between "the model the manifest names" and "whatever that URL served
//! today". So the failure path matters more than the success path: a
//! file that does not match its digest must never be left where
//! `model_source` would find it and load it (#50).

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

/// One file of a model.
///
/// The digest is `sha256:<hex>` — the same spelling `ModelSource`'s
/// `verified_digest` takes, so a verified download hands its value
/// straight through without a second opinion about format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPart {
    /// What the file is called on disk once installed, e.g.
    /// `qwen2.5-3b-instruct-q4_k_m.gguf`. Taken from the manifest, never
    /// from the URL or a redirect: a server does not get to choose where
    /// Kettle writes.
    pub file_name: String,
    pub url: String,
    /// `sha256:<64 hex chars>`.
    pub sha256: String,
    /// Expected size. A mismatch is refused before any bytes are
    /// written, so a wrong or hostile URL costs one request rather than
    /// a full download.
    pub bytes: u64,
}

/// One downloadable model, as the pinned manifest names it — in one
/// file, or in several.
///
/// Larger weights are commonly published split across files, and
/// llama.cpp cannot open the first without every other part present.
/// So a model is the unit that gets installed, not a file: several
/// parts arrive, every one is verified, and either all of them land or
/// none do. Half a split model is not most of a model — it is a model
/// that fails to load, sitting exactly where `model_source` will find
/// it and try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDownload {
    /// At least one, in the order the publisher numbers them. The first
    /// is the one llama.cpp is pointed at.
    pub parts: Vec<ModelPart>,
}

impl ModelDownload {
    /// The file llama.cpp opens. For a split model that is part one,
    /// which finds its siblings by name.
    ///
    /// Panics on a model with no parts, which [`parse_manifest`] refuses
    /// to build — a partless model is not a degenerate case worth
    /// carrying through every caller, it is a manifest bug.
    pub fn file_name(&self) -> &str {
        &self.parts.first().expect("a model has parts").file_name
    }

    /// Every part's bytes. This is what a person is waiting for, so it
    /// is what progress is measured against.
    pub fn bytes(&self) -> u64 {
        self.parts.iter().map(|part| part.bytes).sum()
    }
}

/// How far along a download is, for the progress the person watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug)]
pub enum DownloadError {
    /// The bytes arrived and are not what the manifest promised. Carries
    /// both digests because the only useful thing to say next is which
    /// one to trust.
    ChecksumMismatch {
        expected: String,
        got: String,
    },
    /// The server disagreed about the size before sending a body.
    SizeMismatch {
        expected: u64,
        got: u64,
    },
    /// The person stopped it. Not a failure; the partial file is kept so
    /// resuming is possible.
    Cancelled,
    Http(String),
    Io(std::io::Error),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Plain language, per CLAUDE.md: no "checksum", no "hash".
            DownloadError::ChecksumMismatch { .. } => write!(
                f,
                "the downloaded file isn't the one Kettle expected, so it has been discarded — \
                 try again, and if it keeps happening the download source may have changed"
            ),
            DownloadError::SizeMismatch { .. } => write!(
                f,
                "the download source offered a different file than expected — nothing was saved"
            ),
            DownloadError::Cancelled => write!(f, "download stopped"),
            DownloadError::Http(reason) => {
                write!(f, "could not reach the download source: {reason}")
            }
            DownloadError::Io(e) => write!(f, "could not save the download: {e}"),
        }
    }
}

impl std::error::Error for DownloadError {}

/// Download `spec` into `into`, verify it, and only then put it where
/// Kettle will find it. Returns the installed path.
///
/// The shape the tests pin:
///
/// - bytes land in `<into>/<file_name>.part` while downloading, never at
///   the final name — `model_source` picks the first `.gguf` it finds,
///   so a half-written file at the real name is a model Kettle would try
///   to load;
/// - an existing `.part` is resumed with a `Range` header rather than
///   restarted, and a server that ignores `Range` must still produce a
///   correct file;
/// - the digest is checked before the rename, and a mismatch deletes the
///   partial file — a bad download must not be resumable into a
///   permanently poisoned one;
/// - `progress` is called as bytes arrive so a person sees movement;
/// - `cancel` is polled during the transfer, and returning true leaves
///   the `.part` in place for a later resume;
/// - a model with several parts installs all of them or none — every
///   part is verified while staged, and the renames happen only once
///   the last one has passed.
pub fn download_verified(
    spec: &ModelDownload,
    into: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &dyn Fn() -> bool,
) -> Result<PathBuf, DownloadError> {
    std::fs::create_dir_all(into).map_err(DownloadError::Io)?;

    let total = spec.bytes();
    let mut staged: Vec<(PathBuf, &ModelPart)> = Vec::new();
    let mut done_before = 0u64;

    // Every part is fetched and verified while still wearing `.part`.
    // Nothing takes its real name until the whole model has passed,
    // because a verified part beside a rejected one is still a model
    // that will not open.
    for part in &spec.parts {
        let partial = download_part(part, into, done_before, total, progress, cancel)?;
        done_before += part.bytes;
        staged.push((partial, part));
    }

    for (partial, part) in &staged {
        std::fs::rename(partial, into.join(&part.file_name)).map_err(DownloadError::Io)?;
    }
    for (_, part) in &staged {
        record_verified(into, &part.file_name, &part.sha256);
    }

    Ok(into.join(spec.file_name()))
}

/// Fetch one part and verify it, leaving it staged as `<name>.part`.
///
/// `done_before` and `total` place this part inside the whole model's
/// progress: a person watching is watching one download, and a bar that
/// reached the end and started again would read as a fault.
fn download_part(
    spec: &ModelPart,
    into: &Path,
    done_before: u64,
    total: u64,
    progress: &mut dyn FnMut(Progress),
    cancel: &dyn Fn() -> bool,
) -> Result<PathBuf, DownloadError> {
    let partial = into.join(format!("{}.part", spec.file_name));

    // What a previous attempt left behind. Asking for bytes we already
    // have is the difference between finishing a download and starting
    // one, on a file measured in gigabytes.
    let already = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);

    if cancel() {
        return Err(DownloadError::Cancelled);
    }

    // A part that is already fully here needs checking, not fetching.
    // This is the ordinary case on the second attempt at a split model
    // whose later part failed: re-downloading four verified gigabytes to
    // reach the one that went wrong would be its own kind of defect.
    // (It also avoids asking for `bytes=N-` on a file of exactly N,
    // which is a 416 and would surface as a plain HTTP failure.)
    if already == spec.bytes {
        let got = file_digest(&partial).map_err(DownloadError::Io)?;
        if got == spec.sha256 {
            progress(Progress {
                downloaded: done_before + spec.bytes,
                total,
            });
            return Ok(partial);
        }
        // Not what it claims to be. Same rule as below: bad bytes must
        // never be resumable, because every resume appends to them.
        let _ = std::fs::remove_file(&partial);
        return Err(DownloadError::ChecksumMismatch {
            expected: spec.sha256.clone(),
            got,
        });
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.get(&spec.url);
    if already > 0 {
        request = request.header("Range", &format!("bytes={already}-"));
    }
    let mut response = request
        .call()
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(DownloadError::Http(format!("the server answered {status}")));
    }

    // 206 means the server honoured the Range and is sending the tail.
    // 200 means it ignored it and is sending the whole file — appending
    // that to what we have would produce a plausible-looking file that
    // is silently wrong, so start again instead.
    let resuming = status == 206 && already > 0;
    let from = if resuming { already } else { 0 };

    // The size check happens here, on what the server says, rather than
    // on what eventually arrived: a wrong or hostile URL should cost one
    // request, not a full download. A resumed response states only the
    // remaining length, hence `from + stated`.
    if let Some(stated) = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        let whole = from + stated;
        if whole != spec.bytes {
            return Err(DownloadError::SizeMismatch {
                expected: spec.bytes,
                got: whole,
            });
        }
    }

    let mut file = if resuming {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&partial)
            .map_err(DownloadError::Io)?
    } else {
        std::fs::File::create(&partial).map_err(DownloadError::Io)?
    };

    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded = from;
    loop {
        if cancel() {
            // The partial file stays: cancelling a multi-gigabyte
            // download usually means "not now", not "never".
            return Err(DownloadError::Cancelled);
        }
        let read = reader.read(&mut buffer).map_err(DownloadError::Io)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(DownloadError::Io)?;
        downloaded += read as u64;
        progress(Progress {
            downloaded: done_before + downloaded,
            total,
        });
    }
    file.flush().map_err(DownloadError::Io)?;
    drop(file);

    // Verified before the rename, never after: `model_source` picks the
    // first .gguf in this directory, so the moment a file wears the real
    // name it is a model Kettle would load.
    let got = file_digest(&partial).map_err(DownloadError::Io)?;
    if got != spec.sha256 {
        // The partial goes too. Keeping it would let the next attempt
        // resume a download already known to be wrong, and it would
        // never come right — every resume appends to bad bytes.
        let _ = std::fs::remove_file(&partial);
        return Err(DownloadError::ChecksumMismatch {
            expected: spec.sha256.clone(),
            got,
        });
    }

    // Staged, not installed. The caller renames every part together
    // once the whole model has passed.
    Ok(partial)
}

/// Where a models directory notes what it has verified.
const VERIFIED: &str = ".verified.json";

/// The digest this model was checked against when it was installed, or
/// `None` for a file nobody here verified (#50).
///
/// The distinction is the whole point. A digest is only known to be true
/// at the moment it was checked, by the code that checked it; anything
/// reading the directory later sees bytes. So a downloaded model can
/// say what it was verified against, and a model somebody copied in by
/// hand says nothing — which sends the caller off to hash it itself
/// rather than letting an unchecked file inherit a guarantee it never
/// earned.
pub fn verified_digest(models_dir: &Path, file_name: &str) -> Option<String> {
    let raw = std::fs::read_to_string(models_dir.join(VERIFIED)).ok()?;
    let record: serde_json::Value = serde_json::from_str(&raw).ok()?;
    record.get(file_name)?.as_str().map(str::to_owned)
}

/// Note a verified install, keeping whatever else is already recorded.
///
/// Best-effort: a models directory that cannot be written to is a
/// nuisance, not a reason to throw away a download that has just been
/// checked and installed. The cost of losing the note is that the app
/// hashes the file itself next time.
fn record_verified(models_dir: &Path, file_name: &str, sha256: &str) {
    let mut record = std::fs::read_to_string(models_dir.join(VERIFIED))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    record[file_name] = serde_json::json!(sha256);
    let _ = std::fs::write(
        models_dir.join(VERIFIED),
        serde_json::to_string_pretty(&record).unwrap_or_default(),
    );
}

/// `sha256:<hex>` for a file already on disk, for re-verifying an
/// install without downloading it again.
pub fn file_digest(path: &Path) -> Result<String, std::io::Error> {
    use sha2::Digest as _;

    // Streamed, because these files do not fit in memory.
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_digest(hasher.finalize()))
}

/// `sha256:` followed by the digest in lowercase hex.
///
/// Written by iterating the bytes rather than `format!("{:x}", …)`,
/// which reads better but only compiles against sha2 0.10: 0.11 returns
/// a `hybrid_array::Array` in place of `GenericArray`, and that does not
/// implement `LowerHex`. Iterating works on both, so the digest string
/// is identical either side of the upgrade and this file does not have
/// to move in lockstep with it.
fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;

    let mut out = String::from("sha256:");
    for byte in digest.as_ref() {
        // Writing to a String cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The pinned manifest: the models Kettle offers, and nothing else.
///
/// Parsed rather than compiled in so that adding a model is a data
/// change. An entry whose digest is missing or malformed is refused at
/// parse time — an unverifiable entry in a manifest whose entire purpose
/// is verification is a bug, not a permissive default.
pub fn parse_manifest(json: &str) -> Result<Vec<ModelDownload>, String> {
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("the model list could not be read: {e}"))?;

    entries
        .into_iter()
        .map(|entry| {
            // A model published as one file writes its fields inline; a
            // split one lists them under `parts`. The single-file form
            // is not legacy support — most models are one file, and
            // wrapping every one of them in a one-element array would
            // make the common entry harder to read for the sake of the
            // rare one.
            let parts = match entry.get("parts") {
                Some(parts) => {
                    let parts = parts
                        .as_array()
                        .ok_or_else(|| "a model's parts are not a list".to_owned())?;
                    if parts.is_empty() {
                        return Err("a model in the list has no parts — there is \
                                    nothing here to download or verify"
                            .to_owned());
                    }
                    parts
                        .iter()
                        .map(parse_part)
                        .collect::<Result<Vec<_>, _>>()?
                }
                None => vec![parse_part(&entry)?],
            };
            Ok(ModelDownload { parts })
        })
        .collect()
}

fn parse_part(entry: &serde_json::Value) -> Result<ModelPart, String> {
    let string = |key: &str| {
        entry[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("a model in the list has no {key}"))
    };
    let sha256 = string("sha256")?;
    if !is_sha256(&sha256) {
        return Err(format!(
            "a model in the list has no usable digest ({sha256:?}) — \
             an entry that cannot be verified does not belong in a list \
             whose purpose is verification"
        ));
    }
    Ok(ModelPart {
        file_name: string("file_name")?,
        url: string("url")?,
        sha256,
        bytes: entry["bytes"]
            .as_u64()
            .ok_or_else(|| "a model in the list has no size".to_owned())?,
    })
}

/// `sha256:` followed by exactly 64 hex characters, and nothing else.
/// Another algorithm is not a lenient case to accept — it is a digest
/// this code cannot check.
fn is_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

// ---------------------------------------------------------------------
// Notes for the implementation, decided here so they are not decided
// twice.
//
// **`ureq` has TLS on for this** (`features = ["rustls"]` in the
// workspace manifest). It did not before #49: the crate was pulled in
// with `default-features = false` for talking to a llama-server on
// localhost, which is plain http. Every test in `tests/download.rs` is
// http too, so this is a gap tests cannot see — an implementation could
// pass all eleven and download nothing real.
//
// **Stream, do not buffer.** These files are gigabytes; read into a
// fixed buffer, write to the `.part` as you go, and hash the same bytes
// on the way past rather than re-reading the file afterwards. A resumed
// download has to hash the bytes already on disk first, or the digest
// covers only the second half.
//
// **`bytes` is checked against what the server says before the body is
// written**, not against what eventually arrived — the point is to fail
// on one request rather than after gigabytes. On a resumed request the
// server reports the *remaining* length, so compare
// `already + remaining`.
