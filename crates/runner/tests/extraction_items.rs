//! #279: obligations get per-item records with their own harm lens, so
//! a letter pack can declare a confident-wrong ceiling.
//!
//! The asymmetry is the point. Missing an obligation is often
//! unrecoverable — the fine escalates, the hearing passes — while
//! inventing one costs a person a phone call. Those two errors cannot
//! share a ceiling, so they cannot share a class.

mod support;

use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::{EvalMetric, ExtractionOutcome, HarmClass, MachineInfo, MetricReport};
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

/// A letter with one real obligation and one passage that asks for
/// nothing — the case a keen extractor gets wrong. Wholly invented.
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

/// A letter pack whose fixture expects one obligation and one nothing.
fn letter_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-ext-items-{}-{name}", std::process::id()));
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

/// The model finds the payment and, wrongly, reads an obligation into
/// the courtesy closing — one hit and one invention, which is exactly
/// the pair the harm lens has to tell apart.
fn answer_inventing_from_the_closing() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }]
                },
                {
                    "id": 2,
                    "segment": segments[2],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "response",
                        "party": "Harborne Parking Services",
                        "ask": "Co-operate",
                        "deadline": "as soon as possible",
                        "anchor": "the date of this letter"
                    }]
                }
            ]
        })
        .to_string(),
    )
}

/// The model misses the payment entirely — the expensive error.
fn answer_missing_the_payment() -> String {
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
        pdfium_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs")
}

#[test]
fn an_obligations_run_emits_per_item_records_with_their_own_harm_lens() {
    let dir = letter_pack("harm-lens");
    let report = evaluate(&dir, answer_inventing_from_the_closing());

    let items = &report.fixtures[0].items;
    // One record per authored item, plus the answered-nothing record
    // for the unauthored date line (#429) — kept for calibration and
    // excluded from every metric a gate reads.
    assert_eq!(
        items.len(),
        3,
        "two authored, one answered-nothing: {items:?}"
    );
    assert_eq!(
        items
            .iter()
            .filter(|item| !item.decision.is_unauthored_negative())
            .count(),
        2,
        "one gate-visible record per authored item: {items:?}"
    );

    // Identity and provenance are the shared machinery from #237, not
    // classification concepts — an extraction item carries them too.
    let payment = items
        .iter()
        .find(|item| item.item_id == "parking-fine-payment-01")
        .expect("the payment is a scored item");
    assert_eq!(
        payment.id,
        "app.kttl.test-letter/parking-reminder-01/parking-fine-payment-01"
    );
    assert_eq!(payment.strata, ["dated-letter", "relative-deadline"]);
    assert_eq!(payment.pack_version, "0.0.1");
    assert!(payment.prompt_version.starts_with("blake3:"));
    assert_eq!(payment.decision.metric(), EvalMetric::Extraction);
    let (expected, actual) = payment
        .decision
        .as_extraction()
        .expect("an extraction decision");
    let Some(runner::eval::Extracted::Obligation(want)) = expected.as_ref() else {
        panic!("a letter pack's expectation is an obligation: {expected:?}");
    };
    assert_eq!(want.kind, "payment");
    assert!(
        matches!(actual, ExtractionOutcome::Found { .. }),
        "the model found it: {actual:?}"
    );
    assert!(
        payment
            .exchanges
            .iter()
            .any(|exchange| exchange.response.contains("within 14 days")),
        "the item keeps the exchange that produced it"
    );

    // The invention: nothing was asked of anybody in the closing, and
    // the model asserted something was.
    let closing = items
        .iter()
        .find(|item| item.item_id == "courtesy-closing-no-ask-01")
        .expect("the closing is a scored item");
    let (expected, actual) = closing
        .decision
        .as_extraction()
        .expect("an extraction decision");
    assert!(expected.is_none(), "nothing is expected here");
    assert!(
        matches!(actual, ExtractionOutcome::Found { .. }),
        "the model invented one: {actual:?}"
    );

    // The lens: an invention is confidently wrong about `no_obligation`,
    // and must not be counted against the miss class.
    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let invented = &metrics.overall.harm_classes[&HarmClass::NoObligation];
    assert_eq!(invented.confident_wrong_rate.successes, 1);
    let missed = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        missed.confident_wrong_rate.successes, 0,
        "inventing is not missing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missed_obligation_is_counted_in_the_miss_class_not_the_invent_class() {
    let dir = letter_pack("miss");
    let report = evaluate(&dir, answer_missing_the_payment());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let missed = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        missed.confident_wrong_rate.successes, 1,
        "the deadline nobody will now meet"
    );
    let invented = &metrics.overall.harm_classes[&HarmClass::NoObligation];
    assert_eq!(invented.confident_wrong_rate.successes, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_letter_packs_asymmetric_ceilings_are_both_enforced() {
    // One miss out of one expected obligation is a 100% miss rate,
    // which no ceiling tolerates — the gate must fail, and it must
    // name the miss class rather than the invent class.
    let dir = letter_pack("gate");
    let report = evaluate(&dir, answer_missing_the_payment());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let gates = metrics
        .gates
        .get("dated-letter")
        .expect("the declared stratum is gated");
    assert!(
        !gates[&HarmClass::Obligation].outcome.clears(),
        "the miss ceiling must fail: {:?}",
        gates[&HarmClass::Obligation]
    );
    assert_eq!(gates[&HarmClass::Obligation].max_wilson_95, 0.01);
    assert_eq!(
        gates[&HarmClass::NoObligation].max_wilson_95,
        0.05,
        "the two directions carry different ceilings"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_segment_routed_to_a_person_is_neither_a_hit_nor_a_miss() {
    // The honest floor: no model means every segment reaches a person.
    // Scored as misses, that would read as catastrophic — the exact
    // opposite of what it does.
    let dir = letter_pack("floor");
    let pack = load_pack(&dir).expect("pack loads");
    let evaluator = FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };
    let report = evaluator.evaluate(&pack).expect("the floor runs");

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    for class in [HarmClass::Obligation, HarmClass::NoObligation] {
        assert_eq!(
            metrics.overall.harm_classes[&class]
                .confident_wrong_rate
                .successes,
            0,
            "{class:?}: review is not an assertion"
        );
    }
    assert_eq!(
        metrics.overall.harm_classes[&HarmClass::Obligation]
            .recall
            .estimate,
        Some(1.0),
        "surfaced recall: the obligation reached a person"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #443, found by the mutation harness: a found-but-wrong assertion —
/// the model asserts an obligation on the right passage with the wrong
/// deadline — must count confident-wrong in the class that expected
/// it. A person told the wrong date confidently has been harmed in
/// exactly the way the ceiling exists to bound; on the real bed all
/// 340 one-digit deadline mutants cleared every gate because this
/// cell read as a correct assertion.
#[test]
fn a_found_but_wrong_assertion_counts_confident_wrong_in_the_obligation_class() {
    let dir = letter_pack("wrong-assertion");
    let report = evaluate(&dir, answer_with_the_wrong_deadline());

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let wrongly_asserted = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        wrongly_asserted.confident_wrong_rate.successes, 1,
        "the wrong deadline is a confident wrong answer: {wrongly_asserted:?}"
    );
    let invented = &metrics.overall.harm_classes[&HarmClass::NoObligation];
    assert_eq!(
        invented.confident_wrong_rate.successes, 0,
        "asserting wrongly is not inventing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The model finds the payment where it is, but reads its deadline one
/// digit off — the mutation the ceilings never saw.
fn answer_with_the_wrong_deadline() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 15 days",
                        "anchor": "the date of this letter"
                    }]
                },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

/// #442, found by the mutation harness: an obligation asserted about a
/// passage the bed never authored must become a scored invention, not
/// vanish between the raw answer and the metrics. On the real bed
/// 1,750 such inventions were invisible to every scored item while a
/// real run would have carried them into the report.
#[test]
fn an_assertion_on_an_unauthored_passage_becomes_a_scored_invention() {
    let dir = letter_pack("unauthored-passage");
    // The expectations keep only the payment item: the closing
    // passage is now unauthored, and the model asserts about it.
    let expected = std::fs::read_to_string(dir.join("fixtures/letter-01.expected.json"))
        .expect("expectations read");
    let mut parsed: serde_json::Value = serde_json::from_str(&expected).expect("valid json");
    parsed["obligations"]
        .as_array_mut()
        .expect("items")
        .retain(|item| item["id"] == "parking-fine-payment-01");
    std::fs::write(
        dir.join("fixtures/letter-01.expected.json"),
        serde_json::to_string_pretty(&parsed).expect("serialises"),
    )
    .expect("write");

    let report = evaluate(&dir, answer_inventing_from_the_closing());

    let items = &report.fixtures[0].items;
    let invention = items
        .iter()
        .find(|item| item.raw_input.contains("sorry for any inconvenience"))
        .expect("the unauthored passage's assertion is a scored item");
    let (expected, actual) = invention
        .decision
        .as_extraction()
        .expect("an extraction decision");
    assert!(
        expected.is_none(),
        "nothing authored expects this passage: {invention:?}"
    );
    assert!(
        matches!(actual, ExtractionOutcome::Found { .. }),
        "the assertion is recorded as made: {actual:?}"
    );

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let invented = &metrics.overall.harm_classes[&HarmClass::NoObligation];
    assert_eq!(
        invented.confident_wrong_rate.successes, 1,
        "the invention counts against the invention class: {invented:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
