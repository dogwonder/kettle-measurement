//! CONTRACT TESTS (#49). These say what `runner::download` must do.
//!
//! **Do not edit these to make an implementation pass.** If one looks
//! wrong, stop and say so — a test that turns out to be wrong is a good
//! outcome and worth reporting. A test bent to fit the code it is
//! testing proves only that somebody edited it.
//!
//! The mock server is hand-rolled TCP, matching `tests/support`: CI
//! downloads nothing and depends on nothing.

use runner::download::{download_verified, file_digest, parse_manifest, DownloadError, Progress};
use runner::download::{ModelDownload, ModelPart};
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// A server that serves `body` for every request, recording the last
/// `Range` header it saw.
struct MockHost {
    port: u16,
    last_range: Arc<std::sync::Mutex<Option<String>>>,
}

impl MockHost {
    fn serving(body: Vec<u8>) -> Self {
        Self::serving_with(body, true)
    }

    /// `honour_range`: whether the server respects a `Range` request. A
    /// server that ignores it and sends the whole body from the start is
    /// common enough (and CDN-dependent) that resuming must cope.
    fn serving_with(body: Vec<u8>, honour_range: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock host");
        let port = listener.local_addr().expect("addr").port();
        let last_range = Arc::new(std::sync::Mutex::new(None));
        let range_slot = last_range.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => break,
                };
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    raw.extend_from_slice(&buffer[..n]);
                    if n == 0 || raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&raw).into_owned();
                // Case-insensitively, because header names are (RFC 9110
                // §5.1) and ureq 3 sends `range:` in lower case — it is
                // built on the `http` crate, which normalises them.
                // Matching "Range: " exactly made this mock deaf to a
                // header that was being sent correctly all along.
                let range = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("range")
                        .then(|| value.trim().to_owned())
                });
                *range_slot.lock().expect("range slot") = range.clone();

                let start = match (&range, honour_range) {
                    (Some(value), true) => value
                        .trim_start_matches("bytes=")
                        .split('-')
                        .next()
                        .and_then(|from| from.parse::<usize>().ok())
                        .unwrap_or(0),
                    _ => 0,
                };
                let slice = &body[start.min(body.len())..];
                let status = if start > 0 && honour_range {
                    "HTTP/1.1 206 Partial Content"
                } else {
                    "HTTP/1.1 200 OK"
                };
                let header = format!(
                    "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(slice);
                let _ = stream.flush();
            }
        });

        MockHost { port, last_range }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/model.gguf", self.port)
    }
}

/// A server that serves a different body per path, for the split-model
/// case (#49). Deliberately separate from `MockHost` rather than a flag
/// on it: the single-file tests are the ones that must not change
/// meaning while this is added.
struct MockParts {
    port: u16,
}

impl MockParts {
    fn serving(bodies: Vec<(&'static str, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock host");
        let port = listener.local_addr().expect("addr").port();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => break,
                };
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    raw.extend_from_slice(&buffer[..n]);
                    if n == 0 || raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&raw).into_owned();
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("")
                    .to_owned();
                let body = bodies
                    .iter()
                    .find(|(name, _)| path.ends_with(name))
                    .map(|(_, body)| body.clone())
                    .unwrap_or_default();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });

        MockParts { port }
    }

    fn url(&self, name: &str) -> String {
        format!("http://127.0.0.1:{}/{name}", self.port)
    }
}

fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("kettle-download-{}-{name}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn sha256_of(bytes: &[u8]) -> String {
    // Deliberately not computed with the same helper the implementation
    // uses: a test that shares its subject's arithmetic checks nothing.
    // Hence the separate hex encoding here too, rather than reaching for
    // `download::hex_digest` — `{:x}` would be shorter but compiles only
    // against sha2 0.10, whose `GenericArray` implements `LowerHex`.
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, bytes);
    let hex: String = sha2::Digest::finalize(hasher)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex}")
}

/// A single-file model, which is simply a model with one part.
fn spec(host: &MockHost, sha256: String, bytes: u64) -> ModelDownload {
    ModelDownload {
        parts: vec![ModelPart {
            file_name: "test-model.gguf".to_owned(),
            url: host.url(),
            sha256,
            bytes,
        }],
    }
}

/// **The test #49 names.** A file that does not match its digest must
/// not survive the attempt.
///
/// This is the whole feature. Kettle asks somebody to download several
/// gigabytes and then runs it as the thing that reads their bank
/// statements; the digest is what separates "the model the manifest
/// names" from "whatever that URL served today". Leaving the bad bytes
/// on disk would be worse than not downloading at all, because
/// `model_source` picks up the first `.gguf` it finds.
#[test]
fn checksum_mismatch_rejects_file() {
    let body = b"not the model you were promised".to_vec();
    let host = MockHost::serving(body.clone());
    let into = scratch("mismatch");
    let wrong = "sha256:".to_owned() + &"0".repeat(64);

    let error = download_verified(
        &spec(&host, wrong, body.len() as u64),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect_err("a file that does not match its digest must be refused");

    assert!(
        matches!(error, DownloadError::ChecksumMismatch { .. }),
        "expected a checksum mismatch, got {error:?}",
    );
    assert!(
        !into.join("test-model.gguf").exists(),
        "the rejected download was left where model_source would load it",
    );
    assert!(
        !into.join("test-model.gguf.part").exists(),
        "the partial file survived, so a retry would resume a download already known to be wrong",
    );
}

/// The success path: verified bytes arrive at the manifest's file name.
#[test]
fn a_verified_download_is_installed_under_its_manifest_name() {
    let body = b"pretend these are weights".to_vec();
    let host = MockHost::serving(body.clone());
    let into = scratch("verified");

    let installed = download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect("a matching download installs");

    assert_eq!(installed, into.join("test-model.gguf"));
    assert_eq!(std::fs::read(&installed).expect("read installed"), body);
    assert!(
        !into.join("test-model.gguf.part").exists(),
        "the partial file should be gone once installed",
    );
}

/// Nothing is written at the final name until it has been verified.
///
/// `model_source` picks the first `.gguf` in the directory, so a
/// half-written file under the real name is a model Kettle would try to
/// load — mid-download, on a machine whose owner is watching a progress
/// bar.
#[test]
fn nothing_appears_at_the_final_name_until_it_is_verified() {
    // Comfortably larger than any sane read buffer, so `downloaded <
    // total` is genuinely true for some of the progress reports. Sized
    // to the buffer, this test could see one report at 100% and pass
    // having checked nothing.
    let body = vec![b'w'; 512 * 1024];
    let host = MockHost::serving(body.clone());
    let into = scratch("partial");
    let final_name = into.join("test-model.gguf");
    let seen_early = Arc::new(AtomicBool::new(false));
    let mid_transfer = Arc::new(AtomicBool::new(false));

    let watcher = seen_early.clone();
    let reached_middle = mid_transfer.clone();
    let watched = final_name.clone();
    download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut move |progress: Progress| {
            if progress.downloaded < progress.total {
                reached_middle.store(true, Ordering::Relaxed);
                if watched.exists() {
                    watcher.store(true, Ordering::Relaxed);
                }
            }
        },
        &|| false,
    )
    .expect("download installs");

    assert!(
        mid_transfer.load(Ordering::Relaxed),
        "no progress report arrived before the end, so this test checked nothing",
    );
    assert!(
        !seen_early.load(Ordering::Relaxed),
        "the final file name existed while bytes were still arriving",
    );
}

/// Progress is reported, and it ends where it should.
#[test]
fn progress_is_reported_and_finishes_at_the_total() {
    let body = vec![b'p'; 32 * 1024];
    let host = MockHost::serving(body.clone());
    let into = scratch("progress");
    let seen: Arc<std::sync::Mutex<Vec<Progress>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

    let record = seen.clone();
    download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut move |p| record.lock().expect("progress log").push(p),
        &|| false,
    )
    .expect("download installs");

    let seen = seen.lock().expect("progress log");
    assert!(!seen.is_empty(), "no progress was reported at all");
    let last = seen.last().expect("a final progress report");
    assert_eq!(
        last.downloaded, last.total,
        "the last progress report should say it finished",
    );
    assert_eq!(last.total, body.len() as u64);
}

/// An interrupted download resumes rather than starting again — several
/// gigabytes is too much to repeat because a laptop lid closed.
#[test]
fn an_interrupted_download_resumes_from_what_it_already_has() {
    let body = vec![b'r'; 8 * 1024];
    let host = MockHost::serving(body.clone());
    let into = scratch("resume");
    let already = 3 * 1024;
    std::fs::write(into.join("test-model.gguf.part"), &body[..already])
        .expect("a partial download from last time");

    download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect("download completes");

    let range = host.last_range.lock().expect("range").clone();
    assert_eq!(
        range.as_deref(),
        Some(&format!("bytes={already}-")[..]),
        "a resumable download must ask for the bytes it is missing",
    );
    assert_eq!(
        std::fs::read(into.join("test-model.gguf")).expect("read installed"),
        body,
    );
}

/// ...and a server that ignores `Range` still has to produce a correct
/// file. Whether a CDN honours it is not something Kettle controls, and
/// gluing a fresh full body onto existing bytes would corrupt silently
/// — which the digest would catch, but only after another few gigabytes.
#[test]
fn a_server_that_ignores_range_still_produces_a_correct_file() {
    let body = vec![b'i'; 8 * 1024];
    let host = MockHost::serving_with(body.clone(), false);
    let into = scratch("ignores-range");
    std::fs::write(into.join("test-model.gguf.part"), &body[..2048])
        .expect("a partial download from last time");

    download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect("download completes even though the server restarted it");

    assert_eq!(
        std::fs::read(into.join("test-model.gguf")).expect("read installed"),
        body,
        "the file was assembled from a full body pasted after a partial one",
    );
}

/// Cancelling keeps what has been fetched, because the point of
/// cancelling a multi-gigabyte download is usually "not now", not
/// "never".
#[test]
fn cancelling_keeps_the_partial_file_for_later() {
    let body = vec![b'c'; 512 * 1024];
    let host = MockHost::serving(body.clone());
    let into = scratch("cancel");

    // Cancel *during* the transfer, not before it. A closure that
    // returns true immediately would be answered before a single byte
    // moved, and this test would pass without ever exercising the thing
    // it is named after — whether a part-finished download survives.
    let started = Arc::new(AtomicBool::new(false));
    let watcher = started.clone();
    let stop = started.clone();

    let error = download_verified(
        &spec(&host, sha256_of(&body), body.len() as u64),
        &into,
        &mut move |_| watcher.store(true, Ordering::Relaxed),
        &move || stop.load(Ordering::Relaxed),
    )
    .expect_err("a cancelled download does not install");

    assert!(
        matches!(error, DownloadError::Cancelled),
        "expected Cancelled, got {error:?}",
    );
    assert!(
        !into.join("test-model.gguf").exists(),
        "a cancelled download must not install",
    );
    let partial = into.join("test-model.gguf.part");
    assert!(
        partial.is_file(),
        "the partial file was thrown away, so resuming means downloading it all again",
    );
    let kept = std::fs::metadata(&partial).expect("partial metadata").len();
    assert!(
        kept > 0 && kept < body.len() as u64,
        "expected a part-finished file, got {kept} of {} bytes",
        body.len(),
    );
}

/// A size the manifest did not predict is refused before the body is
/// written: a wrong or hostile URL should cost one request, not a full
/// download and then a digest failure.
#[test]
fn a_size_the_manifest_did_not_predict_is_refused() {
    let body = vec![b's'; 4 * 1024];
    let host = MockHost::serving(body.clone());
    let into = scratch("size");

    let error = download_verified(
        &spec(&host, sha256_of(&body), 999_999),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect_err("a body of unexpected size must be refused");

    assert!(
        matches!(error, DownloadError::SizeMismatch { .. }),
        "expected SizeMismatch, got {error:?}",
    );
    assert!(!into.join("test-model.gguf").exists());
}

/// `file_digest` agrees with the digests the manifest carries, so an
/// installed model can be re-verified without downloading it again.
#[test]
fn file_digest_matches_the_manifest_spelling() {
    let into = scratch("digest");
    let body = b"some bytes".to_vec();
    let path = into.join("thing.gguf");
    std::fs::write(&path, &body).expect("write");

    assert_eq!(file_digest(&path).expect("digest"), sha256_of(&body));
}

/// The manifest is data, so adding a model is a data change.
#[test]
fn the_manifest_parses_into_downloadable_models() {
    let models = parse_manifest(
        r#"[
             {
               "file_name": "qwen2.5-3b-instruct-q4_k_m.gguf",
               "url": "https://example.invalid/qwen.gguf",
               "sha256": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
               "bytes": 2000000000
             }
           ]"#,
    )
    .expect("a well-formed manifest parses");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].file_name(), "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(models[0].bytes(), 2_000_000_000);
}

/// An entry with no usable digest is refused at parse time. A manifest
/// whose entire purpose is verification must not carry an entry that
/// cannot be verified, and "allow it and check later" is how that
/// becomes optional.
#[test]
fn a_manifest_entry_without_a_usable_digest_is_refused() {
    for bad in [
        r#""sha256": """#,
        r#""sha256": "sha256:short""#,
        r#""sha256": "md5:0000000000000000000000000000000000000000000000000000000000000000""#,
    ] {
        let json = format!(
            r#"[{{ "file_name": "m.gguf", "url": "https://example.invalid/m", {bad}, "bytes": 1 }}]"#
        );
        assert!(
            parse_manifest(&json).is_err(),
            "an entry with {bad} should be refused",
        );
    }
}

/// #50's last criterion: an installed app can later find a *checksummed*
/// model — not merely a file that happens to be there.
///
/// The digest is only known to be true at the moment it was checked, by
/// the code that checked it. Anything reading the models directory
/// afterwards sees bytes and has to take them on trust, so the download
/// leaves a note saying which file it verified and to what.
#[test]
fn a_verified_download_records_what_it_verified() {
    let body = b"weights that were checked".to_vec();
    let host = MockHost::serving(body.clone());
    let into = scratch("record");
    let digest = sha256_of(&body);

    download_verified(
        &spec(&host, digest.clone(), body.len() as u64),
        &into,
        &mut |_| {},
        &|| false,
    )
    .expect("download installs");

    assert_eq!(
        runner::download::verified_digest(&into, "test-model.gguf").as_deref(),
        Some(digest.as_str()),
    );
}

/// A file nobody verified has no digest to offer, and saying so is the
/// point: `None` sends the caller to hash it itself rather than letting
/// an unchecked file inherit somebody else's guarantee.
#[test]
fn a_model_put_there_by_hand_has_no_recorded_digest() {
    let into = scratch("by-hand");
    std::fs::write(into.join("dropped-in.gguf"), b"nobody checked these")
        .expect("a model copied in by hand");

    assert_eq!(
        runner::download::verified_digest(&into, "dropped-in.gguf"),
        None
    );
}

/// A rejected download must not leave a note claiming it verified
/// something — the record and the file have to fail together.
#[test]
fn a_rejected_download_records_nothing() {
    let body = b"not what was promised".to_vec();
    let host = MockHost::serving(body.clone());
    let into = scratch("rejected-record");

    let _ = download_verified(
        &spec(
            &host,
            "sha256:".to_owned() + &"0".repeat(64),
            body.len() as u64,
        ),
        &into,
        &mut |_| {},
        &|| false,
    );

    assert_eq!(
        runner::download::verified_digest(&into, "test-model.gguf"),
        None
    );
}

// ---------------------------------------------------------------------------
// #49: split models — one model published as several files.
//
// The recommended tier is published this way, and llama.cpp cannot open
// part one without every other part present. So a partially installed
// split model is not "most of a model": it is a model that fails to
// load, sitting exactly where `model_source` will find it and try. That
// makes all-or-nothing installation the whole point of these tests, in
// the same way a rejected digest never reaching the final name is the
// point of the single-file ones.

fn part(host: &MockParts, name: &'static str, body: &[u8]) -> ModelPart {
    ModelPart {
        file_name: name.to_owned(),
        url: host.url(name),
        sha256: sha256_of(body),
        bytes: body.len() as u64,
    }
}

#[test]
fn every_part_of_a_split_model_is_installed_and_the_first_is_returned() {
    let one = b"first part of the weights".to_vec();
    let two = b"second part".to_vec();
    let host = MockParts::serving(vec![
        ("m-00001-of-00002.gguf", one.clone()),
        ("m-00002-of-00002.gguf", two.clone()),
    ]);
    let into = scratch("split-ok");

    let model = ModelDownload {
        parts: vec![
            part(&host, "m-00001-of-00002.gguf", &one),
            part(&host, "m-00002-of-00002.gguf", &two),
        ],
    };

    let installed =
        download_verified(&model, &into, &mut |_| {}, &|| false).expect("both parts verify");

    // llama.cpp is pointed at part one and finds the rest by name, so
    // that is the path worth handing back.
    assert_eq!(installed, into.join("m-00001-of-00002.gguf"));
    assert_eq!(std::fs::read(&installed).expect("part one"), one);
    assert_eq!(
        std::fs::read(into.join("m-00002-of-00002.gguf")).expect("part two"),
        two
    );
}

#[test]
fn a_split_model_installs_nothing_when_any_part_fails_its_digest() {
    let one = b"first part of the weights".to_vec();
    let two = b"second part".to_vec();
    let host = MockParts::serving(vec![
        ("m-00001-of-00002.gguf", one.clone()),
        ("m-00002-of-00002.gguf", two.clone()),
    ]);
    let into = scratch("split-bad");

    let mut bad = part(&host, "m-00002-of-00002.gguf", &two);
    bad.sha256 = "sha256:".to_owned() + &"0".repeat(64);

    let model = ModelDownload {
        parts: vec![part(&host, "m-00001-of-00002.gguf", &one), bad],
    };

    let error = download_verified(&model, &into, &mut |_| {}, &|| false)
        .expect_err("a part that does not match its digest must be refused");
    assert!(
        matches!(error, DownloadError::ChecksumMismatch { .. }),
        "expected a checksum mismatch, got {error:?}"
    );

    // The first part verified perfectly well. It still must not be
    // installed: on its own it is a model llama.cpp will pick up and
    // fail to open, and `model_source` takes the first .gguf it finds.
    assert!(
        !into.join("m-00001-of-00002.gguf").exists(),
        "a verified part was installed beside a rejected one, leaving half a model"
    );
    assert!(
        !into.join("m-00002-of-00002.gguf").exists(),
        "the rejected part was installed"
    );
}

#[test]
fn progress_across_a_split_model_counts_every_part() {
    let one = vec![b'a'; 40_000];
    let two = vec![b'b'; 10_000];
    let host = MockParts::serving(vec![
        ("m-00001-of-00002.gguf", one.clone()),
        ("m-00002-of-00002.gguf", two.clone()),
    ]);
    let into = scratch("split-progress");

    let model = ModelDownload {
        parts: vec![
            part(&host, "m-00001-of-00002.gguf", &one),
            part(&host, "m-00002-of-00002.gguf", &two),
        ],
    };

    let mut seen: Vec<Progress> = Vec::new();
    download_verified(&model, &into, &mut |p| seen.push(p), &|| false).expect("installs");

    // A person watching one progress bar is watching one download. A
    // bar that reached 100% and restarted would read as a fault.
    let total = (one.len() + two.len()) as u64;
    assert!(
        seen.iter().all(|p| p.total == total),
        "the total must be the whole model, not the part in flight: {seen:?}"
    );
    assert_eq!(
        seen.last().expect("at least one report").downloaded,
        total,
        "progress must finish at the total"
    );
    assert!(
        seen.windows(2).all(|w| w[0].downloaded <= w[1].downloaded),
        "progress must never go backwards between parts: {seen:?}"
    );
}

#[test]
fn the_manifest_reads_a_split_model_as_its_parts() {
    let models = parse_manifest(
        r#"[
             {
               "parts": [
                 {
                   "file_name": "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
                   "url": "https://example.invalid/one.gguf",
                   "sha256": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                   "bytes": 3993201344
                 },
                 {
                   "file_name": "qwen2.5-7b-instruct-q4_k_m-00002-of-00002.gguf",
                   "url": "https://example.invalid/two.gguf",
                   "sha256": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                   "bytes": 689872288
                 }
               ]
             }
           ]"#,
    )
    .expect("a split entry parses");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].parts.len(), 2);
    assert_eq!(
        models[0].file_name(),
        "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf"
    );
    assert_eq!(models[0].bytes(), 3_993_201_344 + 689_872_288);
}

#[test]
fn a_split_entry_with_one_unverifiable_part_is_refused_whole() {
    // Same rule as the single-file case, for the same reason: a list
    // whose purpose is verification must not carry something that
    // cannot be verified. Half a verifiable model is not half safe.
    let json = r#"[
        {
          "parts": [
            {
              "file_name": "a.gguf",
              "url": "https://example.invalid/a",
              "sha256": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
              "bytes": 1
            },
            {
              "file_name": "b.gguf",
              "url": "https://example.invalid/b",
              "sha256": "",
              "bytes": 1
            }
          ]
        }
    ]"#;

    assert!(parse_manifest(json).is_err());
}

#[test]
fn a_model_with_no_parts_at_all_is_refused() {
    assert!(parse_manifest(r#"[{ "parts": [] }]"#).is_err());
}

/// The manifest Kettle actually ships has to parse, and every digest in
/// it has to be one `download_verified` can check.
///
/// It is a data file that nothing else validates: a typo in a digest,
/// or an entry that quietly lost its `sha256`, would otherwise surface
/// as a failed download on a stranger's machine after several
/// gigabytes. `parse_manifest` refuses both, so the only thing missing
/// was somebody running it against the real file.
#[test]
fn the_shipped_manifest_parses_and_every_entry_can_be_verified() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/models.json");
    let json = std::fs::read_to_string(&path).expect("the shipped models.json");

    let models = parse_manifest(&json)
        .unwrap_or_else(|e| panic!("app/src-tauri/models.json does not parse: {e}"));

    assert!(!models.is_empty(), "a model list with no models in it");

    for model in &models {
        assert!(
            !model.parts.is_empty(),
            "{} has no parts",
            model.file_name()
        );
        for part in &model.parts {
            assert!(
                part.sha256.starts_with("sha256:") && part.sha256.len() == 71,
                "{} carries a digest download_verified cannot check: {:?}",
                part.file_name,
                part.sha256
            );
            assert!(part.bytes > 0, "{} claims no size", part.file_name);
            assert!(
                part.url.starts_with("https://"),
                "{} would be fetched over {:?} — weights arrive over TLS or not at all",
                part.file_name,
                part.url
            );
        }
    }
}
