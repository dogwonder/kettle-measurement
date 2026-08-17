//! #429: confidence must survive scoring before it can be calibrated.
//!
//! The issue names
//! `empty_extraction_answers_keep_their_confidence_and_enter_the_risk_table`;
//! the risk table itself is the calibration report's plumbing and
//! trails this slice, so the record-shape essence is what goes red
//! first: a correct nothing-found passage and a missed obligation at
//! the same declared confidence both land as scored records carrying
//! that confidence and the trace id of the exchange whose answer
//! carried it. Until now the "nothing here" answers existed only in
//! raw exchange text — 83% of the letter run's answered passages are
//! in that shape — and confidence appeared nowhere in any scored
//! field.

mod support;

use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::{
    DeclaredConfidence, EvalMetric, ExtractionOutcome, HarmClass, MachineInfo, MetricReport,
};
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

/// A letter with one real obligation, one passage that asks for
/// nothing, and a date line nobody authored an expectation for.
/// Wholly invented.
const LETTER: &str = "3 March 2026\n\nPlease pay £120.00 to Harborne \
Parking Services within 14 days of the date of this letter.\n\nWe are \
sorry for any inconvenience and thank you for your co-operation.";

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// A letter pack whose fixture expects one obligation and one nothing,
/// and says nothing at all about the letter's date line.
fn letter_pack(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kettle-calibration-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write pack file");
    };
    write(
        "pack.json",
        r#"{
          "id": "app.kttl.test-letter",
          "name": "Letter test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "multiple": false }],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:timeline-sort" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Tracks how many passages a person reads.", "date": "2026-07-31" } },
          "eval_strata": {
            "dated-letter": {
              "description": "Letters that carry their own date, whether or not the passage asks anything.",
              "classes": {
                "obligation": { "max_wilson_95": 0.01, "reason": "A missed deadline is often unrecoverable.", "date": "2026-07-31" },
                "no_obligation": { "max_wilson_95": 0.05, "reason": "An invented obligation costs a phone call.", "date": "2026-07-31" }
              }
            },
            "relative-deadline": { "description": "Diagnostic slice.", "classes": {} },
            "no-obligation": { "description": "Diagnostic slice.", "classes": {} }
          },
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "enum": ["payment", "response", "attendance", "other"] },
                    "party": { "type": "string" },
                    "ask": { "type": "string" },
                    "deadline": { "type": "string" },
                    "anchor": { "type": "string" }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/letter-01.txt", LETTER);
    write(
        "fixtures/letter-01.expected.json",
        r#"{
          "fixture_id": "parking-reminder-01",
          "eval_set": "development",
          "obligations": [
            {
              "id": "parking-fine-payment-01",
              "strata": ["dated-letter", "relative-deadline"],
              "segment": "Please pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.",
              "expect": {
                "kind": "payment",
                "party": "Harborne Parking Services",
                "deadline": "within 14 days",
                "anchor": "the date of this letter",
                "due": "2026-03-17"
              }
            },
            {
              "id": "courtesy-closing-no-ask-01",
              "strata": ["dated-letter", "no-obligation"],
              "segment": "We are sorry for any inconvenience and thank you for your co-operation.",
              "expect": null
            }
          ]
        }"#,
    );
    dir
}

/// The model answers every passage with nothing, all at high
/// confidence: a missed obligation, a correct authored negative, and a
/// correct negative on the unauthored date line — three decisions at
/// the same declared confidence, one of them an error.
fn answer_nothing_everywhere_at_high() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                { "id": 1, "segment": segments[1], "confidence": "high", "obligations": [] },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

/// The model finds the payment, and says so at low confidence.
fn answer_finding_the_payment_at_low() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "low",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }]
                },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

fn evaluate(dir: &Path, answer: String) -> runner::eval::EvalReport {
    let pack = load_pack(dir).expect("the letter pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs")
}

/// The declared level and trace id, or a panic naming what the record
/// carried instead — an inherited/untraceable confidence is a
/// different claim and must never satisfy these assertions.
fn declared(item: &runner::eval::ScoredItem) -> (&str, &str) {
    match item
        .confidence
        .as_ref()
        .unwrap_or_else(|| panic!("{} records no confidence", item.id))
    {
        DeclaredConfidence::Declared { level, trace_id } => (level.as_str(), trace_id.as_str()),
        other => panic!("{} carries {other:?}, not a declared confidence", item.id),
    }
}

#[test]
fn empty_extraction_answers_keep_their_confidence_and_land_as_scored_records() {
    let dir = letter_pack("nothing-everywhere");
    let report = evaluate(&dir, answer_nothing_everywhere_at_high());
    let items = &report.fixtures[0].items;

    // The missed obligation: the model said "nothing here" about a
    // passage that asks for a payment, and said it at high confidence.
    // The error and the confidence must land on the same record.
    let missed = items
        .iter()
        .find(|item| item.item_id == "parking-fine-payment-01")
        .expect("the missed payment is a scored item");
    let (expected, actual) = missed.decision.as_extraction().expect("an extraction");
    assert!(expected.is_some(), "an obligation was expected here");
    assert!(
        matches!(actual, ExtractionOutcome::Absent),
        "the model asserted nothing: {actual:?}"
    );
    let (missed_level, missed_trace) = declared(missed);
    assert_eq!(missed_level, "high");
    assert!(
        missed.trace_ids.contains(&missed_trace.to_owned()),
        "the confidence names an exchange this record already traces to: \
         {missed_trace} not in {:?}",
        missed.trace_ids
    );

    // The authored correct negative keeps its confidence too: absent
    // is a decision, not a missing record.
    let closing = items
        .iter()
        .find(|item| item.item_id == "courtesy-closing-no-ask-01")
        .expect("the authored nothing is a scored item");
    let (closing_level, _) = declared(closing);
    assert_eq!(closing_level, "high");

    // The unauthored date line: the model was asked about it and
    // answered "nothing here". Until #429 that decision existed only in
    // raw exchange text; it is now a first-class scored record — a
    // correct negative, at the same declared confidence as the miss.
    let date_line = items
        .iter()
        .find(|item| item.raw_input.contains("3 March 2026"))
        .expect("the answered-nothing passage is a scored record");
    let (expected, actual) = date_line.decision.as_extraction().expect("an extraction");
    assert!(
        expected.is_none(),
        "nothing authored expects this passage: {date_line:?}"
    );
    assert!(
        matches!(actual, ExtractionOutcome::Absent),
        "the model asserted nothing: {actual:?}"
    );
    assert!(
        date_line.decision.is_unauthored_negative(),
        "the record says why it exists: {date_line:?}"
    );
    let (date_level, date_trace) = declared(date_line);
    assert_eq!(date_level, missed_level, "the same declared confidence");
    assert!(!date_trace.is_empty(), "the negative names its exchange");
    assert_ne!(
        date_trace, missed_trace,
        "two decisions, two answers, two exchanges"
    );

    // The record must not move what the gates were sized on: the
    // no-obligation denominators count authored decisions only, so the
    // unauthored correct negative adds a record and no evidence.
    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    assert_eq!(
        metrics.overall.harm_classes[&HarmClass::NoObligation]
            .recall
            .n,
        1,
        "only the authored nothing counts towards the invention class"
    );
    assert_eq!(
        metrics.overall.harm_classes[&HarmClass::Obligation]
            .confident_wrong_rate
            .successes,
        1,
        "the miss still counts exactly as it did"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_found_assertion_keeps_the_confidence_its_answer_declared() {
    let dir = letter_pack("found-at-low");
    let report = evaluate(&dir, answer_finding_the_payment_at_low());
    let items = &report.fixtures[0].items;

    let payment = items
        .iter()
        .find(|item| item.item_id == "parking-fine-payment-01")
        .expect("the payment is a scored item");
    let (_, actual) = payment.decision.as_extraction().expect("an extraction");
    assert!(
        matches!(actual, ExtractionOutcome::Found { .. }),
        "the model found it: {actual:?}"
    );
    let (level, trace_id) = declared(payment);
    assert_eq!(level, "low", "found keeps the answer's own confidence");
    assert!(
        payment.trace_ids.contains(&trace_id.to_owned()),
        "the confidence names an exchange this record already traces to"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #429: a confidence nothing aggregates is decorative.
///
/// The plumbing above proved the level survives on positive, negative
/// and unauthored-negative records alike. This is the half that makes
/// it worth carrying: every one of those decisions enters a risk table
/// keyed by the level the model declared, and the errors land in the
/// bucket that claimed them.
///
/// The unauthored correct negative counts **here** and not towards a
/// gate, which is the distinction `extraction_metrics` already draws:
/// no ceiling was sized on passages nobody authored, but a model's
/// confidence in answering "nothing here" is exactly what a risk table
/// exists to price.
#[test]
fn empty_extraction_answers_enter_the_risk_table() {
    let dir = letter_pack("risk-table");
    let report = evaluate(&dir, answer_nothing_everywhere_at_high());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let high = metrics
        .calibration
        .buckets
        .get("high")
        .expect("the level every answer declared has a bucket");

    // Three decisions, one wrong: the missed payment, the authored
    // correct negative, and the unauthored one — all answered "nothing
    // here", all at high.
    assert_eq!(high.decisions, 3, "{:?}", metrics.calibration);
    assert_eq!(high.errors, 1, "{:?}", metrics.calibration);
    assert_eq!(high.routed_to_review, 0, "{:?}", metrics.calibration);
    assert_eq!(high.error_rate.successes, 1);
    assert_eq!(high.error_rate.n, 3);

    let _ = std::fs::remove_dir_all(&dir);
}

/// #429's fifth acceptance criterion: a model that declares one level
/// for everything is saying nothing, and the report must say so rather
/// than print a table that looks like evidence.
///
/// This is not a hypothetical shape. On the v14 letter run 3,025 of
/// 3,028 decisions were declared `high`; routing by that confidence
/// could not separate a right answer from a wrong one because there was
/// nothing to separate them by.
#[test]
fn a_model_that_declares_one_level_for_everything_carries_no_ranking_signal() {
    let dir = letter_pack("one-level");
    let report = evaluate(&dir, answer_nothing_everywhere_at_high());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    assert!(
        matches!(
            &metrics.calibration.signal,
            runner::eval::RankingSignal::NoVariation { level } if level == "high"
        ),
        "one level for every decision ranks nothing: {:?}",
        metrics.calibration
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Levels that vary are still not levels that rank. Two buckets whose
/// intervals overlap have not shown that one is safer than the other,
/// and #429 is explicit: where the evidence is too thin, say unproven
/// rather than quoting a difference the bed cannot support.
#[test]
fn levels_that_vary_without_separating_are_unproven() {
    let dir = letter_pack("unproven");
    let report = evaluate(&dir, answer_finding_the_payment_at_low());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    assert!(
        metrics.calibration.buckets.len() > 1,
        "this answer declares two levels: {:?}",
        metrics.calibration
    );
    assert!(
        matches!(
            metrics.calibration.signal,
            runner::eval::RankingSignal::Unproven
        ),
        "four decisions cannot establish a ranking: {:?}",
        metrics.calibration
    );

    let _ = std::fs::remove_dir_all(&dir);
}
