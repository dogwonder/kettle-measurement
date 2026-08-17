//! The per-run directory (#24): everything one run did, in one place.
//!
//! Input hashes, every raw request and response the model saw, timings,
//! and the run's outputs. Two things get easy at once (brief §4a):
//! debugging a bad answer — you can read exactly what was asked — and
//! "delete everything", which is one `remove_dir_all`.
//!
//! Layout:
//!
//! ```text
//! <root>/<run-id>/
//!   run.json          input names, sizes and hashes, and the model
//!   raw/0001-grouping-payments-by-merchant.request.txt   (the prompt)
//!   raw/0001-grouping-payments-by-merchant.response.json (the answer)
//!   claims.json       kettle/claim-traces@0 diagnostic lifecycle
//!   results.json      kettle/run-report@0
//!   actions.json      kettle/proposed-actions@0
//!   report.html       self-contained
//! ```
//!
//! Raw files are numbered in the order they happened, so reading the
//! directory top to bottom replays the run.
//!
//! CONTRACT: `RunLog` is the seam the pipeline writes through. Its
//! shape is fixed — `run_pack` and `exec::run_batch` take a `&dyn
//! RunLog` and neither should learn what a directory is.

use std::path::{Component, Path, PathBuf};

/// Where a run writes its evidence. The pipeline knows only this.
///
/// Implementations must never fail a run: a full disk is a reason to
/// stop logging, not a reason to lose the answers. Errors are swallowed
/// deliberately, which is why nothing here returns `Result`.
pub trait RunLog {
    /// One model exchange, exactly as it went over the wire.
    fn exchange(
        &self,
        step: &str,
        batch: usize,
        items: &[crate::exec::BatchItem],
        request: &str,
        response: &str,
    );
}

/// For runs nobody is debugging — unit tests, and any caller that has
/// nowhere to write. The eval harness deliberately does *not* use this
/// when given a runs directory: a score with no record of what the
/// model actually said cannot be turned into a prompt edit.
pub struct NoLog;

impl RunLog for NoLog {
    fn exchange(
        &self,
        _step: &str,
        _batch: usize,
        _items: &[crate::exec::BatchItem],
        _request: &str,
        _response: &str,
    ) {
    }
}

/// A run's directory on disk.
pub struct RunDir {
    pub path: PathBuf,
    /// Next raw-file number. Interior mutability because `RunLog` takes
    /// `&self` — the pipeline holds it immutably.
    next: std::cell::Cell<usize>,
}

#[derive(Debug)]
pub enum RunDirError {
    Io(std::io::Error),
    /// A run id that would escape the root — `..`, an absolute path, a
    /// separator. Refused before anything is created.
    UnsafeRunId(String),
    /// That run id already has a directory (#118). Completed and errored
    /// runs keep theirs, so this is a normal answer, not a fault: the
    /// caller picks another id and tries again.
    AlreadyExists(String),
}

impl std::fmt::Display for RunDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunDirError::Io(e) => write!(f, "{e}"),
            RunDirError::UnsafeRunId(id) => write!(f, "that isn't a usable run name: {id}"),
            RunDirError::AlreadyExists(id) => write!(f, "that run already has a folder: {id}"),
        }
    }
}

impl std::error::Error for RunDirError {}

/// Is `run_id` a single plain path component? Refuses anything that
/// could make `root.join(run_id)` land outside `root` — `..`, an
/// absolute path, a separator — the same trust boundary `packs.rs`
/// drew for pack-relative paths in #77, applied here to run ids.
fn is_safe_run_id(run_id: &str) -> bool {
    if run_id.is_empty() {
        return false;
    }
    let mut components = Path::new(run_id).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// A step label as a filename: lowercased, spaces hyphenated, and
/// anything that isn't a letter, digit or hyphen dropped.
///
/// Today's labels are hardcoded in `run.rs` and perfectly tame. Packs
/// declaring their own labels is a live possibility, though, and a
/// label containing a separator would otherwise write outside `raw/`.
/// Filtering here costs nothing and closes that before it opens.
fn slug(step: &str) -> String {
    step.to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// One recorded input: name only (never a full path — see the note on
/// `InputInfo::file` in `results.rs`), its size, and the BLAKE3 hash the
/// results cache also keys on.
#[derive(serde::Serialize, serde::Deserialize)]
struct RecordedInput {
    file: String,
    size: u64,
    hash: String,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct RunManifest {
    #[serde(default)]
    inputs: Vec<RecordedInput>,
    /// Which model answered, when one did (#303).
    ///
    /// A run directory is written by a run that knew this and used to
    /// drop it, so a replay of those exchanges could not say whose
    /// answers it was serving — and `baseline::compare`, which joins on
    /// the model, could never match a replayed report to a live one.
    /// `None` is the deterministic floor, where the honest answer really
    /// is that no model was involved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<crate::eval::ModelInfo>,
}

impl RunDir {
    /// Create `<root>/<run_id>/raw/`. `run_id` must be a single plain
    /// path component (`packs.rs` learned this lesson in #77) that no
    /// run has used before.
    ///
    /// The run directory itself is made with `create_dir`, never
    /// `create_dir_all`: existence *is* the claim on the id (#118).
    /// Check-then-create would let two callers — or the same app after a
    /// restart — both think they own a directory and write over an
    /// earlier run's evidence. `AlreadyExists` means "pick another id",
    /// and nothing inside the existing directory is touched.
    pub fn create(root: &Path, run_id: &str) -> Result<RunDir, RunDirError> {
        if !is_safe_run_id(run_id) {
            return Err(RunDirError::UnsafeRunId(run_id.to_string()));
        }
        let path = root.join(run_id);
        // The root may not exist yet; only the run's own directory is
        // an exclusive claim.
        std::fs::create_dir_all(root).map_err(RunDirError::Io)?;
        std::fs::create_dir(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => RunDirError::AlreadyExists(run_id.to_string()),
            _ => RunDirError::Io(e),
        })?;
        std::fs::create_dir_all(path.join("raw")).map_err(RunDirError::Io)?;
        Ok(RunDir {
            path,
            next: std::cell::Cell::new(1),
        })
    }

    /// Create `<root>/<run_id>/`, deleting anything already there.
    ///
    /// For callers whose ids are deterministic by design — the eval
    /// harness names a directory after (pack, model, fixture) — where
    /// running again means *this* run's exchanges, not a merge of two
    /// (#118). App runs must use [`RunDir::create`]: a person's evidence
    /// is never replaced on the strength of a reused number.
    /// Take up a run directory that already exists, to write outputs
    /// into it after the run itself has finished (#412).
    ///
    /// Used when a letter run was parked waiting for its date and a
    /// person has now settled it. The directory is not re-created and
    /// nothing in it is cleared — the parked outcome and the run's
    /// inputs are exactly what is being finished.
    ///
    /// Raw-file numbering resumes past whatever is already in `raw/`,
    /// so a later log cannot overwrite an earlier one. It resumes at 1
    /// on a directory with no raw files, which is the same place
    /// [`RunDir::create`] starts.
    pub fn open(path: &Path) -> Result<RunDir, RunDirError> {
        if !path.is_dir() {
            return Err(RunDirError::UnsafeRunId(
                path.to_string_lossy().into_owned(),
            ));
        }
        let used = std::fs::read_dir(path.join("raw"))
            .map(|entries| entries.flatten().count())
            .unwrap_or(0);
        Ok(RunDir {
            path: path.to_path_buf(),
            next: std::cell::Cell::new(used + 1),
        })
    }

    pub fn replace(root: &Path, run_id: &str) -> Result<RunDir, RunDirError> {
        if !is_safe_run_id(run_id) {
            return Err(RunDirError::UnsafeRunId(run_id.to_string()));
        }
        match std::fs::remove_dir_all(root.join(run_id)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(RunDirError::Io(e)),
        }
        RunDir::create(root, run_id)
    }

    /// Record what went in: each input's path, size and BLAKE3 hash —
    /// the same hash the results cache keys on (`cache.rs`).
    pub fn record_inputs(&self, inputs: &[PathBuf]) -> Result<(), RunDirError> {
        let mut recorded = Vec::with_capacity(inputs.len());
        for input in inputs {
            let metadata = std::fs::metadata(input).map_err(RunDirError::Io)?;
            let mut hasher = blake3::Hasher::new();
            hasher
                .update_reader(std::fs::File::open(input).map_err(RunDirError::Io)?)
                .map_err(RunDirError::Io)?;
            recorded.push(RecordedInput {
                file: input
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size: metadata.len(),
                hash: format!("blake3:{}", hasher.finalize().to_hex()),
            });
        }
        let mut manifest = self.manifest();
        manifest.inputs = recorded;
        self.write_manifest(&manifest)
    }

    /// Record which model answered here (#303).
    ///
    /// Merged into `run.json` rather than written beside it, so the file
    /// that says what a run stood on stays one file however many callers
    /// contribute to it — and so the two writers are order-independent.
    pub fn record_model(&self, model: Option<&crate::eval::ModelInfo>) -> Result<(), RunDirError> {
        let mut manifest = self.manifest();
        manifest.model = model.cloned();
        self.write_manifest(&manifest)
    }

    /// What `run.json` currently says, or an empty manifest. A manifest
    /// that will not parse is treated as absent: this is a record being
    /// added to, and refusing to write because the old copy was corrupt
    /// would lose the run rather than save it.
    fn manifest(&self) -> RunManifest {
        std::fs::read_to_string(self.path.join("run.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write_manifest(&self, manifest: &RunManifest) -> Result<(), RunDirError> {
        let contents = serde_json::to_string_pretty(manifest)
            .expect("RunManifest is always representable as JSON");
        self.write_output("run.json", &contents)
    }

    /// Write one of the run's output documents, e.g. `results.json`.
    pub fn write_output(&self, name: &str, contents: &str) -> Result<(), RunDirError> {
        std::fs::write(self.path.join(name), contents).map_err(RunDirError::Io)
    }

    /// Persist the model-claim lifecycle as a versioned diagnostic
    /// output. It lives inside the run directory so the existing
    /// one-call deletion guarantee includes it automatically.
    pub fn record_claims(
        &self,
        claims: &[crate::claim_trace::ClaimTrace],
    ) -> Result<(), RunDirError> {
        let document = crate::claim_trace::ClaimTraceDocument::new(claims);
        let contents = serde_json::to_string_pretty(&document)
            .expect("claim trace documents are always representable as JSON");
        self.write_output("claims.json", &(contents + "\n"))
    }

    /// Delete the whole run — "delete everything", one call.
    pub fn delete(self) -> Result<(), RunDirError> {
        std::fs::remove_dir_all(&self.path).map_err(RunDirError::Io)
    }
}

impl RunLog for RunDir {
    /// `raw/0001-<slug of step>.request.txt` and `.response.json`.
    ///
    /// The request is the *rendered prompt*, not the HTTP payload, so it
    /// is text and is named as such (#478) — the archive is published,
    /// and a filename that misdescribes its contents is a claim that
    /// does not match its evidence. The response really is JSON.
    /// `replay` reads both suffixes so runs archived under the old name
    /// keep replaying.
    /// The slug is the progress label lowercased with spaces as
    /// hyphens, so the filenames read like the run looked. Numbering is
    /// a single sequence across the whole run, not per batch, so
    /// reading the directory top to bottom replays the run in order;
    /// `batch` is the caller's own bookkeeping and isn't needed here.
    ///
    /// Errors are swallowed on purpose (see the trait doc): a full disk
    /// stops the logging, not the run.
    fn exchange(
        &self,
        step: &str,
        batch: usize,
        items: &[crate::exec::BatchItem],
        request: &str,
        response: &str,
    ) {
        let _ = batch;
        let _ = items;
        let number = self.next.get();
        self.next.set(number + 1);
        let slug = slug(step);
        let raw = self.path.join("raw");
        let _ = std::fs::write(raw.join(format!("{number:04}-{slug}.request.txt")), request);
        let _ = std::fs::write(
            raw.join(format!("{number:04}-{slug}.response.json")),
            response,
        );
    }
}
