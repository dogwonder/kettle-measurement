//! #240: the obligations model role and the Extraction payload — the
//! letter typology's runner arm (part 3 of #51). A pack declaring
//! `obligations` over document segments produces `Payload::Extraction`
//! and never touches merchant cleanup or recurrence detection.

mod support;

use runner::packs::load_pack;
use runner::run::{run_pack, Answers, Payload};
use runner::run_dir::NoLog;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

/// Two paragraphs, two segments, two obligations. Wholly invented —
/// no real council, no real person (CLAUDE.md).
const LETTER: &str = "Please pay £120.00 to Harborne Parking Services \
within 14 days of the date of this letter.\n\nYou must confirm your \
payment in writing to Harborne Parking Services by 12 August 2026.";

/// A minimal letter pack in a per-test scratch directory, following
/// the `ScratchPack` idiom in `packs.rs`.
fn letter_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-letter-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create letter pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write letter pack file");
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
            { "step": "render", "template": "report.html.tera" }
          ],
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do, and by when?\n{{ batch_json }}\n",
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
    write("fixtures/letter.txt", LETTER);
    dir
}

/// The mock model's answer for the letter's one batch: both segments
/// echoed, one obligation each. Dates exactly as written — the model
/// never computes one (#240).
fn obligations_answer() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                {
                    "id": 0,
                    "segment": segments[0],
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
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "response",
                        "party": "Harborne Parking Services",
                        "ask": "Confirm your payment in writing",
                        "deadline": "by 12 August 2026",
                        "anchor": "12 August 2026"
                    }]
                }
            ]
        })
        .to_string(),
    )
}

#[test]
fn an_obligations_step_fills_the_extraction_payload() {
    let dir = letter_pack("fills-payload");
    let pack = load_pack(&dir).expect("a pack declaring obligations loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", obligations_answer())]);

    let outcome = run_pack(
        &pack,
        &[dir.join("fixtures/letter.txt")],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes");

    assert_eq!(outcome.input.rows, 2, "two segments read");
    assert!(
        outcome.needs_review.is_empty(),
        "{:?}",
        outcome.needs_review
    );

    let Payload::Extraction(extraction) = &outcome.payload else {
        panic!("a document pipeline produces the Extraction payload");
    };
    assert_eq!(extraction.obligations.len(), 2, "{extraction:?}");

    let pay = &extraction.obligations[0];
    assert_eq!(pay.kind, "payment");
    assert_eq!(pay.party, "Harborne Parking Services");
    assert_eq!(pay.ask, "Pay £120.00");
    assert_eq!(pay.deadline, "within 14 days", "the phrase, never a date");
    assert_eq!(pay.anchor, "the date of this letter");
    assert_eq!(pay.confidence, "high");
    // Evidence is the passage itself, cited as a person would find it.
    assert_eq!(pay.evidence.len(), 1);
    assert_eq!(pay.evidence[0].page, 1);
    assert_eq!(pay.evidence[0].ordinal, 0);
    assert!(
        pay.evidence[0].text.contains("within 14 days"),
        "{}",
        pay.evidence[0].text
    );

    let confirm = &extraction.obligations[1];
    assert_eq!(confirm.kind, "response");
    assert_eq!(confirm.deadline, "by 12 August 2026");
    assert_eq!(confirm.evidence[0].ordinal, 1);
    assert_eq!(
        confirm.due, None,
        "no timeline step ran, so no date was computed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_model_no_obligations_are_invented() {
    // The floor's honest arm (#240): no model means no obligations were
    // found — every segment goes to a person, never a placeholder
    // obligation nobody decided.
    let dir = letter_pack("no-model");
    let pack = load_pack(&dir).expect("pack loads");

    let outcome = run_pack(
        &pack,
        &[dir.join("fixtures/letter.txt")],
        &Answers::WithoutModel,
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the floor runs");

    let Payload::Extraction(extraction) = &outcome.payload else {
        panic!("the floor still produces the typology's payload");
    };
    assert!(
        extraction.obligations.is_empty(),
        "invented: {:?}",
        extraction.obligations
    );
    assert_eq!(
        outcome.needs_review.len(),
        2,
        "every segment reaches a person: {:?}",
        outcome.needs_review
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A dated letter whose obligations arrive out of order, one of them
/// twice. Wholly invented, as ever.
const DATED_LETTER: &str = "3 March 2026\n\nYou must confirm your \
payment in writing to Harborne Parking Services by 12 August 2026.\n\n\
Please pay £120.00 to Harborne Parking Services within 14 days of the \
date of this letter.\n\nWe remind you that payment of £120.00 is due \
within 14 days of the date of this letter.";

/// The mock's answers for the dated letter: the date line carries no
/// obligations; the reminder repeats the payment word for word, as an
/// overlapping reading would.
fn dated_letter_answer() -> String {
    let segments: Vec<&str> = DATED_LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "response",
                        "party": "Harborne Parking Services",
                        "ask": "Confirm your payment in writing",
                        "deadline": "by 12 August 2026",
                        "anchor": "12 August 2026"
                    }]
                },
                {
                    "id": 2,
                    "segment": segments[2],
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
                    "id": 3,
                    "segment": segments[3],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }]
                }
            ]
        })
        .to_string(),
    )
}

#[test]
fn the_timeline_step_dates_merges_and_orders_the_obligations() {
    // #241: the model read the phrases; the arithmetic, the merge and
    // the order are Rust's.
    let dir = letter_pack("timeline");
    let manifest_path = dir.join("pack.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let with_timeline = manifest.replace(
        r#"{ "step": "render", "template": "report.html.tera" }"#,
        r#"{ "step": "aggregate", "impl": "builtin:timeline-sort" },
            { "step": "render", "template": "report.html.tera" }"#,
    );
    assert_ne!(
        manifest, with_timeline,
        "the render step should exist to insert before"
    );
    std::fs::write(&manifest_path, with_timeline).expect("write manifest");
    std::fs::write(dir.join("fixtures/letter.txt"), DATED_LETTER).expect("write letter");

    let pack = load_pack(&dir).expect("a pack declaring timeline-sort loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", dated_letter_answer())]);

    let outcome = run_pack(
        &pack,
        &[dir.join("fixtures/letter.txt")],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes");

    let Payload::Extraction(extraction) = &outcome.payload else {
        panic!("a document pipeline produces the Extraction payload");
    };

    // Three readings, two obligations: the repeated payment merged,
    // keeping both passages as evidence.
    assert_eq!(extraction.obligations.len(), 2, "{extraction:?}");

    let pay = &extraction.obligations[0];
    assert_eq!(pay.kind, "payment");
    assert_eq!(
        pay.due.map(|d| d.date),
        Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 17).expect("a real day")),
        "within 14 days of 3 March is 17 March — Rust's arithmetic, not the model's"
    );
    assert_eq!(
        pay.deadline, "within 14 days",
        "the phrase survives beside the date"
    );
    let ordinals: Vec<usize> = pay.evidence.iter().map(|s| s.ordinal).collect();
    assert_eq!(ordinals, vec![2, 3], "both passages kept as evidence");

    let confirm = &extraction.obligations[1];
    assert_eq!(confirm.kind, "response");
    assert_eq!(
        confirm.due.map(|d| d.date),
        Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 12).expect("a real day")),
        "an absolute date is read, and sorts after the sooner deadline"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
