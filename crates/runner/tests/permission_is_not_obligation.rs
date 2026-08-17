//! #406: *may* is not *must*, and Rust is what says so.
//!
//! On the v14 letter run the model read *"You may also confirm in
//! writing that you have made this payment, within 28 days of the date
//! of this letter"* as a `response` obligation, at high confidence. It
//! is the `controlled-must-to-may` twin: the authored edit turns the
//! requirement into a permission, so the obligation should disappear
//! and nothing else move. Two instruments reported the one defect — the
//! declared relation, and the item-level `no_obligation` class, where
//! it was the single confident-wrong left in `any-letter` once #406's
//! invoice shape stopped gating.
//!
//! A permission read as a requirement is not a contested reading. So
//! the fix is a deterministic one, in the shape #258 settled: the model
//! labels, Rust decides. A passage that grants and does not require is
//! routed to a person rather than asserted at — which costs review
//! rate, the declared cost, and never harm.
//!
//! What the bed can and cannot say about it: three passages in 826
//! fixtures carry a permission modal with nothing requiring beside it,
//! and two of them are the must-to-may twins themselves. This rule is
//! therefore **measured on the defect and barely exposed anywhere
//! else** — which is the argument for it routing to review rather than
//! dropping, not an argument that it has been proven safe at scale.

mod support;

use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::{ExtractionOutcome, MachineInfo};
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

/// The must-to-may pair in one letter: a payment that is required, and
/// a confirmation that is offered. Wholly invented, as every fixture in
/// this repository is.
const LETTER: &str = "3 March 2026\n\nYou must pay £120.00 to Harborne \
Parking Services within 14 days of the date of this letter.\n\nYou may \
also confirm in writing that you have made this payment, within 28 days \
of the date of this letter.";

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// A letter pack whose fixture expects the required payment and nothing
/// at all from the offered confirmation.
fn letter_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-permission-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-permission",
          "name": "Permission test",
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
                "obligation": { "max_wilson_95": 0.01, "reason": "A missed deadline is often unrecoverable.", "date": "2026-08-14" },
                "no_obligation": { "max_wilson_95": 0.05, "reason": "An invented obligation costs a phone call.", "date": "2026-08-14" }
              }
            },
            "permission": { "description": "Diagnostic slice.", "classes": {} }
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
          "fixture_id": "must-to-may-01",
          "eval_set": "development",
          "obligations": [
            {
              "id": "must-to-may-payment-01",
              "strata": ["dated-letter"],
              "segment": "You must pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.",
              "expect": {
                "kind": "payment",
                "party": "Harborne Parking Services",
                "deadline": "within 14 days",
                "anchor": "the date of this letter",
                "due": "2026-03-17"
              }
            },
            {
              "id": "must-to-may-permission-02",
              "strata": ["dated-letter", "permission"],
              "segment": "You may also confirm in writing that you have made this payment, within 28 days of the date of this letter.",
              "expect": null
            }
          ]
        }"#,
    );
    dir
}

/// The run of 14 August, reproduced: the required payment read
/// correctly, and the offered confirmation read as a `response`
/// obligation at high confidence.
fn answer_reading_the_permission_as_an_obligation() -> String {
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
                        "ask": "Confirm in writing",
                        "deadline": "within 28 days",
                        "anchor": "the date of this letter"
                    }]
                }
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
        model: Some(model_info("qwen3.5-4b-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs")
}

#[test]
fn an_obligation_read_out_of_a_permission_is_routed_to_a_person() {
    let dir = letter_pack("routed");
    let report = evaluate(&dir, answer_reading_the_permission_as_an_obligation());

    let items = &report.fixtures[0].items;
    let permission = items
        .iter()
        .find(|item| item.item_id == "must-to-may-permission-02")
        .expect("the offered confirmation is a scored item");
    let (expected, actual) = permission
        .decision
        .as_extraction()
        .expect("an extraction decision");
    assert!(
        expected.is_none(),
        "a permission obliges nobody: {permission:?}"
    );
    assert!(
        matches!(actual, ExtractionOutcome::NeedsReview { .. }),
        "the model's assertion must be contained rather than asserted, got: {actual:?}"
    );
}

/// The other half, and the one that makes the rule worth having rather
/// than merely quiet: a requirement in the same letter, in the same
/// words bar the modal, is still asserted. A rule that bought its
/// invention ceiling by routing real obligations to review would have
/// traded the recoverable harm for the unrecoverable one.
#[test]
fn a_requirement_beside_it_is_still_asserted() {
    let dir = letter_pack("requirement");
    let report = evaluate(&dir, answer_reading_the_permission_as_an_obligation());

    let items = &report.fixtures[0].items;
    let payment = items
        .iter()
        .find(|item| item.item_id == "must-to-may-payment-01")
        .expect("the required payment is a scored item");
    let (_, actual) = payment
        .decision
        .as_extraction()
        .expect("an extraction decision");
    assert!(
        matches!(actual, ExtractionOutcome::Found { .. }),
        "the requirement is still read: {actual:?}"
    );
}
