//! Picking a measurement up where it stopped (#282).
//!
//! A letter bed takes about seventy minutes (#242), and an interrupted
//! run is a lost run — three of them went that way in one afternoon.
//! This is the cache that stops that happening.
//!
//! # The dangerous direction
//!
//! Losing a run costs an hour. *Reusing a result that should not have
//! been reused* costs the truth: the report claims numbers nobody
//! measured, and nothing says so. So the key below is deliberately
//! wide, every field earns its place by naming a wrong answer it
//! prevents, and anything unreadable is treated as a miss. Re-measuring
//! is always safe; believing a stale record is not.
//!
//! The subtlest field is `fixture_digest`. `kettle bed` regenerates a
//! bed in place — same file names, different statements, different
//! expectations. Keyed on the name alone, a resumed eval would score
//! yesterday's fixtures against today's expectations and report it as
//! a clean run.

use super::{EvalSet, FixtureResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Everything that must be identical before one fixture's score may be
/// reused. Each field prevents a specific wrong answer; the tests name
/// them one by one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeKey {
    pub pack: String,
    /// A 1.2.0 fixture must not be scored into a 1.3.0 run.
    pub pack_version: String,
    /// blake3 of the prompt, its examples and its schema. An
    /// unmeasured prompt edit is the one change this project cannot
    /// review (CLAUDE.md), and half-measuring one is worse.
    pub prompt_version: String,
    pub model: String,
    /// The weights are pinned; the sidecar is not, and a version bump
    /// can change grammar-constrained sampling on its own (#74).
    pub sidecar: String,
    /// What the sidecar loaded the model on, in its own words, or
    /// "unrecorded". The same build answers differently on Metal and
    /// on CUDA — 53 of 852 passages on 1 September 2026 — and a run
    /// interrupted on one runtime and resumed on another would present
    /// both sets of answers as one recording (#596). The whole device
    /// string, not only the backend: resume is a promise of
    /// byte-identity, and only the same runtime keeps it.
    pub device: String,
    pub scoring_version: u32,
    /// The sealed exam selection must never be spent by accident.
    pub eval_set: EvalSet,
    pub fixture: String,
    /// blake3 of the fixture's own bytes and its `expected.json`.
    pub fixture_digest: String,
}

impl ResumeKey {
    /// The file this key's record lives in. A hash rather than the
    /// fields joined together: the parts contain slashes, spaces and
    /// full stops, and a key that is also a legal path is a key with
    /// collisions waiting in it.
    fn file_name(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        // Length-prefixed, so two different keys cannot concatenate
        // into the same bytes — the same reasoning as the prompt
        // digest's.
        for part in [
            self.pack.as_str(),
            self.pack_version.as_str(),
            self.prompt_version.as_str(),
            self.model.as_str(),
            self.sidecar.as_str(),
            self.device.as_str(),
            &self.scoring_version.to_string(),
            self.eval_set.as_str(),
            self.fixture.as_str(),
            self.fixture_digest.as_str(),
        ] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        format!("{}.json", hasher.finalize().to_hex())
    }
}

/// The digest of one fixture: its own bytes and the expectations that
/// describe it, so a regenerated bed cannot pass for the old one.
pub fn fixture_digest(fixture: &Path, expected: &Path) -> String {
    fixture_digest_of(std::slice::from_ref(&fixture.to_path_buf()), expected)
}

/// The same digest for a fixture made of several documents (#354).
///
/// Every document is hashed, in binding order, so two fixtures
/// differing only in their second document are different questions —
/// otherwise the resume cache answers one with the other's answers.
///
/// One document hashes to exactly what [`fixture_digest`] has always
/// produced: the recorded bed digests in `evals/*.json` (#320) and every
/// resume key already on disk are built from it, and a hash that moved
/// for the same bytes would refuse baselines that are still valid.
pub fn fixture_digest_of(documents: &[PathBuf], expected: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    for path in documents.iter().map(PathBuf::as_path).chain([expected]) {
        match std::fs::read(path) {
            Ok(bytes) => {
                hasher.update(&(bytes.len() as u64).to_le_bytes());
                hasher.update(&bytes);
            }
            // A fixture that cannot be read will fail the run in a
            // moment anyway. Hashing the absence keeps this function
            // total rather than making every caller handle it twice.
            Err(_) => {
                hasher.update(b"unreadable");
            }
        }
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

/// The digest of a whole scored set: which questions a measurement was
/// asked (#320).
///
/// Built from each fixture's own digest — the same bytes the resume
/// cache trusts — so the bed's identity and the cache's cannot drift
/// apart about what a fixture *is*. Names are included and the list is
/// sorted, so a bed that renamed a fixture or reordered its directory
/// is a different bed, and one read in a different order is not.
///
/// Callers pass only the fixtures of the set they scored: the digest is
/// per eval set, because a pack-wide one would retire a development
/// measurement for an exam-only change (#319).
pub fn bed_digest(fixtures: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = fixtures.iter().collect();
    sorted.sort();
    let mut hasher = blake3::Hasher::new();
    for (name, digest) in sorted {
        for part in [name.as_str(), digest.as_str()] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

/// One directory of scored fixtures, addressed by [`ResumeKey`].
///
/// Deliberately not the run directory: `run_dir_for` calls
/// `RunDir::replace`, because a re-run's exchanges must not mix with
/// the last one's (#118). A cache whose whole job is to survive the
/// next run cannot live somewhere designed to be replaced by it.
pub struct ResumeCache {
    dir: PathBuf,
}

impl ResumeCache {
    pub fn at(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// This fixture's cached score, or `None`.
    ///
    /// Every failure is a miss, never an error: an absent directory, an
    /// unreadable file, a record half-written by the very interruption
    /// that made resuming necessary. Re-measuring costs seconds and is
    /// always correct.
    pub fn get(&self, key: &ResumeKey) -> Option<FixtureResult> {
        let raw = std::fs::read_to_string(self.dir.join(key.file_name())).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Keep this fixture's score for a later run.
    ///
    /// Returns an error so a caller may report it, but no caller should
    /// stop a run over it: failing to write a note about a measurement
    /// is not a reason to discard the measurement (see [`crate::run_dir`]).
    pub fn put(&self, key: &ResumeKey, result: &FixtureResult) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| {
            format!(
                "could not make the resume cache at {}: {e}",
                self.dir.display()
            )
        })?;
        let document = serde_json::to_string_pretty(result)
            .map_err(|e| format!("a scored fixture should serialise: {e}"))?;
        std::fs::write(self.dir.join(key.file_name()), document + "\n")
            .map_err(|e| format!("could not write to the resume cache: {e}"))
    }
}
