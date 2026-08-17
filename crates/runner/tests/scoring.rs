//! Eval scoring (#36). Pure functions, exhaustively tested: the harness
//! that judges a model must itself be beyond doubt.
//!
//! Tolerances come from `expected.json`: normalise is fuzzy and
//! case-insensitive ("Amazon marketplace" isn't wrong), enums are exact,
//! deterministic steps are exact. Scoring joins on `raw` (normalise) and
//! `name` (classify) — never on batch ids, which are synthetic per run.

use runner::scoring::{enum_accuracy, fuzzy_match, keyed_accuracy, set_f1, similarity, Tolerance};
use std::str::FromStr;

// --- similarity / fuzzy_match ------------------------------------------

#[test]
fn identical_names_are_perfectly_similar() {
    assert_eq!(similarity("Netflix", "Netflix"), 1.0);
}

#[test]
fn similarity_ignores_case_and_surrounding_space() {
    // The model differing only in case has not got the answer wrong.
    assert_eq!(similarity("Amazon Marketplace", "amazon marketplace"), 1.0);
    assert_eq!(similarity("Netflix", "  netflix  "), 1.0);
}

#[test]
fn unrelated_names_score_low() {
    assert!(similarity("Netflix", "British Gas") < 0.5);
}

#[test]
fn fuzzy_match_accepts_near_misses_and_rejects_wrong_merchants() {
    // A near miss on the same merchant clears the pack's 0.85 bar.
    assert!(fuzzy_match(
        "Amazon Marketplace",
        "Amazon marketplace",
        0.85
    ));
    assert!(fuzzy_match("PureGym", "Pure Gym", 0.85));

    // A different merchant does not, however plausible the output.
    assert!(!fuzzy_match("Netflix", "Spotify", 0.85));
}

#[test]
fn brands_sharing_a_first_word_are_not_the_same_merchant() {
    // The trap: a prefix-weighted metric (Jaro-Winkler, as used for
    // merchant *grouping*) scores these ~0.92 and would credit a model
    // for answering "British Airways" to a British Gas charge.
    assert!(!fuzzy_match("British Gas", "British Airways", 0.85));
    assert!(!fuzzy_match("Virgin Media", "Virgin Active", 0.85));
    assert!(!fuzzy_match("Tesco", "Tesco Express", 0.85));
}

#[test]
fn punctuation_and_ampersands_are_not_a_wrong_answer() {
    assert!(fuzzy_match(
        "Amazon Marketplace",
        "Amazon.Marketplace",
        0.85
    ));
    assert!(fuzzy_match("Marks & Spencer", "Marks and Spencer", 0.85));
}

#[test]
fn a_transposed_typo_is_still_the_right_merchant() {
    assert!(fuzzy_match("Netflix", "Netfilx", 0.85));
}

#[test]
fn an_unstripped_suffix_is_a_wrong_answer() {
    // The normalise step exists to remove these; scoring must not
    // forgive the model for leaving them on.
    assert!(!fuzzy_match("British Gas", "British Gas Ltd", 0.85));
}

#[test]
fn fuzzy_match_is_inclusive_at_the_threshold() {
    let a = "Kaffa Coffee";
    let b = "Kaffa Coffe";
    let exact_threshold = similarity(a, b);
    assert!(
        fuzzy_match(a, b, exact_threshold),
        "similarity == threshold passes"
    );
    assert!(!fuzzy_match(a, b, exact_threshold + 0.000_1));
}

// --- Tolerance ----------------------------------------------------------

#[test]
fn tolerances_parse_from_the_expected_json_spellings() {
    assert_eq!(Tolerance::from_str("exact").unwrap(), Tolerance::Exact);
    assert_eq!(
        Tolerance::from_str("fuzzy:0.85").unwrap(),
        Tolerance::Fuzzy(0.85)
    );
}

#[test]
fn unparseable_tolerances_are_rejected_not_guessed() {
    // Silently defaulting would score a run against a tolerance nobody wrote.
    assert!(Tolerance::from_str("fuzzy").is_err());
    assert!(Tolerance::from_str("fuzzy:").is_err());
    assert!(Tolerance::from_str("fuzzy:tight").is_err());
    assert!(Tolerance::from_str("approximate").is_err());
    assert!(Tolerance::from_str("").is_err());
}

#[test]
fn exact_tolerance_is_case_sensitive() {
    // Enums come out of a JSON-Schema-constrained grammar; a case
    // difference means the grammar leaked, and we want to hear about it.
    assert!(Tolerance::Exact.matches("subscription", "subscription"));
    assert!(!Tolerance::Exact.matches("subscription", "Subscription"));
    assert!(!Tolerance::Exact.matches("subscription", "regular_spend"));
}

#[test]
fn exact_tolerance_still_forgives_surrounding_space() {
    assert!(Tolerance::Exact.matches("streaming", " streaming "));
}

#[test]
fn fuzzy_tolerance_applies_its_threshold() {
    assert!(Tolerance::Fuzzy(0.85).matches("Amazon Marketplace", "amazon marketplace"));
    assert!(!Tolerance::Fuzzy(0.85).matches("Netflix", "Spotify"));
}

// --- keyed_accuracy -----------------------------------------------------

#[test]
fn normalise_scores_by_raw_string_under_fuzzy_tolerance() {
    let expected = [
        ("NETFLIX.COM", "Netflix"),
        ("SPOTIFY LTD", "Spotify"),
        ("PUREGYM LTD", "PureGym"),
        ("BRITISH GAS", "British Gas"),
    ];
    let actual = [
        ("NETFLIX.COM", "Netflix"),
        ("SPOTIFY LTD", "spotify"),  // case only — correct
        ("PUREGYM LTD", "Pure Gym"), // spacing only — correct
        ("BRITISH GAS", "Centrica"), // wrong merchant
    ];

    assert_eq!(
        keyed_accuracy(&expected, &actual, Tolerance::Fuzzy(0.85)),
        0.75
    );
}

#[test]
fn a_missing_answer_scores_as_wrong_not_as_absent() {
    // A model that silently drops half the batch must not score 100%.
    let expected = [("NETFLIX.COM", "Netflix"), ("SPOTIFY LTD", "Spotify")];
    let actual = [("NETFLIX.COM", "Netflix")];

    assert_eq!(
        keyed_accuracy(&expected, &actual, Tolerance::Fuzzy(0.85)),
        0.5
    );
}

#[test]
fn answers_nobody_asked_for_do_not_inflate_the_score() {
    let expected = [("NETFLIX.COM", "Netflix")];
    let actual = [("NETFLIX.COM", "Netflix"), ("TESCO STORES 3412", "Tesco")];

    assert_eq!(
        keyed_accuracy(&expected, &actual, Tolerance::Fuzzy(0.85)),
        1.0
    );
}

#[test]
fn the_first_answer_for_a_key_is_the_one_scored() {
    // Order-stable, like the rest of the pipeline: a later duplicate
    // cannot overwrite a wrong answer into a right one.
    let expected = [("NETFLIX.COM", "Netflix")];
    let actual = [("NETFLIX.COM", "Spotify"), ("NETFLIX.COM", "Netflix")];

    assert_eq!(keyed_accuracy(&expected, &actual, Tolerance::Exact), 0.0);
}

#[test]
fn keys_are_joined_case_insensitively() {
    let expected = [("NETFLIX.COM", "Netflix")];
    let actual = [("netflix.com", "Netflix")];

    assert_eq!(keyed_accuracy(&expected, &actual, Tolerance::Exact), 1.0);
}

#[test]
fn expecting_nothing_is_scored_perfect() {
    // No claims to get wrong. Guards the 0/0 division.
    assert_eq!(keyed_accuracy(&[], &[], Tolerance::Exact), 1.0);
    assert_eq!(
        keyed_accuracy(&[], &[("TESCO", "Tesco")], Tolerance::Exact),
        1.0
    );
}

// --- enum_accuracy ------------------------------------------------------

#[test]
fn classify_kinds_are_scored_exactly_by_merchant_name() {
    let expected = [
        ("Netflix", "subscription"),
        ("PureGym", "subscription"),
        ("British Gas", "utility"),
        ("Tesco", "regular_spend"),
    ];
    let actual = [
        ("Netflix", "subscription"),
        ("PureGym", "regular_spend"), // the #81 miss
        ("British Gas", "utility"),
        ("Tesco", "regular_spend"),
    ];

    assert_eq!(enum_accuracy(&expected, &actual), 0.75);
}

#[test]
fn enum_accuracy_admits_no_near_misses() {
    // "streaming_video" is not "streaming"; enums are exact by contract.
    let expected = [("Netflix", "streaming")];
    let actual = [("Netflix", "streaming_video")];

    assert_eq!(enum_accuracy(&expected, &actual), 0.0);
}

// --- set_f1 -------------------------------------------------------------

#[test]
fn identical_sets_score_one() {
    let expected = ["Netflix|monthly|155.88|rise", "Spotify|monthly|143.88|flat"];
    assert_eq!(set_f1(&expected, &expected), 1.0);
}

#[test]
fn disjoint_sets_score_zero() {
    assert_eq!(set_f1(&["Netflix|monthly"], &["Spotify|annual"]), 0.0);
}

#[test]
fn a_miss_and_an_invention_are_both_penalised() {
    let expected = ["Netflix|monthly", "Spotify|monthly", "PureGym|monthly"];

    // Found 2 of 3, invented nothing: precision 1.0, recall 0.667 → F1 0.8.
    let missed_one = ["Netflix|monthly", "Spotify|monthly"];
    assert!((set_f1(&expected, &missed_one) - 0.8).abs() < 1e-6);

    // Found all 3 but invented one: precision 0.75, recall 1.0 → F1 ≈ 0.857.
    let invented_one = [
        "Netflix|monthly",
        "Spotify|monthly",
        "PureGym|monthly",
        "British Gas|quarterly",
    ];
    assert!((set_f1(&expected, &invented_one) - 6.0 / 7.0).abs() < 1e-6);
}

#[test]
fn a_repeated_answer_counts_once() {
    // Sets, not bags: emitting Netflix twice is not two hits.
    let expected = ["Netflix|monthly", "Spotify|monthly"];
    let actual = ["Netflix|monthly", "Netflix|monthly"];

    // 1 of 2 found, 1 distinct answer, both right → precision 1.0,
    // recall 0.5, F1 ≈ 0.667.
    assert!((set_f1(&expected, &actual) - 2.0 / 3.0).abs() < 1e-6);
}

#[test]
fn empty_sets_are_scored_without_dividing_by_zero() {
    assert_eq!(set_f1(&[], &[]), 1.0, "expected nothing, found nothing");
    assert_eq!(set_f1(&[], &["Netflix|monthly"]), 0.0, "all invention");
    assert_eq!(set_f1(&["Netflix|monthly"], &[]), 0.0, "found nothing");
}

#[test]
fn set_membership_is_exact() {
    // `recurring` is deterministic Rust: below 100% is a Rust bug, so
    // this comparison must never be softened into a fuzzy one.
    assert_eq!(
        set_f1(&["Netflix|monthly|155.88"], &["Netflix|monthly|155.8"]),
        0.0
    );
}
