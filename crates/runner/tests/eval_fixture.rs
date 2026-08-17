//! Scoring one fixture's run against its `expected.json` (#25) — the
//! half of the eval harness that needs a pipeline but not a model.
//!
//! The pipeline's own output is the thing scored: `RunOutcome` already
//! carries the raw merchant beside the normalised name, and the
//! classification beside both, so nothing here reaches into the run to
//! instrument it.

use runner::eval::fixture::{fixtures_at_with_retired, score_fixture, Expected};
use runner::eval::{classification_metrics, Perf};
use runner::recurrence::Period;
use runner::run::{AuditOutcome, Evidence, Finding, InputSeen, Payload, ReviewItem, RunOutcome};
use rust_decimal::Decimal;

fn perf() -> Perf {
    Perf {
        wall_ms: 0,
        model_ms: 0,
        tokens_per_second: 0.0,
        peak_rss_mb: 0,
        retries: 0,
    }
}

fn finding(raw: &str, merchant: &str, kind: &str, category: &str) -> Finding {
    Finding {
        merchant: merchant.to_owned(),
        raw_merchant: raw.to_owned(),
        kind: kind.to_owned(),
        // A helper's findings stand for detected series, which is the
        // branch the category map decides (#272).
        kind_from: runner::kinds::KindFrom::CategoryMap,
        category: category.to_owned(),
        confidence: "high".to_owned(),
        period: Period::Monthly,
        current_amount: Decimal::new(1299, 2),
        price_rise: None,
        evidence: vec![Evidence {
            date: "2026-01-04".parse().expect("date"),
            amount: Decimal::new(1299, 2),
        }],
    }
}

fn outcome(findings: Vec<Finding>) -> RunOutcome {
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
        payload: Payload::Audit(AuditOutcome {
            findings,
            ..AuditOutcome::default()
        }),
    }
}

/// The model turned "NETFLIX.COM" into the name `expected.json` asks
/// for, so `normalise` scored one out of one.
#[test]
fn scores_normalise_against_the_raw_merchant_the_statement_carried() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [],
            "recurring": [],
            "tolerances": { "normalise": "fuzzy:0.85" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    let normalise = result
        .step_scores
        .get("normalise")
        .expect("normalise scored");
    assert_eq!(normalise.expected, 1);
    assert_eq!(normalise.correct, 1);
    assert_eq!(normalise.score, 1.0);
}

/// Kind and category are reported separately, per class. A correct
/// classification therefore has full precision and surfaced recall in
/// both dimensions without being collapsed into accuracy.
#[test]
fn scores_classify_as_a_kind_and_a_category_per_merchant() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "clean-test-fixture-01",
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [
                { "id": "monthly-video-streaming-01", "strata": ["clean"],
                  "name": "Netflix", "kind": "subscription",
                  "category": "streaming" }
            ],
            "recurring": [],
            "tolerances": { "classify_kind": "exact",
                            "classify_category": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    let metrics = classification_metrics(&result.items);
    assert_eq!(
        metrics.overall.kinds["subscription"].precision.estimate,
        Some(1.0)
    );
    assert_eq!(
        metrics.overall.kinds["subscription"].recall.estimate,
        Some(1.0)
    );
    assert_eq!(
        metrics.overall.categories["streaming"].precision.estimate,
        Some(1.0)
    );
    assert_eq!(
        metrics.overall.categories["streaming"].recall.estimate,
        Some(1.0)
    );
    assert!(!result.step_scores.contains_key("classify"));
}

/// Right kind, wrong category remains visible as one correct class and
/// one silent miss; an accuracy mean must not blur the two.
#[test]
fn a_merchant_in_the_right_kind_but_the_wrong_category_is_half_right() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "clean-test-fixture-01",
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [
                { "id": "monthly-video-streaming-01", "strata": ["clean"],
                  "name": "Netflix", "kind": "subscription",
                  "category": "streaming" }
            ],
            "recurring": [],
            "tolerances": { "classify_kind": "exact",
                            "classify_category": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix",
            "subscription",
            "software",
        )]),
        perf(),
    );

    let metrics = classification_metrics(&result.items);
    assert_eq!(
        metrics.overall.kinds["subscription"].recall.estimate,
        Some(1.0)
    );
    assert_eq!(
        metrics.overall.categories["streaming"].recall.estimate,
        Some(0.0)
    );
    assert_eq!(
        metrics.overall.categories["software"].precision.estimate,
        Some(0.0)
    );
}

/// The model called it "DisneyPlus" and the fixture says "Disney+", but
/// it put it in exactly the right kind and category. That is one
/// mistake, in `normalise`, and it is charged once.
///
/// The pipeline routes answers to merchant groups by batch id, never by
/// name (`run::run_pack`), so a classification lands on the right
/// merchant whatever the model called it. The name is a label, not a
/// key — scoring it as one charged a single naming slip twice and made
/// a model that sorted every merchant correctly look like it had failed
/// at sorting.
#[test]
fn a_misnamed_merchant_still_gets_credit_for_being_sorted_correctly() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "messy-test-fixture-01",
            "normalise": [{ "raw": "PAYPAL *DISNEYPLUS", "name": "Disney+" }],
            "classify": [
                { "id": "disney-plus-messy-descriptor-01",
                  "strata": ["messy-merchant-strings"],
                  "name": "Disney+", "kind": "subscription",
                  "category": "streaming" }
            ],
            "recurring": [],
            "tolerances": { "normalise": "fuzzy:0.85",
                            "classify_kind": "exact",
                            "classify_category": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-02-messy.csv",
        &expected,
        &outcome(vec![finding(
            "PAYPAL *DISNEYPLUS",
            "DisneyPlus",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    assert_eq!(
        result.step_scores["normalise"].correct, 0,
        "the name is wrong, and normalise is where that is charged"
    );
    assert_eq!(
        classification_metrics(&result.items).overall.kinds["subscription"]
            .recall
            .estimate,
        Some(1.0),
        "the sorting is right, independently of the normalised name"
    );
}

/// `recurring` is deterministic Rust, so a matching series is the whole
/// point: merchant and cadence both right scores 1.0.
#[test]
fn end_to_end_scores_the_recurring_set_the_run_found() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [],
            "recurring": [{ "merchant": "Netflix", "period": "monthly" }],
            "tolerances": { "recurring": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    assert_eq!(result.end_to_end, 1.0);
}

/// The right merchant at the wrong cadence is not the right finding —
/// an annualised total built on it would be three times out. Cadence is
/// part of set membership precisely so this cannot score 1.0.
#[test]
fn a_series_found_at_the_wrong_cadence_is_not_a_match() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [],
            "recurring": [{ "merchant": "Netflix", "period": "quarterly" }],
            "tolerances": { "recurring": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    assert_eq!(result.end_to_end, 0.0);
}

fn review(raw: &str) -> ReviewItem {
    ReviewItem {
        subject: raw.to_owned(),
        reason: "Kettle didn't get an answer for this one, so it needs your eyes.".to_owned(),
        transactions: Vec::new(),
    }
}

/// Two merchants on the statement, one of which Kettle couldn't answer
/// for: half the statement went to a person.
#[test]
fn needs_review_rate_counts_the_merchants_parked_for_a_person() {
    let expected: Expected = serde_json::from_str(
        r#"{ "normalise": [], "classify": [], "recurring": [],
             "tolerances": {} }"#,
    )
    .expect("expected.json");

    let mut outcome = outcome(vec![finding(
        "NETFLIX.COM",
        "Netflix",
        "subscription",
        "streaming",
    )]);
    outcome.needs_review.push(review("SPOTIFY LTD"));

    let result = score_fixture("statement-01.csv", &expected, &outcome, perf());

    assert_eq!(result.needs_review_rate, 0.5);
}

/// A finding the model marked low-confidence is shown for checking
/// rather than trusted, so it is someone's work too — the rate counts
/// it alongside the merchants that failed outright.
#[test]
fn needs_review_rate_counts_low_confidence_findings_too() {
    let expected: Expected = serde_json::from_str(
        r#"{ "normalise": [], "classify": [], "recurring": [],
             "tolerances": {} }"#,
    )
    .expect("expected.json");

    let mut unsure = finding("SPOTIFY LTD", "Spotify", "subscription", "streaming");
    unsure.confidence = "low".to_owned();
    let outcome = outcome(vec![
        finding("NETFLIX.COM", "Netflix", "subscription", "streaming"),
        unsure,
    ]);

    let result = score_fixture("statement-01.csv", &expected, &outcome, perf());

    assert_eq!(result.needs_review_rate, 0.5);
}

/// Review is a deliberate third outcome. Keep the model's proposal for
/// diagnosis and count it as surfaced recall, but do not serialise it
/// as though Kettle asserted the classification.
#[test]
fn a_low_confidence_classification_is_recorded_as_needs_review() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "review-test-fixture-01",
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [{
                "id": "monthly-video-streaming-01",
                "strata": ["ambiguous-category"],
                "name": "Netflix",
                "kind": "subscription",
                "category": "streaming"
            }],
            "recurring": [],
            "tolerances": {}
        }"#,
    )
    .expect("expected.json");

    let mut unsure = finding("NETFLIX.COM", "Netflix", "subscription", "streaming");
    unsure.confidence = "low".to_owned();
    let mut run = outcome(vec![unsure]);
    run.needs_review.push(review("NETFLIX.COM"));

    let result = score_fixture("statement-review.csv", &expected, &run, perf());
    let item = &result.items[0];

    assert!(matches!(
        item.decision,
        runner::eval::ScoredDecision::Classification {
            actual: runner::eval::ClassificationOutcome::NeedsReview {
                proposed: Some(_),
                ..
            },
            ..
        }
    ));
    let performance = &classification_metrics(&result.items).overall.kinds["subscription"];
    assert_eq!(performance.recall.estimate, Some(1.0));
    assert_eq!(performance.precision.estimate, None);
    assert_eq!(performance.cells.expected_class_needs_review, 1);
}

/// The point of the end-to-end score: `recurring` is deterministic Rust
/// (CLAUDE.md), so it must not move when the *model* has a bad day. A
/// misnamed merchant costs `normalise` its mark and leaves recurrence
/// detection untouched — it found the series, at the right cadence,
/// from the same transactions either way.
#[test]
fn end_to_end_does_not_move_when_the_model_misnames_a_merchant() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "normalise": [{ "raw": "NETFLIX.COM", "name": "Netflix" }],
            "classify": [],
            "recurring": [{ "merchant": "Netflix", "period": "monthly" }],
            "tolerances": { "normalise": "fuzzy:0.85", "recurring": "exact" }
        }"#,
    )
    .expect("expected.json");

    let result = score_fixture(
        "statement-01.csv",
        &expected,
        &outcome(vec![finding(
            "NETFLIX.COM",
            "Netflix Inc. Streaming",
            "subscription",
            "streaming",
        )]),
        perf(),
    );

    assert_eq!(
        result.step_scores["normalise"].correct, 0,
        "the model missed"
    );
    assert_eq!(result.end_to_end, 1.0, "recurrence did not");
}

fn identity_fixture(dir: &std::path::Path, fixture: &str, fixture_id: &str, item_id: &str) {
    std::fs::write(
        dir.join(format!("{fixture}.csv")),
        "Date,Description,Debit\n2026-01-01,SYNTHETIC MERCHANT,9.99\n",
    )
    .expect("write synthetic statement");
    std::fs::write(
        dir.join(format!("{fixture}.expected.json")),
        serde_json::json!({
            "fixture_id": fixture_id,
            "normalise": [
                { "raw": "SYNTHETIC MERCHANT", "name": "Synthetic Merchant" }
            ],
            "classify": [{
                "id": item_id,
                "strata": ["clean"],
                "name": "Synthetic Merchant",
                "kind": "subscription",
                "category": "software"
            }],
            "recurring": [],
            "tolerances": {}
        })
        .to_string(),
    )
    .expect("write expectations");
}

/// A stable id identifies one item in the whole pack, not merely one
/// row in one file. Two live items wearing it would make a baseline join
/// to whichever happened to be found first.
#[test]
fn duplicate_item_ids_within_a_pack_are_refused() {
    let dir = std::env::temp_dir().join(format!(
        "kettle-eval-duplicate-item-ids-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture directory");
    identity_fixture(
        &dir,
        "statement-a",
        "clean-everyday-01",
        "monthly-software-subscription-01",
    );
    identity_fixture(
        &dir,
        "statement-b",
        "clean-everyday-02",
        "monthly-software-subscription-01",
    );

    let problem =
        fixtures_at_with_retired(&dir, &[]).expect_err("duplicate item id must be refused");

    assert!(
        problem.contains("monthly-software-subscription-01")
            && problem.contains("statement-a")
            && problem.contains("statement-b"),
        "{problem}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Retirement burns an id permanently. Reusing it would let a current
/// item silently compare with a different historical item in an old
/// baseline.
#[test]
fn retired_item_ids_cannot_be_reused() {
    let dir = std::env::temp_dir().join(format!(
        "kettle-eval-retired-item-ids-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture directory");
    identity_fixture(
        &dir,
        "statement-a",
        "clean-everyday-01",
        "annual-renewal-once-yearly-01",
    );

    let problem = fixtures_at_with_retired(&dir, &["annual-renewal-once-yearly-01".to_owned()])
        .expect_err("retired item id must stay burned");

    assert!(
        problem.contains("annual-renewal-once-yearly-01") && problem.contains("retired"),
        "{problem}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn opaque_ordinal_item_ids_are_refused() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "clean-everyday-01",
            "normalise": [{
                "raw": "SYNTHETIC MERCHANT",
                "name": "Synthetic Merchant"
            }],
            "classify": [{
                "id": "item-0042",
                "strata": ["clean"],
                "name": "Synthetic Merchant",
                "kind": "subscription",
                "category": "software"
            }],
            "recurring": [],
            "tolerances": {}
        }"#,
    )
    .expect("expectations parse");

    let problem = expected
        .validate()
        .expect_err("an opaque ordinal is not a usable diff label");
    assert!(
        problem.contains("annual-renewal-once-yearly-01") && problem.contains("item-0042"),
        "{problem}"
    );
}

#[test]
fn classification_expectation_can_name_one_raw_descriptor_when_names_merge() {
    let expected: Expected = serde_json::from_str(
        r#"{
            "fixture_id": "multi-descriptor-merchant-01",
            "normalise": [
                { "raw": "STRIPE* NORTHSTAR", "name": "Northstar Learning" },
                { "raw": "SQ *NORTHSTAR", "name": "Northstar Learning" },
                { "raw": "PAYPAL *NORTHSTAR", "name": "Northstar Learning" }
            ],
            "classify": [{
                "id": "northstar-learning-square-01",
                "strata": ["messy-merchant-strings", "multi-descriptor-merchant"],
                "raw": "SQ *NORTHSTAR",
                "name": "Northstar Learning",
                "kind": "subscription",
                "category": "software"
            }],
            "recurring": [],
            "tolerances": {}
        }"#,
    )
    .expect("expectations parse");

    assert_eq!(expected.classify[0].raw.as_deref(), Some("SQ *NORTHSTAR"));
    expected
        .validate()
        .expect("the authored raw descriptor is one of the normalise inputs");
}
