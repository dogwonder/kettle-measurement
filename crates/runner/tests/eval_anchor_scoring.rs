//! #287: an obligation is scored on the date it resolves to, not on the
//! anchor's wording.
//!
//! The anchor reaches no person — it appears in neither the report
//! template nor `letter_report` — and `timeline::resolve_deadline` reads
//! only a *date* inside it, falling back to the letter's own date when
//! there is none. So two dateless anchors are the same input, and
//! charging a run for preferring one wording measured our ambiguity
//! rather than the model's reading (#242's first measurement: 67 of 93
//! imperfect extractions were this one disagreement).
//!
//! What is given up is stated plainly in the issue and accepted: a
//! dateless anchor that is *wrong* ("the invoice date") is no longer
//! visible to the bed, because it cannot change the date a person sees.
//! An anchor carrying a date still is — get that wrong and `due` moves.

use chrono::NaiveDate;
use runner::claim::Kind;
use runner::eval::fixture::{score_fixture, Expected};
use runner::eval::Perf;
use runner::run::{ExtractionOutcome, InputSeen, Obligation, Payload, RunOutcome};
use runner::timeline::Resolved;

fn date(s: &str) -> NaiveDate {
    s.parse().expect("date")
}

fn perf() -> Perf {
    Perf {
        wall_ms: 0,
        model_ms: 0,
        tokens_per_second: 0.0,
        peak_rss_mb: 0,
    }
}

fn found(anchor: &str, due: Option<NaiveDate>) -> RunOutcome {
    RunOutcome {
        input: InputSeen {
            rows: 0,
            period: None,
        },
        // Built directly for scoring; no input was run through it.
        inputs: Vec::new(),
        needs_review: Vec::new(),
        warnings: Vec::new(),
        claim_traces: Vec::new(),
        payload: Payload::Extraction(ExtractionOutcome {
            date_disputes: vec![],
            obligations: vec![Obligation {
                kind: "payment".to_owned(),
                party: "Denholm Veterinary Group".to_owned(),
                ask: "Settle the balance".to_owned(),
                deadline: "by the end of the month".to_owned(),
                anchor: anchor.to_owned(),
                amount: "no amount".to_owned(),
                confidence: "high".to_owned(),
                // A month-end deadline is always Rust's arithmetic;
                // scoring joins on the date, never on the kind.
                due: due.map(|date| Resolved {
                    date,
                    kind: Kind::WorkedOut,
                }),
                evidence: Vec::new(),
                dated_by: None,
                priced_by: None,
                disputed: vec![],
            }],
        }),
    }
}

/// The bed's own wording, for reference: what `expected.json` asserts.
fn expected() -> Expected {
    serde_json::from_str(
        r#"{
            "fixture_id": "development-payment_month_end-amber-21",
            "obligations": [
                {
                    "id": "payment_month_end-amber-21-payment-01",
                    "segment": "The balance on your pet health plan is £54.00. Please settle it by the end of the month.",
                    "strata": ["month-end"],
                    "expect": {
                        "kind": "payment",
                        "party": "Denholm Veterinary Group",
                        "deadline": "by the end of the month",
                        "anchor": "the date of this letter",
                        "due": "2026-09-30"
                    }
                }
            ]
        }"#,
    )
    .expect("expected.json")
}

/// The real disagreement from #242's measurement, on a letter dated
/// 7 September 2026: the bed says the month is counted from the letter's
/// date, the model restates the deadline. Both resolve to 30 September,
/// so the person sees the same due date and nothing is wrong.
#[test]
fn an_anchor_wording_that_resolves_to_the_same_date_is_not_an_error() {
    let result = score_fixture(
        "generated-development-payment_month_end-amber-21.txt",
        &expected(),
        &found("the end of the month", Some(date("2026-09-30"))),
        perf(),
    );

    assert_eq!(
        result.step_scores["obligations"].score, 1.0,
        "the resolved date matches, so the obligation was read correctly"
    );
}

/// The other half, and the reason this is not simply loosening the bed:
/// an anchor that moves the date is still an error. Here the model read
/// the month wrongly and a person would diary the wrong day.
#[test]
fn an_anchor_that_moves_the_resolved_date_is_still_an_error() {
    let result = score_fixture(
        "generated-development-payment_month_end-amber-21.txt",
        &expected(),
        &found("the end of the month", Some(date("2026-10-31"))),
        perf(),
    );

    assert_eq!(
        result.step_scores["obligations"].score, 0.0,
        "a different due date is a different obligation, however it was worded"
    );
}
