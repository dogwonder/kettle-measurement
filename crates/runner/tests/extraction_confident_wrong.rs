//! #452: what a confident-wrong assertion *is*.
//!
//! The harm lens used to ask whole-struct inequality of an extracted
//! payload, so it counted fields the rest of the harness deliberately
//! excludes from its joins — `anchor` on an obligation, `quote` on a
//! term. The first v11 measurements failed both obligation gates on
//! them: every letter obligation found and every `due` correct, beside
//! a confident-wrong rate of 0.22, and 128 of 294 renewal terms
//! differing from the authored sentence by a trailing full stop.
//!
//! An assertion is wrong when it says something different about what
//! the person acts on. These tests pin both halves of that: the
//! rewordings must not count, and a field that genuinely changes the
//! answer must still count.

use runner::eval::{
    extraction_metrics, ExpectedObligation, ExpectedTerm, Extracted, ExtractionOutcome, HarmClass,
    ScoredDecision, ScoredItem,
};

fn scored(expected: Extracted, actual: Extracted) -> ScoredItem {
    ScoredItem {
        id: "app.kttl.test/fixture-01/item-01".to_owned(),
        item_id: "item-01".to_owned(),
        pack: "app.kttl.test".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:test".to_owned(),
        fixture: "fixture.txt".to_owned(),
        fixture_id: "fixture-01".to_owned(),
        strata: vec!["any".to_owned()],
        raw_input: "PASSAGE 1".to_owned(),
        decision_key: "PASSAGE 1".to_owned(),
        decision: ScoredDecision::Extraction {
            expected: Some(expected),
            expected_review: false,
            unauthored_negative: false,
            actual: ExtractionOutcome::Found { extracted: actual },
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: Vec::new(),
    }
}

/// How many of these decisions the lens calls confidently wrong about
/// the class that carries a value.
fn confidently_wrong(expected: Extracted, actual: Extracted) -> usize {
    let metrics = extraction_metrics(&[scored(expected, actual)]);
    metrics.overall.harm_classes[&HarmClass::Obligation]
        .confident_wrong_rate
        .successes
}

fn obligation(deadline: &str, anchor: &str, due: Option<&str>) -> Extracted {
    Extracted::Obligation(ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Harborne Parking Services".to_owned(),
        deadline: deadline.to_owned(),
        anchor: anchor.to_owned(),
        due: due.map(|d| d.parse().expect("an authored date")),
    })
}

fn term(value: &str, quote: &str) -> Extracted {
    Extracted::Term(ExpectedTerm {
        term: "total_annual_premium".to_owned(),
        basis: "per_year".to_owned(),
        value: value.to_owned(),
        quote: quote.to_owned(),
    })
}

/// The letter case, 111 of 462 items in the first v11 run. The anchor's
/// only job is to say which date a relative deadline counts from
/// (`timeline::resolve_deadline`); neither of these names one, so both
/// count from the letter's own date and the person reads the same day.
/// It is not shown to them either — no report template renders it.
#[test]
fn a_reworded_undated_anchor_is_not_a_wrong_obligation() {
    assert_eq!(
        confidently_wrong(
            obligation(
                "within 14 days",
                "the date of this letter",
                Some("2026-03-17")
            ),
            obligation("within 14 days", "14 days", Some("2026-03-17")),
        ),
        0,
        "same deadline, same due date, a differently worded undated anchor"
    );
}

/// The other half. An anchor that names a date is doing work — it is
/// what the arithmetic counts from — so naming the wrong one is a wrong
/// assertion. Since scoring version 15 (#552) it is measured where the
/// work shows: `timeline::resolve_deadline` counts from the anchor, so
/// a wrong anchor is a wrong `due`, and `due` is what the person acts
/// on. The v12 form of this test paired "8 June" with a due date of
/// 15 June — a pair the resolver cannot produce — and measured the
/// anchor's words instead, which is the disagreement with the pooled
/// join that #552 found.
#[test]
fn an_anchor_naming_a_different_date_is_still_a_wrong_obligation() {
    assert_eq!(
        confidently_wrong(
            obligation(
                "within 14 days",
                "the hearing on 1 June 2026",
                Some("2026-06-15")
            ),
            obligation(
                "within 14 days",
                "the hearing on 8 June 2026",
                Some("2026-06-22")
            ),
        ),
        1,
        "a dated anchor is what the deadline counts from, so the day moves with it"
    );
    // An anchor the arithmetic never used — the deadline named its own
    // day — reaches nobody: no report renders the phrase (#452), and
    // the pooled join never keyed on it (#287).
    assert_eq!(
        confidently_wrong(
            obligation(
                "by 15 June 2026",
                "the hearing on 1 June 2026",
                Some("2026-06-15")
            ),
            obligation(
                "by 15 June 2026",
                "the hearing on 8 June 2026",
                Some("2026-06-15")
            ),
        ),
        0,
        "an absolute deadline counts from nothing, so its anchor asserts nothing"
    );
}

/// A deadline that resolves to a different day is the harm this gate
/// exists for, anchor or no anchor.
#[test]
fn a_different_due_date_is_still_a_wrong_obligation() {
    assert_eq!(
        confidently_wrong(
            obligation(
                "within 14 days",
                "the date of this letter",
                Some("2026-03-17")
            ),
            obligation(
                "within 14 days",
                "the date of this letter",
                Some("2026-03-31")
            ),
        ),
        1,
    );
}

/// The renewal case, 128 of 294 found terms in the first v11 run. The
/// run's own quote guardrail already found each of these verbatim in
/// the source, which is what #258 asks of a quote; it is the bed that
/// demanded string equality with the authored sentence.
#[test]
fn a_quote_differing_by_a_trailing_full_stop_is_not_a_wrong_term() {
    assert_eq!(
        confidently_wrong(
            term("£1,880.00", "Total annual premium: £1,880.00."),
            term("£1,880.00", "Total annual premium: £1,880.00"),
        ),
        0,
        "the same passage, one full stop shorter"
    );
    assert_eq!(
        confidently_wrong(
            term("£240.00", "Compulsory excess: £240.00 per claim."),
            term("£240.00", "£240.00 per claim"),
        ),
        0,
        "the same passage, without the label prefix"
    );
}

/// Containment, not similarity. A quote that is not the authored
/// passage at all is a term read out of the wrong sentence, which is
/// exactly the mistake that turns a rise into a cut.
#[test]
fn a_quote_from_another_sentence_is_still_a_wrong_term() {
    assert_eq!(
        confidently_wrong(
            term("£1,880.00", "Total annual premium: £1,880.00."),
            term("£1,880.00", "Last year's premium was £1,650.00."),
        ),
        1,
    );
}

/// An empty quote supports nothing, so it can never stand in for one
/// that does — the containment rule must not read "" as inside every
/// passage.
#[test]
fn an_empty_quote_is_still_a_wrong_term() {
    assert_eq!(
        confidently_wrong(
            term("£1,880.00", "Total annual premium: £1,880.00."),
            term("£1,880.00", ""),
        ),
        1,
    );
}

/// The value is the assertion. Nothing about the leniency above may
/// reach the fields the pooled score joins on.
#[test]
fn a_different_value_is_still_a_wrong_term() {
    assert_eq!(
        confidently_wrong(
            term("£1,880.00", "Total annual premium: £1,880.00."),
            term("£1,650.00", "Total annual premium: £1,880.00."),
        ),
        1,
    );
}

/// Two shapes are never the same assertion, whatever their fields say.
#[test]
fn a_term_asserted_where_an_obligation_was_expected_is_wrong() {
    assert_eq!(
        confidently_wrong(
            obligation("within 14 days", "the date of this letter", None),
            term("£1,880.00", "Total annual premium: £1,880.00."),
        ),
        1,
    );
}
