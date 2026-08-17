//! The results cache (#18): re-running the same pack, over the same
//! files, with the same model is a no-op that hands back the previous
//! run directory.
//!
//! The key is (pack id, pack version, BLAKE3 hash of the inputs, model
//! id) — every component that could change an answer, and nothing that
//! couldn't. Notably it hashes input *content*, not paths: the same
//! statement saved somewhere else produces the same run.
//!
//! The cache wraps *around* `run::run_pack` rather than living inside
//! it, so a hit costs one hash of the inputs and no pipeline at all.

use std::io;
use std::path::{Path, PathBuf};

/// A cache key: hex BLAKE3 over the four components. Opaque on purpose
/// — callers pass it around and use it as a filename, nothing else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Hash one component into `hasher`, length-prefixed so that no pair of
/// different component lists can produce the same byte stream ("ab" +
/// "c" must not collide with "a" + "bc").
fn absorb(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// The cache key for one run. Errors only if an input can't be read —
/// which the caller wants to hear about before starting anyway.
pub fn cache_key(
    pack_id: &str,
    pack_version: &str,
    inputs: &[PathBuf],
    model_id: &str,
) -> io::Result<CacheKey> {
    // The pipeline concatenates statements before grouping, so the
    // order files were picked in cannot change the answer. Sorting the
    // per-file hashes keeps two orderings of the same set on one entry.
    let mut input_hashes: Vec<[u8; 32]> = inputs
        .iter()
        .map(|path| {
            let mut file_hasher = blake3::Hasher::new();
            file_hasher.update_reader(std::fs::File::open(path)?)?;
            Ok(*file_hasher.finalize().as_bytes())
        })
        .collect::<io::Result<_>>()?;
    input_hashes.sort_unstable();

    let mut hasher = blake3::Hasher::new();
    absorb(&mut hasher, pack_id.as_bytes());
    absorb(&mut hasher, pack_version.as_bytes());
    absorb(&mut hasher, model_id.as_bytes());
    absorb(&mut hasher, &(input_hashes.len() as u64).to_le_bytes());
    for input_hash in &input_hashes {
        hasher.update(input_hash);
    }

    Ok(CacheKey(hasher.finalize().to_hex().to_string()))
}

/// A directory of cache entries: one small file per key holding the
/// path of the run directory it produced. Deliberately not a database
/// — "delete everything" has to stay a `rm -rf`, and an entry pointing
/// at a run someone deleted is a miss, not a crash.
#[derive(Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn entry(&self, key: &CacheKey) -> PathBuf {
        self.root.join(format!("{}.run", key.as_str()))
    }

    /// The run directory for `key`, if there is one and it still
    /// exists. Any unreadable or stale entry reads as a miss: the worst
    /// case is doing the work again, which is always safe.
    pub fn lookup(&self, key: &CacheKey) -> Option<PathBuf> {
        let recorded = std::fs::read_to_string(self.entry(key)).ok()?;
        let run_dir = PathBuf::from(recorded.trim_end_matches('\n'));
        run_dir.is_dir().then_some(run_dir)
    }

    /// The run directory for `key`, but only if it sits inside
    /// `runs_root` (#113).
    ///
    /// An entry is a path read off the disk: it can be stale from an
    /// install that lived elsewhere, or hand-edited, or carry a `..`.
    /// A cache must never be the thing that widens what Kettle will
    /// open, so the answer is resolved and checked against the root the
    /// caller says runs live in. Anything else is a miss, and a miss
    /// only ever costs the work again.
    pub fn lookup_within(&self, runs_root: &Path, key: &CacheKey) -> Option<PathBuf> {
        let run_dir = self.lookup(key)?;
        contains(runs_root, &run_dir).then_some(run_dir)
    }

    /// Remember that `key` produced `run_dir`.
    pub fn record(&self, key: &CacheKey, run_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.entry(key), format!("{}\n", run_dir.display()))
    }

    /// Forget every entry pointing at `run_dir`, and say how many there
    /// were (#109).
    ///
    /// Deleting a run has to take its pointers with it: an entry
    /// outliving the run it names would hand a deleted answer back to
    /// the next identical ask, which is the opposite of what the person
    /// asked for. A run nothing points at is 0, not an error — the
    /// caller is deleting a run, and whether it was ever cached is
    /// not their problem.
    pub fn forget(&self, run_dir: &Path) -> io::Result<usize> {
        let mut forgotten = 0;
        for entry in self.entries()? {
            if recorded_path(&entry).is_some_and(|recorded| same_place(&recorded, run_dir)) {
                std::fs::remove_file(&entry)?;
                forgotten += 1;
            }
        }
        Ok(forgotten)
    }

    /// Forget everything. Part of "delete all local data" (#109) — the
    /// index of what was kept is itself a record of what someone ran.
    ///
    /// Only the pointers: which run directories go is the caller's
    /// decision, not the cache's.
    pub fn clear(&self) -> io::Result<()> {
        for entry in self.entries()? {
            std::fs::remove_file(entry)?;
        }
        Ok(())
    }

    /// Every entry file, or none at all if the cache has never been
    /// written — an absent cache holds nothing, which is not a fault.
    fn entries(&self) -> io::Result<Vec<PathBuf>> {
        let listing = match std::fs::read_dir(&self.root) {
            Ok(listing) => listing,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(listing
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "run"))
            .collect())
    }
}

/// The run directory an entry file names, if it can be read.
fn recorded_path(entry: &Path) -> Option<PathBuf> {
    let recorded = std::fs::read_to_string(entry).ok()?;
    Some(PathBuf::from(recorded.trim_end_matches('\n')))
}

/// Whether two paths name the same directory, `..` and symlinks and
/// all. Falls back to a plain comparison when a path can't be resolved
/// — a run that has already gone still matches its own entry by name,
/// which is exactly what `forget` needs after a deletion.
fn same_place(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Whether `child` really is inside `root`, resolved rather than
/// compared as text. An unresolvable child is not inside anything: the
/// question is only ever asked about a directory we are about to open.
fn contains(root: &Path, child: &Path) -> bool {
    let (Ok(root), Ok(child)) = (root.canonicalize(), child.canonicalize()) else {
        return false;
    };
    child.starts_with(&root) && child != root
}
