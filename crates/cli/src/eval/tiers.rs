//! `--write-tiers`: putting what an eval measured into the pack's own
//! `tiers.json` (#39, brief §6).
//!
//! The shapes and the merge rule are [`runner::eval::TiersFile`] — they
//! are a contract with the shell, which reads this file to say "on a
//! machine like yours, this task is typically 95% automatic". What lives
//! here is only the parts that touch a disk: where the file goes, how an
//! existing one is read, and what is said when it cannot be.
//!
//! # Why the flag takes no path
//!
//! `--write-baseline` takes one because a baseline is a document you keep
//! wherever you like. `tiers.json` is not: it ships *with the pack*, so
//! its location is fixed by what the file is. And `--all` measures
//! several packs in one go, writing several files — which a single path
//! could not express even if the location were negotiable.
//!
//! # Why a corrupt file stops the command
//!
//! Merging is the whole point (brief §5 keeps an old 8GB laptop as the
//! honesty check, and its numbers must survive a measurement taken
//! elsewhere). A file that cannot be read cannot be merged into, and
//! writing anyway would throw away exactly the measurements that are
//! hardest to take again. So it says so, in a sentence, and leaves the
//! file alone for someone to look at.

use chrono::{DateTime, Utc};
use runner::eval::{EvalReport, Tier, TiersFile};
use std::path::{Path, PathBuf};

/// What a pack's tier measurements are called, inside the pack.
pub const FILE_NAME: &str = "tiers.json";

/// Where a pack's `tiers.json` lives.
pub fn path(packs_dir: &Path, pack: &str) -> PathBuf {
    packs_dir.join(pack).join(FILE_NAME)
}

/// Record everything this eval measured for one pack, keeping everything
/// it didn't.
///
/// `reports` are one pack's, one per model, each already the worst of its
/// repeats. `runs` is how many repeats each was across.
pub fn write(
    packs_dir: &Path,
    pack: &str,
    reports: &[EvalReport],
    runs: u32,
    now: DateTime<Utc>,
) -> Result<PathBuf, String> {
    let path = path(packs_dir, pack);
    if reports.is_empty() {
        return Err(format!(
            "Nothing was measured for {pack}, so there is nothing to record in {}.",
            path.display(),
        ));
    }

    let mut file = match read(&path)? {
        Some(existing) => existing,
        None => TiersFile {
            pack: pack.to_owned(),
            tiers: Vec::new(),
            baseline: Vec::new(),
        },
    };
    // The pack id is the file's, and only the file's. Everything that is
    // true of a *measurement* rather than of the pack — the pack version
    // it was taken against, the scoring it was taken under — belongs to
    // the entry, so a later measurement of another machine cannot speak
    // for it.
    file.pack = pack.to_owned();
    file.merge(
        reports
            .iter()
            .map(|report| Tier::of(report, runs, now))
            .collect::<Vec<_>>(),
    );

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Could not make {}: {e}", parent.display()))?;
    }
    // Pretty-printed: tiers.json is committed with the pack, and a diff
    // nobody can read is a diff nobody reviews.
    let text = serde_json::to_string_pretty(&file).expect("tiers serialise") + "\n";
    std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(path)
}

/// The `tiers.json` a pack already has, if it has one.
///
/// `Ok(None)` is the first measurement of a pack — an ordinary thing.
/// `Err` is a file that exists and cannot be understood, which is not:
/// see the module note on why that stops the command rather than being
/// written over.
pub fn read(path: &Path) -> Result<Option<TiersFile>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| {
        format!(
            "Could not make sense of {}: {e}. It has been left exactly as it is — \
             it holds measurements taken on machines that may not be to hand, and \
             overwriting it would lose them. Fix or move it, then measure again.",
            path.display(),
        )
    })
}
