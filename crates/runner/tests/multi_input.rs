//! #330: a pack declaring `"multiple": true` is run against more than
//! one document, and the runner has to keep them apart.
//!
//! Both shipped packs advertise multi-document runs and nothing tested
//! one. `run_pack` pools every input into a single `segments`
//! collection, so two documents become one — with the consequences
//! pinned below.

mod support;

use runner::claim_trace::{CheckOutcome, ClaimKind, Guardrail, TerminalDisposition};
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
                    "party": { "type": "object", "properties": { "at": { "type": "integer" }, "value": { "type": "string" } }, "required": ["at", "value"] },
                    "ask": { "type": "string" },
                    "deadline": { "type": "object", "properties": { "at": { "type": "integer" }, "value": { "type": "string" } }, "required": ["at", "value"] },
                    "anchor": { "type": "string" },
                    "amount": { "type": "object", "properties": { "at": { "type": "integer" }, "value": { "type": "string" } }, "required": ["at", "value"] }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/march.txt", MARCH_LETTER);
    write("fixtures/june.txt", JUNE_LETTER);
    dir
}

/// The mock's answers, one batch per letter (#624): a date line and a
/// body from each. Only the bodies carry an obligation, and each cites
/// its own letter's phrasing verbatim — the model never computes a date
/// (#240). Ids number across the step, so the second letter's date line
/// is id 2 even though it opens its own batch.
fn obligations_answers() -> Vec<(&'static str, String)> {
    let march: Vec<&str> = MARCH_LETTER.split("\n\n").collect();
    let june: Vec<&str> = JUNE_LETTER.split("\n\n").collect();
    let letter = |first: usize, lines: &[&str], party: &str, ask: &str| {
        completion_envelope(
            &serde_json::json!({
                "results": [
                    { "id": first, "segment": lines[0], "confidence": "high", "obligations": [] },
                    {
                        "id": first + 1,
                        "segment": lines[1],
                        "confidence": "high",
                        "obligations": [{
                            "kind": "payment",
                            "party": { "at": first + 1, "value": party },
                            "ask": ask,
                            "deadline": { "at": first + 1, "value": "within 14 days" },
                            "anchor": "the date of this letter"
                        }]
                    }
                ]
            })
            .to_string(),
        )
    };
    vec![
        (
            "200 OK",
            letter(0, &march, "Harborne Parking Services", "Pay £120.00"),
        ),
        ("200 OK", letter(2, &june, "Selly Oak Water", "Pay £80.00")),
    ]
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
    let mock = MockModel::respond_sequence(obligations_answers());

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
            .find(|obligation| obligation.party.value == party)
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
                        "party": { "at": 1, "value": "Harborne Parking Services" },
                        "ask": "Pay £120.00",
                        "deadline": { "at": 1, "value": "within 14 days" },
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

/// Identical wording in two documents is two readings (review of #626,
/// Task 2). A chaser that repeats the original letter's ask is a second
/// letter making it, and a person is shown both, each citing its own
/// document. The order is the documents' own: the original before the
/// chaser that refers back to it, never the chaser's ordinal 0 sorting
/// above the original's closing line as though one document continued
/// the other.
#[test]
fn identical_asks_in_two_documents_are_both_shown_in_document_order() {
    let segment = |document: usize, ordinal: usize, text: &str| Segment {
        document,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    };
    let ask = |evidence: Vec<Segment>| Obligation {
        kind: "payment".to_owned(),
        party: runner::reading::Reading::new(0, "Harborne Parking Services".to_owned()),
        ask: "Pay £120.00".to_owned(),
        deadline: runner::reading::Reading::new(0, "by 12 August 2026".to_owned()),
        anchor: "12 August 2026".to_owned(),
        amount: runner::reading::Reading::absent(0),
        refused: Vec::new(),
        confidence: "high".to_owned(),
        due: None,
        evidence,
        dated_by: None,
        priced_by: None,
        shown: Default::default(),
        disputed: vec![],
    };

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

    assert_eq!(
        sorted.len(),
        2,
        "one ask in two letters is shown twice: {sorted:#?}"
    );
    let cited: Vec<(usize, usize)> = sorted
        .iter()
        .map(|o| (o.evidence[0].document, o.evidence[0].ordinal))
        .collect();
    assert_eq!(
        cited,
        vec![(0, 4), (1, 0)],
        "the original letter before the chaser"
    );
}

/// The batch ids a request put in front of the model, read back out of
/// the rendered prompt the mock received.
fn ids_shown(request_body: &str) -> Vec<usize> {
    let body: serde_json::Value = serde_json::from_str(request_body).expect("request is JSON");
    let content: String = body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter_map(|message| message["content"].as_str())
        .collect();
    let from = content
        .find('[')
        .expect("the prompt carries the batch JSON");
    let to = content.rfind(']').expect("the batch JSON closes");
    let batch: Vec<serde_json::Value> =
        serde_json::from_str(&content[from..=to]).expect("batch JSON parses");
    batch
        .iter()
        .map(|item| item["id"].as_u64().unwrap() as usize)
        .collect()
}

/// A mock answer that echoes every passage of one batch and finds no
/// obligation in any of them.
fn empty_answer(ids: std::ops::Range<usize>, passages: &[&str]) -> String {
    completion_envelope(
        &serde_json::json!({
            "results": ids
                .map(|id| serde_json::json!({
                    "id": id, "segment": passages[id], "confidence": "high", "obligations": []
                }))
                .collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// The batch is the window the model sees, and the window should be
/// the letter (#624; `app/METHOD.md` §1.4 item 4). Two letters of eight
/// passages each, batched in twenties, were one batch of sixteen: the
/// second letter began at id 8 of a window that started with somebody
/// else's letterhead. A document that fits in a batch starts one.
#[test]
fn a_document_that_fits_in_a_batch_starts_one() {
    let dir = letter_pack("own-batch");
    let manifest = std::fs::read_to_string(dir.join("pack.json")).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace(r#""batch": 8"#, r#""batch": 20"#),
    )
    .unwrap();
    let passages: Vec<String> = (1..=8)
        .map(|n| format!("Paragraph {n} of the first letter."))
        .chain((1..=8).map(|n| format!("Paragraph {n} of the second letter.")))
        .collect();
    let borrowed: Vec<&str> = passages.iter().map(String::as_str).collect();
    std::fs::write(dir.join("fixtures/march.txt"), borrowed[..8].join("\n\n")).unwrap();
    std::fs::write(dir.join("fixtures/june.txt"), borrowed[8..].join("\n\n")).unwrap();

    let pack = load_pack(&dir).expect("the pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", empty_answer(0..8, &borrowed)),
        ("200 OK", empty_answer(8..16, &borrowed)),
    ]);
    run_pack(
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

    assert_eq!(
        ids_shown(&mock.request_body()),
        (0..8).collect::<Vec<_>>(),
        "the first batch is the first letter and nothing else"
    );
    assert_eq!(
        ids_shown(&mock.request_body()),
        (8..16).collect::<Vec<_>>(),
        "the second letter starts a batch of its own, still numbered across the step"
    );
}

/// A named passage must be one the model was shown (#624; `app/METHOD.md`
/// §0 outcome 1). The batch is the window: a model answering ids 2–5
/// can write `amount_from: 6`, and if 6 is a row of the same letter the
/// naming was accepted and "verified" against a passage the model never
/// saw. The page cannot vouch for it, so the reading is refused — no
/// `priced_by`, the amount stays `no amount`, and the staged finder
/// does not go looking either, because a refused naming is not "named
/// nothing". The obligation itself stands, and its trace says why the
/// sum does not.
///
/// The second letter has more passages than the batch holds, so it is
/// the one shape that still straddles a boundary after a document
/// boundary starts a batch.
#[test]
fn a_named_passage_outside_the_batch_answered_is_refused() {
    let dir = letter_pack("outside-window");
    let manifest = std::fs::read_to_string(dir.join("pack.json")).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace(r#""batch": 8"#, r#""batch": 4"#),
    )
    .unwrap();
    let march: Vec<&str> = MARCH_LETTER.split("\n\n").collect();
    let june = [
        "20 June 2026",
        "Please pay the total shown below to Selly Oak Water within 14 days of the date of \
         this letter.",
        "Your account number is 0044 2210.",
        "Charges for the period 1 April to 30 June 2026.",
        "Total £80.00",
    ];
    std::fs::write(dir.join("fixtures/june.txt"), june.join("\n\n")).unwrap();

    let ask = |id: usize, segment: &str, party: &str, amount: serde_json::Value| {
        let mut obligation = serde_json::json!({
            "kind": "payment", "party": { "at": id, "value": party }, "ask": "Pay what is owed",
            "deadline": { "at": id, "value": "within 14 days" }, "anchor": "the date of this letter",
        });
        for (field, value) in amount.as_object().expect("amount fields") {
            obligation[field] = value.clone();
        }
        serde_json::json!({
            "id": id, "segment": segment, "confidence": "high", "obligations": [obligation]
        })
    };
    let quiet = |id: usize, segment: &str| serde_json::json!({ "id": id, "segment": segment, "confidence": "high", "obligations": [] });
    let envelope = |results: Vec<serde_json::Value>| {
        (
            "200 OK",
            completion_envelope(&serde_json::json!({ "results": results }).to_string()),
        )
    };
    // Batches: the March letter (ids 0–1); the June letter's first four
    // passages (2–5); its totals row alone (6). The June ask, answered
    // in the second batch, names the row in the third.
    let mock = MockModel::respond_sequence(vec![
        envelope(vec![
            quiet(0, march[0]),
            ask(
                1,
                march[1],
                "Harborne Parking Services",
                serde_json::json!({ "amount": { "at": 1, "value": "£120.00" } }),
            ),
        ]),
        envelope(vec![
            quiet(2, june[0]),
            ask(
                3,
                june[1],
                "Selly Oak Water",
                serde_json::json!({ "amount": { "at": 6, "value": "£80.00" } }),
            ),
            quiet(4, june[2]),
            quiet(5, june[3]),
        ]),
        envelope(vec![quiet(6, june[4])]),
    ]);

    let outcome = run_pack(
        &pack_at(&dir),
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
    let water = extraction
        .obligations
        .iter()
        .find(|obligation| obligation.party.value == "Selly Oak Water")
        .unwrap_or_else(|| panic!("the obligation stands: {:#?}", extraction.obligations));

    // The reading is refused, never the obligation.
    assert_eq!(
        water.amount.value, "",
        "a sum read off a passage the model never saw"
    );
    assert!(
        water.priced_by.is_none(),
        "nothing vouches for the row, and nothing goes looking: {:?}",
        water.priced_by
    );
    assert_eq!(
        water.due.map(|due| due.date.to_string()),
        Some("2026-07-04".to_owned()),
        "the ask itself is untouched"
    );
    // The March ask's own passage carried its sum: no naming, nothing refused.
    let parking = extraction
        .obligations
        .iter()
        .find(|obligation| obligation.party.value == "Harborne Parking Services")
        .expect("the first letter's obligation");
    assert_eq!(parking.amount.value, "£120.00");

    // The trace records the check on the claim that named the passage.
    let trace = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::Obligation && trace.item == 3)
        .expect("the June ask has a trace");
    assert_eq!(
        trace.check_for("amount", Guardrail::PassageShown),
        Some(CheckOutcome::Failed),
        "the trace says the named passage was outside the batch answered: {trace:#?}"
    );
    assert_eq!(trace.terminal, TerminalDisposition::Accepted);
    let march_trace = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::Obligation && trace.item == 1)
        .expect("the March ask has a trace");
    assert_eq!(
        march_trace.check_for("amount", Guardrail::PassageShown),
        Some(CheckOutcome::Passed),
        "its own passage, shown"
    );
}

fn pack_at(dir: &std::path::Path) -> runner::packs::Pack {
    load_pack(dir).expect("the pack loads")
}

/// One invented letter whose ask names its sum elsewhere on the page,
/// with a second figure printed on another row so a naming that
/// slipped its window would find one to accept. Ids 0–3 in one batch.
const NAMED_LETTER: [&str; 4] = [
    "Dated 20 June 2026.",
    "Please pay the total shown below to Selly Oak Water within 14 days of the date of this \
     letter.",
    "Balance brought forward £80.00",
    "Total £80.00",
];

fn named_letter_pack(name: &str) -> PathBuf {
    let dir = letter_pack(name);
    std::fs::write(dir.join("fixtures/named.txt"), NAMED_LETTER.join("\n\n")).unwrap();
    dir
}

fn quiet_result(id: usize) -> serde_json::Value {
    serde_json::json!({
        "id": id, "segment": NAMED_LETTER[id], "confidence": "high", "obligations": []
    })
}

fn ask_result(
    id: usize,
    confidence: &str,
    obligations: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id, "segment": NAMED_LETTER[id], "confidence": confidence,
        "obligations": obligations
    })
}

/// An ask read from passage 1 — the one that names the payee — with
/// the sum named at `amount_from`. `ask` tells two asks apart, since
/// the verified party is the same words on the same page for both.
fn payment(ask: &str, amount_from: usize) -> serde_json::Value {
    serde_json::json!({
        "kind": "payment", "party": { "at": 1, "value": "Selly Oak Water" }, "ask": ask,
        "deadline": { "at": 1, "value": "within 14 days" }, "anchor": "the date of this letter",
        "amount": { "at": amount_from, "value": "£80.00" },
    })
}

fn results_envelope(results: Vec<serde_json::Value>) -> (&'static str, String) {
    (
        "200 OK",
        completion_envelope(&serde_json::json!({ "results": results }).to_string()),
    )
}

fn run_named_letter(dir: &std::path::Path, mock: &MockModel) -> runner::run::RunOutcome {
    run_pack(
        &pack_at(dir),
        &[dir.join("fixtures/named.txt")],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes")
}

fn obligation_of<'a>(outcome: &'a runner::run::RunOutcome, ask: &str) -> &'a Obligation {
    let Payload::Extraction(extraction) = &outcome.payload else {
        panic!("an obligations pack produces the Extraction payload");
    };
    extraction
        .obligations
        .iter()
        .find(|obligation| obligation.ask == ask)
        .unwrap_or_else(|| panic!("the obligation stands: {:#?}", extraction.obligations))
}

fn passage_shown(
    outcome: &runner::run::RunOutcome,
    item: usize,
    named: usize,
) -> Option<CheckOutcome> {
    outcome
        .claim_traces
        .iter()
        .find(|trace| {
            trace.kind == ClaimKind::Obligation
                && trace.item == item
                && trace.candidate["amount"]["at"].as_u64() == Some(named as u64)
        })
        .unwrap_or_else(|| {
            panic!(
                "the ask naming {named} has a trace: {:#?}",
                outcome.claim_traces
            )
        })
        .check_for("amount", Guardrail::PassageShown)
}

/// The window is the request the answer came from, not the batch it
/// was first asked in (review of #626). A pairing retry re-asks only
/// the items that failed, so an answer produced from passage 1 alone
/// cannot vouch for passage 3, even though the abandoned first request
/// showed both.
#[test]
fn a_pairing_retry_answer_is_checked_against_the_retry_request() {
    let dir = named_letter_pack("retry-window");
    let mock = MockModel::respond_sequence(vec![
        // Passage 1 left out of the results: re-asked alone.
        results_envelope(vec![quiet_result(0), quiet_result(2), quiet_result(3)]),
        results_envelope(vec![ask_result(
            1,
            "high",
            vec![payment("Pay what is owed", 3)],
        )]),
    ]);
    let outcome = run_named_letter(&dir, &mock);

    let first = mock.request_body();
    assert!(
        first.contains("Total £80.00"),
        "the first ask showed the row: {first}"
    );
    let retry = mock.request_body();
    assert!(
        retry.contains("Please pay the total") && !retry.contains("Total £80.00"),
        "the retry showed passage 1 alone: {retry}"
    );

    let water = obligation_of(&outcome, "Pay what is owed");
    assert_eq!(
        water.amount.value, "",
        "a sum read off a row the retry never showed"
    );
    assert!(water.priced_by.is_none());
    assert_eq!(
        water.due.map(|due| due.date.to_string()),
        Some("2026-07-04".to_owned()),
        "the ask itself is untouched"
    );
    assert_eq!(passage_shown(&outcome, 1, 3), Some(CheckOutcome::Failed));
}

/// A truncated batch is halved and each half is its own request: an
/// answer from the left half was shown the left half's passages only.
#[test]
fn a_split_answer_is_checked_against_its_own_half() {
    let dir = named_letter_pack("split-window");
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            support::truncated_envelope(r#"{"results": [{"id": 0, "segment": "Dated"#),
        ),
        // Left half: ids 0–1. The ask names the row in the right half.
        results_envelope(vec![
            quiet_result(0),
            ask_result(1, "high", vec![payment("Pay what is owed", 3)]),
        ]),
        // Right half: ids 2–3.
        results_envelope(vec![quiet_result(2), quiet_result(3)]),
    ]);
    let outcome = run_named_letter(&dir, &mock);

    let _first = mock.request_body();
    let left = mock.request_body();
    assert!(
        left.contains("Please pay the total") && !left.contains("Total £80.00"),
        "the left half showed ids 0–1: {left}"
    );

    let water = obligation_of(&outcome, "Pay what is owed");
    assert_eq!(
        water.amount.value, "",
        "the abandoned first request does not count"
    );
    assert!(water.priced_by.is_none());
    assert_eq!(passage_shown(&outcome, 1, 3), Some(CheckOutcome::Failed));
}

/// A retry can carry ids with a gap — here 1 and 3 — and the set is
/// exactly those: a range from 1 to 3 would vouch for passage 2, which
/// prints a figure the ask could then be priced off.
#[test]
fn a_noncontiguous_retry_is_exactly_its_ids() {
    let dir = named_letter_pack("gap-window");
    let mock = MockModel::respond_sequence(vec![
        // Passages 1 and 3 left out: re-asked together.
        results_envelope(vec![quiet_result(0), quiet_result(2)]),
        results_envelope(vec![
            ask_result(
                1,
                "high",
                vec![payment("Pay the balance", 2), payment("Pay the total", 3)],
            ),
            quiet_result(3),
        ]),
    ]);
    let outcome = run_named_letter(&dir, &mock);

    let _first = mock.request_body();
    let retry = mock.request_body();
    assert!(
        retry.contains("Please pay the total")
            && retry.contains("Total £80.00")
            && !retry.contains("Balance brought forward"),
        "the retry showed ids 1 and 3 and not 2: {retry}"
    );

    let gap = obligation_of(&outcome, "Pay the balance");
    assert_eq!(
        gap.amount.value, "",
        "passage 2 sits inside the range and outside the set"
    );
    assert!(gap.priced_by.is_none());
    assert_eq!(passage_shown(&outcome, 1, 2), Some(CheckOutcome::Failed));

    let shown = obligation_of(&outcome, "Pay the total");
    assert_eq!(
        shown.amount.value, "£80.00",
        "passage 3 was in the retry, and prints the sum"
    );
    assert_eq!(
        shown.priced_by.as_ref().map(|row| row.text.as_str()),
        Some("Total £80.00")
    );
    assert_eq!(passage_shown(&outcome, 1, 3), Some(CheckOutcome::Passed));
}

/// A low-confidence answer travels through review rather than
/// `answers`, and carries its provenance just the same: its naming is
/// refused, and it stays a reading to check.
#[test]
fn a_low_confidence_retry_keeps_its_review_status_and_is_checked() {
    let dir = named_letter_pack("low-window");
    let mock = MockModel::respond_sequence(vec![
        results_envelope(vec![quiet_result(0), quiet_result(2), quiet_result(3)]),
        results_envelope(vec![ask_result(
            1,
            "low",
            vec![payment("Pay what is owed", 3)],
        )]),
    ]);
    let outcome = run_named_letter(&dir, &mock);

    let water = obligation_of(&outcome, "Pay what is owed");
    assert_eq!(water.confidence, "low");
    assert_eq!(water.amount.value, "");
    assert!(water.priced_by.is_none());
    assert_eq!(passage_shown(&outcome, 1, 3), Some(CheckOutcome::Failed));
    // The passage's own reading is routed for review, as any
    // low-confidence answer is; the naming check does not change that.
    let reading = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::Decision && trace.item == 1)
        .expect("the passage has a reading trace");
    assert_eq!(reading.terminal, TerminalDisposition::NeedsReview);
    assert_eq!(
        reading.check(Guardrail::ReviewRouting),
        Some(CheckOutcome::Passed)
    );
}
