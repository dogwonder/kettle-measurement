//! Scoring for the eval harness: turning "is this model good enough for
//! this pack?" into a number. Pure functions only — no I/O, no model, no
//! pack knowledge. Thresholds live per-pack in `pack.json` (`eval`), and
//! the tolerances that pick between these functions live in each
//! fixture's `expected.json`.
//!
//! Scoring joins on `raw` (normalise) and `name` (classify), never on
//! batch ids: those are synthetic and per-run, so `expected.json` rightly
//! contains none.

use std::collections::BTreeSet;
use std::str::FromStr;

/// How closely an answer must match the expectation to count as right.
/// Spelled in `expected.json` as `"exact"` or `"fuzzy:0.85"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tolerance {
    /// Byte-for-byte after trimming. Enums and deterministic steps.
    Exact,
    /// Case-insensitive similarity at or above the given threshold.
    Fuzzy(f64),
}

impl Tolerance {
    /// Does `actual` count as the right answer for `expected`?
    pub fn matches(&self, expected: &str, actual: &str) -> bool {
        match self {
            Tolerance::Exact => expected.trim() == actual.trim(),
            Tolerance::Fuzzy(threshold) => fuzzy_match(expected, actual, *threshold),
        }
    }
}

impl FromStr for Tolerance {
    type Err = String;

    fn from_str(spelling: &str) -> Result<Self, Self::Err> {
        // Never guess: an unreadable tolerance would silently score a run
        // against a rule nobody wrote.
        match spelling.trim() {
            "exact" => Ok(Tolerance::Exact),
            other => match other.strip_prefix("fuzzy:") {
                Some(threshold) => threshold
                    .parse()
                    .map(Tolerance::Fuzzy)
                    .map_err(|_| format!("tolerance '{spelling}': '{threshold}' is not a number")),
                None => Err(format!(
                    "tolerance '{spelling}': expected 'exact' or 'fuzzy:<threshold>'"
                )),
            },
        }
    }
}

/// Similarity of two names, 0.0 to 1.0, ignoring case, spacing and
/// punctuation — "Amazon marketplace" is not a wrong answer for "Amazon
/// Marketplace".
///
/// Edit distance, deliberately *not* the Jaro-Winkler used for merchant
/// grouping in [`crate::cleanup`]: its shared-prefix bonus scores
/// "British Gas" against "British Airways" at 0.92, which is what you
/// want when clustering variants of one merchant and exactly what you
/// must not have when marking a model's answer. Damerau over plain
/// Levenshtein so a transposed typo ("Netfilx") stays a near miss.
pub fn similarity(a: &str, b: &str) -> f64 {
    strsim::normalized_damerau_levenshtein(&comparison_key(a), &comparison_key(b))
}

/// Lowercase alphanumerics only, with `&` spelled out — the differences
/// that are never the model's mistake.
fn comparison_key(name: &str) -> String {
    name.to_lowercase()
        .replace('&', "and")
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

/// Are these the same name, allowing for near misses at `threshold`?
/// Inclusive: similarity exactly at the threshold passes.
pub fn fuzzy_match(a: &str, b: &str, threshold: f64) -> bool {
    similarity(a, b) >= threshold
}

/// Fraction of expected answers the model got right, joining each
/// `(key, value)` pair on its key. A key the model never answered counts
/// as wrong, not as absent — a model that silently drops half a batch
/// must not score 100%. Extra answers are ignored here; inventions are
/// caught by [`set_f1`] on the end result.
///
/// Expecting nothing scores 1.0: there was nothing to get wrong.
pub fn keyed_accuracy(
    expected: &[(&str, &str)],
    actual: &[(&str, &str)],
    tolerance: Tolerance,
) -> f32 {
    if expected.is_empty() {
        return 1.0;
    }
    let correct = expected
        .iter()
        .filter(|(key, want)| match answer_for(actual, key) {
            Some(got) => tolerance.matches(want, got),
            None => false,
        })
        .count();
    correct as f32 / expected.len() as f32
}

/// Fraction of expected enum values the model got exactly right, joined
/// by name. Enums come out of a JSON-Schema-constrained grammar, so
/// there are no near misses to forgive: "streaming_video" is not
/// "streaming".
pub fn enum_accuracy(expected: &[(&str, &str)], actual: &[(&str, &str)]) -> f32 {
    keyed_accuracy(expected, actual, Tolerance::Exact)
}

/// F1 of the answer set against the expected set, membership exact.
/// Used for the end-to-end score, where both misses and inventions
/// matter: `recurring` is deterministic Rust, so anything below 1.0 is a
/// Rust bug and the harness says so.
///
/// Sets, not bags — a repeated answer counts once. Two empty sets score
/// 1.0; one empty set scores 0.0.
pub fn set_f1(expected: &[&str], actual: &[&str]) -> f32 {
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    let actual: BTreeSet<&str> = actual.iter().copied().collect();

    if expected.is_empty() && actual.is_empty() {
        return 1.0;
    }
    let found = expected.intersection(&actual).count() as f32;
    if found == 0.0 {
        return 0.0;
    }
    let precision = found / actual.len() as f32;
    let recall = found / expected.len() as f32;
    2.0 * precision * recall / (precision + recall)
}

/// The model's answer for a key, joined case-insensitively (raw merchant
/// strings vary in case between statement and batch). First answer wins,
/// order-stable like the rest of the pipeline: a later duplicate cannot
/// overwrite a wrong answer into a right one.
fn answer_for<'a>(actual: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    actual
        .iter()
        .find(|(candidate, _)| candidate.trim().eq_ignore_ascii_case(key.trim()))
        .map(|(_, value)| *value)
}
