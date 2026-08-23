//! #213. A pack's `min_tier` is the floor the model manager will offer
//! somebody as a supported choice, so it is a claim about measurement —
//! not internal bookkeeping. Kettle ships the measurements in the same
//! directory (`tiers.json`), which means the claim and its evidence can
//! be held against each other by a test rather than by memory.
//!
//! This is the guard that would have caught the original defect: the
//! pack declared `min_tier: "3b"` while its own `tiers.json` recorded
//! the 3B as `fail`.
//!
//! It checks builds, not entries (#49). Several people publish a Q4_K_M
//! of the same weights and their scores differ, so a tier passes when
//! *some* build of it does — and the ones that failed stay recorded.

use runner::eval::fixture::EvalSelection;
use runner::eval::{TiersFile, Verdict, SCORING_VERSION};
use runner::packs::load_pack;
use std::path::{Path, PathBuf};

fn pack_dir(pack: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(pack)
}

/// Every pack that ships with Kettle — **found, never listed**.
///
/// This was a hand-written array holding one pack, with a comment
/// saying a pack added later "must either appear here or be measured".
/// Two were added and neither happened: the letter and renewal packs
/// declare a `min_tier` of `7b`, ship no `tiers.json` at all, and this
/// guard never looked at them. A list is exactly how that goes unseen —
/// the same trap the tokens generator hit when it named one pack's
/// template and the other drifted, generated never and checked never.
/// Reading the directory means a new pack is guarded the day it lands.
fn shipped_packs() -> Vec<String> {
    let mut packs: Vec<String> = std::fs::read_dir(pack_dir(""))
        .expect("the packs directory is there")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.path().join("pack.json").exists().then(|| {
                entry
                    .file_name()
                    .to_str()
                    .expect("a pack directory name is UTF-8")
                    .to_owned()
            })
        })
        .collect();
    packs.sort();
    assert!(!packs.is_empty(), "no packs found to check");
    packs
}

/// `"7B"` in a measurement, `"7b"` in a manifest — the same tier said
/// two ways. Compared on the digits and the letter, nothing else.
fn same_tier(declared: &str, measured_params: &str) -> bool {
    declared.eq_ignore_ascii_case(measured_params)
}

/// A floor whose evidence has gone stale, declared out loud (#254).
///
/// The same idiom as `STAGED_GOVUK_COMPONENTS` and `STAGED_TOOLS` in
/// the styling tests: an exception is a decision, so it carries a
/// reason and a date, never silence. A pack listed here is allowed to
/// declare a `min_tier` with no *current* passing build — and the
/// moment a current pass exists, the stage itself fails the suite,
/// forcing its removal. An exception that outlives its reason is the
/// thing this shape exists to prevent.
const STAGED_STALE_FLOORS: &[(&str, &str, &str)] = &[
    (
        "app.kttl.renewal-diff",
        "min_tier 7b and no tiers.json; its last measurement \
         (evals/baseline-v14-renewal.json) was a PASS on Qwen3.5-4B at scoring v14, \
         refused since v15 (#554) until the pod re-records it. Both floors were \
         declared before either pack had been measured on anything, and the guard \
         could not see them.",
        "2026-08-18",
    ),
    (
        "app.kttl.subscription-audit",
        "the Stage 3 bed (#252) re-scored classification under scoring v4 and no 7B build \
     passes it; #253 changes what classify is asked before any re-measurement can be \
     meaningful. The only 7B pass on record is scoring v2 against pack v1.0.0 — kept \
     as history, no longer a floor. Re-measured at v15 on 23 August (#554): still a FAIL on \
     Qwen3.5-4B (normalise 0.69), so the floor is stale for want of a pass, not \
     for want of a current measurement.",
        "2026-07-29",
    ),
];

#[test]
fn a_declared_min_tier_needs_a_pass_under_the_current_scoring_version() {
    for pack in &shipped_packs() {
        let pack = pack.as_str();
        let dir = pack_dir(pack);
        let loaded = load_pack(&dir).expect("a shipped pack loads");
        let declared = &loaded.manifest.model.min_tier;

        let staged = STAGED_STALE_FLOORS
            .iter()
            .find(|(staged_pack, ..)| *staged_pack == pack);

        // No file at all is the emptiest version of a stale floor, and
        // the stage has to cover it or the guard cannot be turned on at
        // all: both packs it just found declare a floor and ship no
        // measurements whatsoever. A stage is still a decision with a
        // reason and a date on it, which is the difference between this
        // and the hand-written pack list that hid them.
        let text = match std::fs::read_to_string(dir.join("tiers.json")) {
            Ok(text) => text,
            Err(_) if staged.is_some() => continue,
            Err(_) => panic!("{pack} declares min_tier {declared:?} but ships no tiers.json — the floor is then an assertion with nothing behind it"),
        };
        let tiers: TiersFile = serde_json::from_str(&text).expect("tiers.json is a TiersFile");

        let measured: Vec<_> = tiers
            .tiers
            .iter()
            .filter(|tier| {
                tier.model
                    .as_ref()
                    .is_some_and(|model| same_tier(declared, &model.params))
            })
            .collect();

        assert!(
            !measured.is_empty(),
            "{pack} declares min_tier {declared:?}, but no {declared} appears in its tiers.json. \
             The floor is the one tier that must be measured: it is what the model manager \
             offers as supported."
        );

        // The evidence the floor stands on has to be *current* (#254):
        // scored under the meaning the harness currently implements,
        // against the pack as it currently is, on the development set.
        // The eval harness refuses to compare across scoring versions
        // (exit 2), so a guard reading a pass recorded under a retired
        // version is passing on evidence its own harness calls
        // incomparable. Older entries stay in the file — history is the
        // point of a merging file — but they carry no floor.
        //
        // Development only: the held-out exam confirms a
        // pack-version-bump measurement, it is not the evidence prompt
        // iteration stands on, and spending it here would leak it into
        // routine green/red.
        let current: Vec<_> = measured
            .iter()
            .filter(|tier| {
                tier.scoring_version == SCORING_VERSION
                    && tier.pack_version == loaded.manifest.version
                    && tier.eval_set == EvalSelection::Development
            })
            .collect();

        match staged {
            // Staged: the floor is knowingly unsupported, with a reason
            // and a date on record. The stage must die with its reason:
            // a current pass appearing makes it fail until removed.
            Some((_, reason, date)) => {
                assert!(
                    current.iter().all(|tier| tier.verdict == Verdict::Fail),
                    "{pack} is staged as a stale floor (staged {date}: {reason}) but a \
                     current-scoring pass now exists — remove it from STAGED_STALE_FLOORS."
                );
            }
            // Not staged: at least one build of the declared tier must
            // pass under the current scoring, current pack version.
            // "7b" names a size, several people publish a Q4_K_M of the
            // same weights, and their scores genuinely differ (#49) —
            // so one passing build carries the tier, and the ones that
            // failed stay recorded beside it.
            None => {
                assert!(
                    current.iter().any(|tier| tier.verdict != Verdict::Fail),
                    "{pack} declares min_tier {declared:?}, and no {declared} build has a \
                     non-fail verdict under scoring v{SCORING_VERSION} against pack \
                     v{} on the development set.\n\
                     Every {declared} measurement on record:\n{}\n\
                     A floor standing only on retired evidence is not a supported choice. \
                     Either ship a build that passes the current bed, change what the pack \
                     asks of the model (#253), or stage the pack in STAGED_STALE_FLOORS \
                     with a reason and a date.",
                    loaded.manifest.version,
                    measured
                        .iter()
                        .map(|tier| format!(
                            "  - {} — {:?}, scoring v{}, pack v{}, {} set",
                            tier.model
                                .as_ref()
                                .map(|model| model.file.as_str())
                                .unwrap_or("(no model)"),
                            tier.verdict,
                            tier.scoring_version,
                            tier.pack_version,
                            tier.eval_set.as_str(),
                        ))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
        }
    }
}
