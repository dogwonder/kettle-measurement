//! #330: a pack declaring `"multiple": true` is run against more than
//! one document, and the runner has to keep them apart.
//!
//! Both shipped packs advertise multi-document runs and nothing tested
//! one. `run_pack` pools every input into a single `segments`
//! collection, so two documents become one — with the consequences
//! pinned below.

mod support;

use runner::document::Segment;
use runner::packs::load_pack;
use runner::run::{run_pack, Answers, Obligation, Payload};
use runner::run_dir::NoLog;
use runner::timeline::sort_timeline;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

/// Two wholly invented letters (CLAUDE.md), each dated and each asking
/// for something "within 14 days of the date of this letter" — the
/// phrase whose answer depends entirely on *which* letter it is in.
///
/// The parties differ so `same_obligation` cannot merge the two into
/// one: this is a test about resolving dates, not about merging.
const MARCH_LETTER: &str = "12 March 2026\n\nPlease pay £120.00 to \
Harborne Parking Services within 14 days of the date of this letter.";

const JUNE_LETTER: &str = "20 June 2026\n\nPlease pay £80.00 to \
Selly Oak Water within 14 days of the date of this letter.";

/// A letter pack that accepts several documents and sorts a timeline —
/// the real `app.kttl.letter-to-actions` shape, minimised.
fn letter_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-multi-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-multi-letter",
          "name": "Multi-letter test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "multiple": true }],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:timeline-sort" },
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
    write("fixtures/march.txt", MARCH_LETTER);
    write("fixtures/june.txt", JUNE_LETTER);
    dir
}

/// The mock's answer for the one batch covering both letters: four
/// segments, a date line and a body from each. Only the bodies carry an
/// obligation, and each cites its own letter's phrasing verbatim — the
/// model never computes a date (#240).
fn obligations_answer() -> String {
    let march: Vec<&str> = MARCH_LETTER.split("\n\n").collect();
    let june: Vec<&str> = JUNE_LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": march[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": march[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }]
                },
                { "id": 2, "segment": june[0], "confidence": "high", "obligations": [] },
                {
                    "id": 3,
                    "segment": june[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Selly Oak Water",
                        "ask": "Pay £80.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }]
                }
            ]
        })
        .to_string(),
    )
}

/// "The date of this letter" means the date of *the letter it is in*.
///
/// `letter_date` reads the first three segments of what it is given,
/// and `run_pack` gives it every document's segments concatenated. So
/// on a two-letter run the second letter's relative deadlines resolve
/// against the *first* letter's date, and Kettle shows a person a due
/// date that is months wrong while presenting it as a resolved fact.
///
/// This is the harm that makes #330 more than tidying: a wrong date is
/// worse than no date, because `None` is displayed as "undated" and a
/// resolved date is displayed as certain.
#[test]
fn a_relative_deadline_resolves_against_its_own_document_date() {
    let dir = letter_pack("own-date");
    let pack = load_pack(&dir).expect("a multi-input letter pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", obligations_answer())]);

    let outcome = run_pack(
        &pack,
        &[
            dir.join("fixtures/march.txt"),
            dir.join("fixtures/june.txt"),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes");

    let Payload::Extraction(extraction) = &outcome.payload else {
        panic!("an obligations pack produces the Extraction payload");
    };

    let due_for = |party: &str| {
        extraction
            .obligations
            .iter()
            .find(|obligation| obligation.party == party)
            .unwrap_or_else(|| panic!("no obligation for {party}: {:#?}", extraction.obligations))
            .due
    };

    // 12 March + 14 days.
    assert_eq!(
        due_for("Harborne Parking Services").map(|d| d.date.to_string()),
        Some("2026-03-26".to_owned()),
        "the first letter's deadline counts from its own date"
    );
    // 20 June + 14 days — not 26 March, which is what pooling gives.
    assert_eq!(
        due_for("Selly Oak Water").map(|d| d.date.to_string()),
        Some("2026-07-04".to_owned()),
        "the second letter's deadline must count from the second letter's \
         date, not from whichever document happened to be read first"
    );
}

/// Two photographs can be pages of one letter rather than two letters.
/// The date is only on page one and the ask only on page two: treating
/// them as separate documents leaves the deadline unresolved.
#[test]
fn ordered_files_are_one_letter_and_keep_their_page_numbers() {
    let dir = letter_pack("ordered-pages");
    let manifest = std::fs::read_to_string(dir.join("pack.json")).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace(
            r#""multiple": true"#,
            r#""count": { "min": 1 }, "file_semantics": "pages""#,
        ),
    )
    .unwrap();
    std::fs::write(dir.join("fixtures/march.txt"), "12 March 2026").unwrap();
    let ask = "Please pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.";
    std::fs::write(dir.join("fixtures/june.txt"), ask).unwrap();

    let pack = load_pack(&dir).expect("an ordered-page letter pack loads");
    let mock = MockModel::respond_sequence(vec![(
        "200 OK",
        completion_envelope(
            &serde_json::json!({
                "results": [
                    { "id": 0, "segment": "12 March 2026", "confidence": "high", "obligations": [] },
                    { "id": 1, "segment": ask, "confidence": "high", "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay £120.00",
                        "deadline": "within 14 days",
                        "anchor": "the date of this letter"
                    }] }
                ]
            })
            .to_string(),
        ),
    )]);

    let outcome = run_pack(
        &pack,
        &[
            dir.join("fixtures/march.txt"),
            dir.join("fixtures/june.txt"),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the page group runs");

    let Payload::Extraction(extraction) = outcome.payload else {
        panic!("expected extraction");
    };
    let obligation = extraction.obligations.first().expect("one obligation");
    assert_eq!(
        obligation.due.map(|due| due.date.to_string()),
        Some("2026-03-26".to_owned())
    );
    assert_eq!(obligation.evidence[0].document, 0);
    assert_eq!(obligation.evidence[0].page, 2);
}

/// The same ask, sent twice: evidence stays in document order.
///
/// An ordinal counts within its own document, so two documents both
/// have an ordinal 0. Sorting merged evidence on the ordinal alone
/// interleaves them — the second letter's opening line sorts above the
/// first letter's closing one, as though one document continued the
/// other. A person following the citations then reads the chase before
/// the letter it chases.
#[test]
fn merged_evidence_stays_in_document_order() {
    let segment = |document: usize, ordinal: usize, text: &str| Segment {
        document,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    };
    let ask = |evidence: Vec<Segment>| Obligation {
        kind: "payment".to_owned(),
        party: "Harborne Parking Services".to_owned(),
        ask: "Pay £120.00".to_owned(),
        deadline: "by 12 August 2026".to_owned(),
        anchor: "12 August 2026".to_owned(),
        amount: "no amount".to_owned(),
        confidence: "high".to_owned(),
        due: None,
        evidence,
        dated_by: None,
        priced_by: None,
        disputed: vec![],
    };

    // The first letter says it late in the page; the chaser says it
    // first thing. `same_obligation` merges them — same ask, same
    // party, same deadline — so the two documents' evidence lands in
    // one obligation and has to be ordered.
    let sorted = sort_timeline(
        vec![
            ask(vec![segment(
                1,
                0,
                "As set out in our letter of 12 March, pay £120.00.",
            )]),
            ask(vec![segment(0, 4, "Please pay £120.00 by 12 August 2026.")]),
        ],
        &[],
    );

    assert_eq!(sorted.len(), 1, "one ask, said twice: {sorted:#?}");
    let cited: Vec<(usize, usize)> = sorted[0]
        .evidence
        .iter()
        .map(|segment| (segment.document, segment.ordinal))
        .collect();
    assert_eq!(
        cited,
        vec![(0, 4), (1, 0)],
        "the original letter is cited before the chaser that refers back \
         to it; ordinal 0 of the second document is not the start of the \
         first"
    );
}
