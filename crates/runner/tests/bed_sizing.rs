//! #310: a bed must carry the evidence its ceilings need, counted in
//! the unit the ceiling is judged on.
//!
//! This rule already existed for the letter pack, and it passed, because
//! it counted **rows**. The letter bed's `no_obligation` ceiling of 5%
//! needs 73 clean decisions; it had 415 rows made of **23 distinct
//! sentences**, each answered identically every time it appeared. The
//! subscription bed had no such test at all, and nine of its nineteen
//! gates change verdict once repetition is discounted — every one from
//! PASS to FAIL.
//!
//! A bed that cannot carry a gate's evidence does not make the gate
//! lenient. It makes it unmeetable, and a gate that fails for want of
//! evidence reads exactly like a gate that fails for being wrong. So
//! this is asserted for every pack, on every eval set, in the same unit
//! `confident_wrong_distinct` scores.

use runner::eval::fixture::{fixtures_in, EvalSet};
use runner::eval::{decisions_needed, HarmClass};
use runner::packs::load_pack;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn packs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
}

/// Every shipped pack directory, so a pack cannot be added without
/// inheriting this rule.
fn every_pack() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(packs_dir())
        .expect("packs directory")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            path.join("pack.json").is_file().then_some(path)
        })
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no packs found to size");
    dirs
}

/// The ceilings whose beds are known to be too small, with the distinct
/// decisions each had on **1 August 2026**, when #310 first counted them
/// in this unit.
///
/// A deliberate exception goes in a list with a reason and a date, never
/// silently — the rule `STAGED_GOVUK_COMPONENTS` follows. Every entry
/// here is a claim Kettle currently cannot demonstrate, and none is
/// being asserted: the gate reports `UNPROVEN`, not `PASS`. Growing a
/// bed removes its entries, and the counts are a ratchet — a staged bed
/// that shrinks still fails.
///
/// The letter pack is no longer here at all: #315 declared the ceiling
/// its bed could carry and grew the no-obligation evidence from 23
/// distinct passages to 76. What remains is the subscription pack.
///
/// Shortfalls, in decisions still to be authored:
///   - subscription pooled: 32 -> 73 subscriptions, 60 -> 73 negatives
///   - subscription harm strata: 8 -> 73, each of three, both sets
///
/// The subscription list is ten cells rather than thirty-eight because
/// of #316: the pack gates pooled on `any-statement`, keeping a
/// per-stratum ceiling only where a confident denial is the harm the
/// stratum was built to catch. The other sixteen strata still slice the
/// results, they just no longer gate them.
///
/// The counts below are per set, and the two sets no longer share a
/// merchant (#317, 2 August 2026): each pattern's brand list is split in
/// half, so growing the bed now costs twice what it looks like from one
/// column — a merchant authored for development does nothing for the
/// exam. That is the price of an exam that is actually held out, and
/// `the_two_sets_do_not_share_a_decision` keeps it paid.
///
/// Tracked as bed-growth work under #237. This list reaching zero is
/// what "the bed supports its claims" means.
const STAGED_SHORT_BEDS: &[(&str, &str, &str, usize)] = &[
    (
        "app.kttl.subscription-audit",
        "Development",
        "any-statement/Subscription",
        32,
    ),
    (
        "app.kttl.subscription-audit",
        "Development",
        "any-statement/NotSubscription",
        60,
    ),
    (
        "app.kttl.subscription-audit",
        "Exam",
        "any-statement/Subscription",
        32,
    ),
    (
        "app.kttl.subscription-audit",
        "Exam",
        "any-statement/NotSubscription",
        60,
    ),
    (
        "app.kttl.subscription-audit",
        "Development",
        "annual-subscription-once-yearly/Subscription",
        8,
    ),
    (
        "app.kttl.subscription-audit",
        "Development",
        "free-trial-conversion/Subscription",
        8,
    ),
    (
        "app.kttl.subscription-audit",
        "Development",
        "price-rise-mid-series/Subscription",
        8,
    ),
    (
        "app.kttl.subscription-audit",
        "Exam",
        "annual-subscription-once-yearly/Subscription",
        8,
    ),
    (
        "app.kttl.subscription-audit",
        "Exam",
        "free-trial-conversion/Subscription",
        8,
    ),
    (
        "app.kttl.subscription-audit",
        "Exam",
        "price-rise-mid-series/Subscription",
        8,
    ),
];

#[test]
fn every_declared_ceiling_has_the_distinct_decisions_it_needs() {
    let mut failures: Vec<String> = Vec::new();
    let mut staged_seen: BTreeSet<(String, String, String)> = BTreeSet::new();

    for dir in every_pack() {
        let pack = load_pack(&dir).unwrap_or_else(|e| panic!("{} loads: {e}", dir.display()));
        let fixtures = fixtures_in(&pack).expect("fixtures load");

        for set in [EvalSet::Development, EvalSet::Exam] {
            let in_set: Vec<_> = fixtures
                .iter()
                .filter(|fixture| fixture.expected.eval_set == set)
                .collect();
            if in_set.is_empty() {
                continue;
            }

            for (stratum, declaration) in &pack.manifest.eval_strata {
                for (class, ceiling) in &declaration.classes {
                    // The decision key, exactly as the scorer writes it:
                    // a merchant's canonical name, or a passage folded
                    // the way passages are compared.
                    let mut decisions: BTreeSet<String> = BTreeSet::new();
                    for fixture in &in_set {
                        let expected = &fixture.expected;
                        for item in &expected.classify {
                            if !item.strata.iter().any(|s| s == stratum) {
                                continue;
                            }
                            if (item.kind == "subscription")
                                != matches!(class, HarmClass::Subscription)
                            {
                                continue;
                            }
                            decisions.insert(item.name.to_lowercase());
                        }
                        let folded = |segment: &str| {
                            segment
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .to_lowercase()
                        };
                        for item in &expected.obligations {
                            if !item.strata.iter().any(|s| s == stratum) {
                                continue;
                            }
                            if item.expect.is_some() != matches!(class, HarmClass::Obligation) {
                                continue;
                            }
                            decisions.insert(folded(&item.segment));
                        }
                        // A comparison's passages (#356). Without this
                        // the count is zero and every ceiling reads as
                        // unsupportable — the same silence as counting
                        // a block the harness has not been told about,
                        // which is how the letter bed's rows once
                        // passed for decisions (#310).
                        for item in &expected.policy_terms {
                            if !item.strata.iter().any(|s| s == stratum) {
                                continue;
                            }
                            if item.expect.is_some() != matches!(class, HarmClass::Obligation) {
                                continue;
                            }
                            // Role-qualified, exactly as the scorer
                            // keys it: the same sentence in both
                            // documents is two decisions, not one.
                            decisions.insert(format!("{}|{}", item.role, folded(&item.segment)));
                        }
                    }

                    let needed = decisions_needed(ceiling.max_wilson_95);
                    let have = decisions.len();
                    let cell = format!("{stratum}/{class:?}");
                    let set_name = format!("{set:?}");
                    let staged =
                        STAGED_SHORT_BEDS
                            .iter()
                            .find(|(pack_id, staged_set, staged_cell, _)| {
                                *pack_id == pack.manifest.id
                                    && *staged_set == set_name
                                    && *staged_cell == cell
                            });
                    if staged.is_some() {
                        staged_seen.insert((
                            pack.manifest.id.clone(),
                            set_name.clone(),
                            cell.clone(),
                        ));
                    }

                    match (have >= needed, staged) {
                        // Carries its own evidence, and nobody claimed
                        // otherwise.
                        (true, None) => {}
                        // Grown past its staging: a stale exception is
                        // how a list like this stops meaning anything.
                        (true, Some(_)) => failures.push(format!(
                            "{} {set_name} {cell} now has {have} distinct decisions and meets \
                             its {needed} — remove it from STAGED_SHORT_BEDS",
                            pack.manifest.id
                        )),
                        // Known short. Allowed, never allowed to shrink.
                        (false, Some((_, _, _, recorded))) if have >= *recorded => {}
                        (false, Some((_, _, _, recorded))) => failures.push(format!(
                            "{} {set_name} {cell} has {have} distinct decisions, down from the \
                             {recorded} staged on 1 Aug 2026 — a bed under a declared ceiling \
                             must not shrink",
                            pack.manifest.id
                        )),
                        (false, None) => failures.push(format!(
                            "{} {set_name} {cell}: {have} distinct decisions, but its {:.0}% \
                             ceiling needs {needed} before a run with no errors at all could \
                             clear it. Grow the bed, or declare a ceiling this bed can carry — \
                             and if it must wait, stage it in STAGED_SHORT_BEDS with today's \
                             count",
                            pack.manifest.id,
                            ceiling.max_wilson_95 * 100.0,
                        )),
                    }
                }
            }
        }
    }

    // A staged entry naming a cell no pack declares any more is dead
    // weight, and dead weight hides a real shortfall behind a familiar
    // name.
    assert_eq!(
        staged_seen.len(),
        STAGED_SHORT_BEDS.len(),
        "STAGED_SHORT_BEDS names {} cells but only {} still exist — remove the rest",
        STAGED_SHORT_BEDS.len(),
        staged_seen.len(),
    );

    assert!(
        failures.is_empty(),
        "{} declared ceilings cannot be met by their bed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// #317: the sealed set must be held out on the axis the pack is on
/// trial for, not merely on the shape of the file it arrives in.
///
/// The subscription bed's two sets drew on the **same 92 merchants** —
/// not an overlapping majority, the two name sets were equal. So a 7B
/// that had seen `Backblaze` in development had seen it in the exam,
/// and the exam could not catch overfitting to the merchant list, which
/// is the axis #257 rebuilt the bed to measure. It could still catch
/// overfitting to statement shape, ordering and descriptor noise, which
/// is exactly why the gap was easy to miss: a shared-merchant exam
/// passes, and reads like an exam that generalised.
///
/// Asserted on the decision key the scorer uses, so this cannot drift
/// from what a ceiling is actually judged on: a merchant's canonical
/// name, or a passage folded the way passages are compared. The letter
/// pack has always passed this — #299 separated the two voices and #300
/// stopped the letters repeating — and it passes on both keys, which is
/// why the rule is worth stating for every pack rather than for the one
/// that broke it.
#[test]
fn the_two_sets_do_not_share_a_decision() {
    let mut failures: Vec<String> = Vec::new();

    for dir in every_pack() {
        let pack = load_pack(&dir).unwrap_or_else(|e| panic!("{} loads: {e}", dir.display()));
        let fixtures = fixtures_in(&pack).expect("fixtures load");

        let decisions = |set: EvalSet| {
            let mut merchants: BTreeSet<String> = BTreeSet::new();
            let mut passages: BTreeSet<String> = BTreeSet::new();
            for fixture in fixtures
                .iter()
                .filter(|fixture| fixture.expected.eval_set == set)
            {
                for item in &fixture.expected.classify {
                    merchants.insert(item.name.to_lowercase());
                }
                // Every passage-shaped expectation, whichever role
                // answers it. A block this loop did not know about
                // would let a pack hold out nothing and pass — the
                // failure is silent, which is the whole reason #317
                // exists.
                let passage_of = |segment: &str| {
                    segment
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                };
                for item in &fixture.expected.obligations {
                    passages.insert(passage_of(&item.segment));
                }
                for item in &fixture.expected.policy_terms {
                    // Role-qualified, as the scorer keys it (#356): the
                    // same sentence in both documents is two decisions.
                    passages.insert(format!("{}|{}", item.role, passage_of(&item.segment)));
                }
            }
            (merchants, passages)
        };

        let (dev_merchants, dev_passages) = decisions(EvalSet::Development);
        let (exam_merchants, exam_passages) = decisions(EvalSet::Exam);

        // A pack whose bed declares only one set has nothing to hold
        // out, and nothing to assert.
        if dev_merchants.is_empty() && dev_passages.is_empty() {
            continue;
        }
        if exam_merchants.is_empty() && exam_passages.is_empty() {
            continue;
        }

        for (key, development, exam) in [
            ("merchant", &dev_merchants, &exam_merchants),
            ("passage", &dev_passages, &exam_passages),
        ] {
            let shared: Vec<&String> = development.intersection(exam).collect();
            if shared.is_empty() {
                continue;
            }
            let sample: Vec<String> = shared
                .iter()
                .take(3)
                .map(|s| s.chars().take(48).collect())
                .collect();
            failures.push(format!(
                "{}: development and exam share {} of {} {key} decisions (e.g. {}) — the exam \
                 cannot catch overfitting to something it also taught",
                pack.manifest.id,
                shared.len(),
                development.len(),
                sample.join(", "),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} sealed set(s) are not held out:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// #456: every stratum a committed bed plants is declared in its pack.
///
/// `validate_declared_strata` has always been correct and has never been
/// pointed at the real beds — its only tests build fixtures by hand. So
/// the whole test suite passed while the letter bed was unrunnable:
/// #456's shape planted `passive-obligation`, `passive-voice` and
/// `passive-no-obligation`, none of them declared, and the refusal
/// arrives at fixture load — after a CUDA sidecar had been vendored, on
/// a rented GPU, ten minutes into a job that had already cost an
/// afternoon.
///
/// The rule this encodes: a check that only ever runs against synthetic
/// input is not protecting the artefact it was written for. This is the
/// cheapest possible place to fail — before a model is ever asked
/// anything — and it belongs to every pack, so the next bed to grow a
/// stratum inherits it.
#[test]
fn every_stratum_a_bed_plants_is_declared_by_its_pack() {
    let mut failures: Vec<String> = Vec::new();
    for dir in every_pack() {
        let pack = load_pack(&dir).expect("pack loads");
        let fixtures = fixtures_in(&pack).expect("fixtures load");
        if let Err(problem) =
            runner::eval::fixture::validate_declared_strata(&fixtures, &pack.manifest.eval_strata)
        {
            failures.push(format!("{}: {problem}", pack.manifest.id));
        }
    }
    assert!(
        failures.is_empty(),
        "{} pack(s) plant a stratum they do not declare:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
