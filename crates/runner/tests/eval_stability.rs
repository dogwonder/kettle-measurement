//! What `--runs 3` found out (#83).
//!
//! Brief §6 says the flag exists to confirm stability:
//! grammar-constrained answers at temperature 0 should be
//! near-deterministic, so *any* spread is itself the red flag. These
//! tests pin the two claims that follow from that — a spread is
//! recorded rather than averaged away, and "any" means any.

use runner::eval::{Spread, Stability};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn runs_that_agreed_have_not_moved() {
    let spread = Spread::across([0.95, 0.95, 0.95]);

    assert_eq!(spread.low, 0.95);
    assert_eq!(spread.high, 0.95);
    assert!(!spread.moved());
}

#[test]
fn a_spread_keeps_the_lowest_and_the_highest() {
    let spread = Spread::across([1.0, 1.0, 0.85]);

    assert_eq!(spread.low, 0.85);
    assert_eq!(spread.high, 1.0);
    assert!(spread.moved());
}

/// The averaging trap the issue names: 0.95/0.95/0.95 and
/// 1.00/1.00/0.85 have the same mean, and only one of them is a model
/// anyone should recommend.
#[test]
fn two_run_sets_with_the_same_mean_are_told_apart() {
    let steady = Spread::across([0.95, 0.95, 0.95]);
    let wobbly = Spread::across([1.0, 1.0, 0.85]);

    assert!(!steady.moved());
    assert!(wobbly.moved());
}

/// Not "a large spread" — any spread. At temperature 0 against a
/// grammar there is no such thing as an acceptable amount of drift, so
/// there is no band here to argue about.
#[test]
fn the_smallest_real_disagreement_still_counts() {
    let spread = Spread::across([0.95, 0.95, 0.96]);

    assert!(
        spread.moved(),
        "a band would only ever be a judgement about how much silent drift is \
         tolerable, and the honest answer is none"
    );
}

/// One run cannot disagree with itself.
#[test]
fn a_single_value_has_not_moved() {
    let spread = Spread::across([0.88]);

    assert_eq!(spread.low, 0.88);
    assert_eq!(spread.high, 0.88);
    assert!(!spread.moved());
}

#[test]
fn stability_has_moved_when_anything_in_it_has() {
    let steady = Stability {
        runs: 3,
        steps: BTreeMap::from([("normalise".to_owned(), Spread::across([1.0, 1.0, 1.0]))]),
        end_to_end: Spread::across([0.96, 0.96, 0.96]),
        needs_review_rate: Spread::across([0.12, 0.12, 0.12]),
        record_digests: BTreeSet::new(),
    };
    assert!(!steady.moved());

    // A step that wobbled.
    let mut wobbly = steady.clone();
    wobbly
        .steps
        .insert("normalise".to_owned(), Spread::across([1.0, 0.9, 1.0]));
    assert!(wobbly.moved());

    // The review bucket wobbling is the same finding: the run is not
    // reproducible, whatever the scores did.
    let mut restless = steady.clone();
    restless.needs_review_rate = Spread::across([0.12, 0.12, 0.20]);
    assert!(
        restless.moved(),
        "the same statement landing differently in front of a person is a \
         stability finding even when every score holds"
    );
}

#[test]
fn a_stability_block_survives_a_round_trip_through_json() {
    let stability = Stability {
        runs: 3,
        steps: BTreeMap::from([("classify".to_owned(), Spread::across([0.9, 0.8, 0.9]))]),
        end_to_end: Spread::across([1.0, 0.95, 1.0]),
        needs_review_rate: Spread::across([0.1, 0.1, 0.1]),
        record_digests: BTreeSet::from(["blake3:one".to_owned()]),
    };

    let json = serde_json::to_string(&stability).expect("serialise");
    let back: Stability = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back, stability);
    assert!(back.moved());
}

/// The named spreads are a list of the quantities somebody remembered
/// to watch, and the harm ceiling is not on it (#533). `steps`,
/// `end_to_end` and `needs_review_rate` say nothing about `items`,
/// `containment` or the calibration buckets — and `confident_wrong` is
/// computed from `items`. So a repeat could move in exactly the number
/// a ceiling is a ceiling on while every spread held, the harness would
/// report stable, and the registry check that withdraws a claim on
/// moved evidence would never fire.
///
/// The digest makes "did the repeats agree?" total rather than
/// enumerated: one entry means every repeat recorded the same thing
/// about this fixture, whatever anyone thought to watch.
#[test]
fn a_repeat_that_moved_only_where_no_spread_looks_still_counts_as_moved() {
    let steady = Stability {
        runs: 3,
        steps: BTreeMap::from([("obligations".to_owned(), Spread::across([0.98, 0.98, 0.98]))]),
        end_to_end: Spread::across([0.97, 0.97, 0.97]),
        needs_review_rate: Spread::across([0.0, 0.0, 0.0]),
        record_digests: BTreeSet::from(["blake3:aaa".to_owned()]),
    };
    assert!(!steady.moved(), "one digest is every repeat agreeing");

    // Every named spread holds. The run recorded something different
    // anyway — an item flipping from wrong-and-unsure to
    // wrong-and-confident moves the ceiling and not the score.
    let mut divergent = steady.clone();
    divergent.record_digests = BTreeSet::from(["blake3:aaa".to_owned(), "blake3:bbb".to_owned()]);
    assert!(
        divergent.moved(),
        "repeats that recorded different things disagreed, whatever the \
         spreads say"
    );
}
