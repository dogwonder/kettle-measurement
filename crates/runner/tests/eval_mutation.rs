//! #426: semantic mutation testing — prove the safeguards catch known
//! harms instead of waiting for a model to make a useful mistake.
//!
//! The harness deliberately alters recorded candidate answers, replays
//! them through the downstream pipeline without any model, and asserts
//! that a guardrail contains each change or the eval verdict fails. A
//! mutant nothing catches is the finding: it names the missing guard.

use runner::eval::mutation::{MutationHarness, MutationOperator};
use runner::eval::oracle;
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use std::path::PathBuf;

/// A letter with two real obligations and one passage asking nothing.
/// Small enough to read, large enough that the per-fixture gate fits.
/// Wholly invented.
const LETTER: &str = "3 March 2026\n\nPlease pay £120.00 to Harborne \
Parking Services within 14 days of the date of this letter.\n\nPlease \
also return the enclosed reply slip to Harborne Parking Services by \
28 March 2026.\n\nWe thank you for your co-operation.";

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// A letter pack with a declared bar and gate, so a mutated replay has
/// a verdict to fail.
fn letter_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-mutation-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-mutation",
          "name": "Mutation test",
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
          "eval": { "obligations": 0.5 },
          "eval_gate": "per_fixture",
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Tracks how many passages a person reads.", "date": "2026-08-07" } },
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
          "fixture_id": "mutation-letter-01",
          "eval_set": "development",
          "obligations": [
            {
              "id": "mutation-payment-01",
              "strata": [],
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
              "id": "mutation-response-01",
              "strata": [],
              "segment": "Please also return the enclosed reply slip to Harborne Parking Services by 28 March 2026.",
              "expect": {
                "kind": "response",
                "party": "Harborne Parking Services",
                "deadline": "by 28 March 2026",
                "anchor": "28 March 2026",
                "due": "2026-03-28"
              }
            },
            {
              "id": "mutation-closing-01",
              "strata": [],
              "segment": "We thank you for your co-operation.",
              "expect": null
            }
          ]
        }"#,
    );
    dir
}

/// The issue's first test. An oracle recording answers the bed exactly
/// as authored, so its replay passes; removing exactly one recorded
/// obligation and replaying — no model anywhere — must move the named
/// scored item from found to absent and fail the fixture's gate, and
/// the harness must report the mutant killed.
#[test]
fn dropping_a_recorded_obligation_makes_the_extraction_gate_fail() {
    let dir = letter_pack("drop");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::DropObligation])
        .expect("the harness runs");

    assert_eq!(report.mutation_version, 1);
    let drops: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::DropObligation)
        .collect();
    assert_eq!(
        drops.len(),
        2,
        "one mutant per recorded obligation: {:?}",
        report.records
    );
    for record in &drops {
        assert!(
            record.killed,
            "a dropped obligation must not survive: {record:?}"
        );
        assert!(
            record.verdict_moved,
            "the gate must fail on the mutated replay: {record:?}"
        );
    }
    let affected: Vec<&str> = drops
        .iter()
        .flat_map(|record| record.affected_items.iter())
        .map(String::as_str)
        .collect();
    assert!(
        affected.contains(&"app.kttl.test-mutation/mutation-letter-01/mutation-payment-01")
            && affected.contains(&"app.kttl.test-mutation/mutation-letter-01/mutation-response-01"),
        "each mutant names the scored item it moved: {affected:?}"
    );
    assert!(
        report.survivors().is_empty(),
        "nothing survives here: {:?}",
        report.survivors()
    );
}

/// The mutants live in in-memory clones; the recording handed in is
/// the same object, byte for byte, when the harness returns.
#[test]
fn the_original_recording_is_byte_for_byte_untouched() {
    let dir = letter_pack("untouched");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");
    let before = recording.clone();

    let harness = MutationHarness { machine: machine() };
    harness
        .run(&pack, &recording, &[MutationOperator::DropObligation])
        .expect("the harness runs");

    assert_eq!(
        recording, before,
        "mutation must never write into its source"
    );
}

/// A question the recording cannot answer is refused with a sentence,
/// never improvised and never fetched — replay is structurally
/// incapable of calling a model, and the harness inherits that.
#[test]
fn mutation_replay_never_calls_a_model() {
    let dir = letter_pack("no-model");
    let pack = load_pack(&dir).expect("the pack loads");
    let empty = runner::eval::replay::Recording::default();

    let harness = MutationHarness { machine: machine() };
    let refusal = harness
        .run(&pack, &empty, &[MutationOperator::DropObligation])
        .expect_err("an empty recording cannot pass its baseline replay");

    assert!(
        refusal.contains("no recorded answer") || refusal.contains("does not pass"),
        "the refusal is a sentence, not a network attempt: {refusal}"
    );
}

/// Each operator names the harm it plants and where the system is
/// expected to catch it — the declaration the report is read against.
#[test]
fn every_operator_declares_its_harm_and_expected_containment() {
    for operator in MutationOperator::ALL {
        assert!(
            !operator.harm().is_empty(),
            "{operator:?} must name its harm"
        );
        // The declaration itself is the assertion: expected_containment
        // is total over the enum, so a new operator cannot compile
        // without declaring where it is caught.
        let _ = operator.expected_containment();
    }
}

/// Inventing an obligation in a passage that asks nothing must be
/// caught: the invented claim is a wrong assertion and the gate fails.
#[test]
fn inventing_an_obligation_in_a_silent_passage_is_killed() {
    let dir = letter_pack("invent");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::InventObligation])
        .expect("the harness runs");

    let invents: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::InventObligation)
        .collect();
    assert!(
        !invents.is_empty(),
        "the closing passage is a site: {:?}",
        report.records
    );
    for record in &invents {
        assert!(record.killed, "an invention must not survive: {record:?}");
    }
    assert!(
        invents.iter().any(|record| {
            record.affected_items.contains(
                &"app.kttl.test-mutation/mutation-letter-01/mutation-closing-01".to_owned(),
            )
        }),
        "the silent passage's item moves: {invents:?}"
    );
}

/// One digit moved in a deadline is the harm class Kettle exists for.
/// The mutated phrase resolves to a different date, the join misses,
/// and the gate fails.
#[test]
fn changing_one_digit_in_a_deadline_is_killed() {
    let dir = letter_pack("digit");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::DeadlineDigit])
        .expect("the harness runs");

    let digits: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::DeadlineDigit)
        .collect();
    assert!(
        !digits.is_empty(),
        "every recorded deadline with a digit is a site: {:?}",
        report.records
    );
    for record in &digits {
        assert!(
            record.killed,
            "a moved deadline must not survive: {record:?}"
        );
        assert_ne!(
            record.before, record.after,
            "the mutation names what it changed"
        );
    }
}

/// Omitting one batch answer must be contained, not scored as if the
/// model had honestly found nothing. Pairing rejects the damaged
/// answer and demands a re-ask — a question the recording never heard,
/// so the demand surfaces as a refusal under replay; in live operation
/// the failed re-ask lands in needs-review. Either way the mutant dies.
#[test]
fn omitting_a_batch_answer_is_rejected_and_a_reask_demanded() {
    let dir = letter_pack("omit");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::BatchOmit])
        .expect("the harness runs");

    let omits: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::BatchOmit)
        .collect();
    assert_eq!(
        omits.len(),
        4,
        "every batch answer is a site, the dateline and closing included: {:?}",
        report.records
    );
    for record in &omits {
        assert!(
            record.killed,
            "an omitted answer must not survive: {record:?}"
        );
        assert!(
            record.retry_demanded || record.contained_in_review,
            "the containment is a rejection or review, never a wrong assertion: {record:?}"
        );
    }
}

/// Two wholly invented schedules for the comparison operators.
const PREVIOUS: &str = "Your policy schedule for the year to 31 August 2026.\n\n\
Compulsory excess: £250 per claim.";

const RENEWAL: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.";

/// A two-role comparison pack with a declared bar, so a mutated replay
/// has a verdict to fail.
fn comparison_pack(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kettle-mutation-cmp-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-mutation-renewal",
          "name": "Renewal mutation test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [
            { "role": "previous", "label": "Last year's policy", "accept": ["text/plain"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["text/plain"], "multiple": false }
          ],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "policy-terms", "prompt": "prompts/terms.md", "schema": "schemas/terms.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:term-diff" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "value_kinds": { "compulsory_excess": "money" },
          "eval": { "policy-terms": 0.5 },
          "eval_gate": "per_fixture",
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Surfacing uncertainty is a cost, never a wrong answer.", "date": "2026-08-07" } },
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/terms.md",
        "Which named terms does each passage state?\n{{ batch_json }}\n",
    );
    write(
        "schemas/terms.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "terms": { "type": "array", "items": { "type": "object", "properties": {
                    "term": { "enum": ["compulsory_excess", "other"] },
                    "basis": { "enum": ["per_claim", "per_policy"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/renewal-01-previous.txt", PREVIOUS);
    write("fixtures/renewal-01-renewal.txt", RENEWAL);
    let previous: Vec<&str> = PREVIOUS.split("\n\n").collect();
    let renewal: Vec<&str> = RENEWAL.split("\n\n").collect();
    let expected = serde_json::json!({
        "fixture_id": "mutation-renewal-01",
        "eval_set": "development",
        "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "mutation-prev-dateline", "strata": [],
              "role": "previous", "segment": previous[0], "expect": null },
            { "id": "mutation-prev-excess", "strata": [],
              "role": "previous", "segment": previous[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250", "quote": "Compulsory excess: £250 per claim." } },
            { "id": "mutation-renewal-dateline", "strata": [],
              "role": "renewal", "segment": renewal[0], "expect": null },
            { "id": "mutation-renewal-excess", "strata": [],
              "role": "renewal", "segment": renewal[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500", "quote": "Compulsory excess: £500 per claim." } }
        ]
    });
    write(
        "fixtures/renewal-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

/// Issue #426's second named test. A real quote lifted from the other
/// document is verbatim somewhere — just not in the passage the claim
/// is about. The quote guardrail routes it to a person; it never
/// becomes a supported finding, and the mutant is killed by
/// containment, not by luck.
#[test]
fn a_real_quote_from_the_wrong_document_does_not_count_as_supported() {
    let dir = comparison_pack("wrong-doc");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::WrongDocumentQuote])
        .expect("the harness runs");

    let swaps: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::WrongDocumentQuote)
        .collect();
    assert_eq!(
        swaps.len(),
        2,
        "each recorded term can borrow the other's quote: {:?}",
        report.records
    );
    for record in &swaps {
        assert!(
            record.killed,
            "a borrowed quote must not survive: {record:?}"
        );
        assert!(
            record.contained_in_review || record.retry_demanded,
            "the claim is contained, never asserted as supported: {record:?}"
        );
    }
}

/// Swapping the two documents' values for the same term is the harm a
/// comparison exists to prevent: a £250 rise reads as a £250 cut. The
/// swapped values keep their own quotes, so every guardrail short of
/// the join passes — scoring must kill it.
#[test]
fn swapping_previous_and_current_values_is_killed_by_scoring() {
    let dir = comparison_pack("swap");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::SwapRoles])
        .expect("the harness runs");

    let swaps: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::SwapRoles)
        .collect();
    assert_eq!(
        swaps.len(),
        1,
        "the excess pair is the one swappable site: {:?}",
        report.records
    );
    let record = &swaps[0];
    assert!(record.killed, "a swapped pair must not survive: {record:?}");
    assert!(
        record.verdict_moved,
        "both values are wrong assertions, so the gate fails: {record:?}"
    );
    assert_eq!(
        record.affected_items.len(),
        2,
        "both documents' items move: {record:?}"
    );
}

/// A quote containing two figures supports one of them per term. The
/// wrong pick keeps a verbatim quote in the right passage — only the
/// join can say the value is wrong, and it must.
#[test]
fn picking_the_wrong_value_from_a_multi_value_quote_is_killed() {
    let dir = comparison_pack("multi-value");
    // Give both excess passages a second figure, so the recorded quote
    // carries two values and the operator has something to pick.
    let previous = "Your policy schedule for the year to 31 August 2026.\n\n\
Compulsory excess: £250 per claim. Voluntary excess: £100 per claim.";
    let renewal = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim. Voluntary excess: £100 per claim.";
    std::fs::write(dir.join("fixtures/renewal-01-previous.txt"), previous).expect("write");
    std::fs::write(dir.join("fixtures/renewal-01-renewal.txt"), renewal).expect("write");
    let expected = serde_json::json!({
        "fixture_id": "mutation-renewal-01",
        "eval_set": "development",
        "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "mutation-prev-dateline", "strata": [],
              "role": "previous", "segment": previous.split("\n\n").next().unwrap(),
              "expect": null },
            { "id": "mutation-prev-excess", "strata": [],
              "role": "previous", "segment": previous.split("\n\n").nth(1).unwrap(),
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250",
                          "quote": "Compulsory excess: £250 per claim. Voluntary excess: £100 per claim." } },
            { "id": "mutation-renewal-dateline", "strata": [],
              "role": "renewal", "segment": renewal.split("\n\n").next().unwrap(),
              "expect": null },
            { "id": "mutation-renewal-excess", "strata": [],
              "role": "renewal", "segment": renewal.split("\n\n").nth(1).unwrap(),
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500",
                          "quote": "Compulsory excess: £500 per claim. Voluntary excess: £100 per claim." } }
        ]
    });
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    )
    .expect("write");

    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(
            &pack,
            &recording,
            &[MutationOperator::WrongValueFromMultiValueQuote],
        )
        .expect("the harness runs");

    let picks: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::WrongValueFromMultiValueQuote)
        .collect();
    assert_eq!(
        picks.len(),
        2,
        "each two-figure quote is a site: {:?}",
        report.records
    );
    for record in &picks {
        assert!(record.killed, "a wrong pick must not survive: {record:?}");
        assert_ne!(record.before, record.after);
    }
}

/// Duplicating one batch answer must be rejected or deduplicated —
/// never double-counted into two findings from one passage.
#[test]
fn duplicating_a_batch_answer_is_rejected_or_deduplicated() {
    let dir = letter_pack("duplicate");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::BatchDuplicate])
        .expect("the harness runs");

    let duplicates: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::BatchDuplicate)
        .collect();
    assert_eq!(duplicates.len(), 4, "every answer can be doubled");
    for record in &duplicates {
        assert!(
            record.killed,
            "a doubled answer must not survive as a second finding: {record:?}"
        );
    }
}

/// Swapping two answers' ids attaches each reading to the other's
/// passage. Pairing must refuse the mismatched echoes.
#[test]
fn mispairing_batch_ids_is_rejected_by_pairing() {
    let dir = letter_pack("mispair");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::BatchMispair])
        .expect("the harness runs");

    let mispairs: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::BatchMispair)
        .collect();
    assert!(!mispairs.is_empty(), "adjacent answers can trade ids");
    for record in &mispairs {
        assert!(
            record.killed,
            "a mispaired batch must not survive: {record:?}"
        );
        assert!(
            record.retry_demanded || record.contained_in_review,
            "pairing rejects it, one way or the other: {record:?}"
        );
    }
}

/// Reordering batch answers changes nothing: pairing is by id, not
/// position. This operator declares no-effect — the mutant is killed
/// by *invariance*, and any item that moves under it names a defect.
#[test]
fn reordering_batch_answers_changes_nothing() {
    let dir = letter_pack("reorder");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::BatchReorder])
        .expect("the harness runs");

    let reorders: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::BatchReorder)
        .collect();
    assert_eq!(reorders.len(), 1, "one reversal per batch");
    let record = &reorders[0];
    assert!(
        record.killed,
        "invariance held, so the mutant is killed: {record:?}"
    );
    assert!(
        record.affected_items.is_empty() && !record.verdict_moved,
        "nothing may move under a reorder: {record:?}"
    );
}

/// An unresolvable deadline rewritten as a plausible absolute date is
/// the invention Kettle must not let through: the resolved due date
/// diverges from the authored one and scoring kills it.
#[test]
fn inventing_a_date_for_an_unresolvable_deadline_is_killed() {
    let dir = letter_pack("invent-date");
    // Make the response obligation's deadline unresolvable, as a
    // courtesy phrase is: the bed says it resolves to nothing. The
    // letter and the expectation change together — a passage the
    // letter no longer carries would score as absent.
    let letter = LETTER.replace("by 28 March 2026", "as soon as possible");
    std::fs::write(dir.join("fixtures/letter-01.txt"), &letter).expect("write");
    let expected = std::fs::read_to_string(dir.join("fixtures/letter-01.expected.json"))
        .expect("expectations read")
        .replace("by 28 March 2026", "as soon as possible")
        .replace(
            r#""anchor": "28 March 2026",
                "due": "2026-03-28""#,
            r#""anchor": "as soon as possible""#,
        );
    std::fs::write(dir.join("fixtures/letter-01.expected.json"), &expected).expect("write");

    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::InventDate])
        .expect("the harness runs");

    let inventions: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::InventDate)
        .collect();
    assert_eq!(
        inventions.len(),
        1,
        "the digit-free deadline is the one site: {:?}",
        report.records
    );
    let record = &inventions[0];
    assert!(
        record.killed,
        "an invented date must not survive: {record:?}"
    );
}

/// A schema-invalid answer must be rejected at the schema boundary:
/// the retry is demanded with the validation error appended, which a
/// recording has never heard.
#[test]
fn breaking_the_answer_schema_is_rejected_and_a_reask_demanded() {
    let dir = letter_pack("schema-break");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::SchemaBreak])
        .expect("the harness runs");

    let breaks: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::SchemaBreak)
        .collect();
    assert!(!breaks.is_empty(), "every enum field is a site");
    for record in &breaks {
        assert!(
            record.killed,
            "a schema-invalid answer must not survive: {record:?}"
        );
        assert!(
            record.retry_demanded,
            "schema validation rejects it before anything downstream: {record:?}"
        );
    }
}

/// A term outside the pack's contract is the pack-coverage boundary:
/// the reading is refused into review, never silently adopted.
#[test]
fn an_unmodelled_term_is_routed_to_a_person() {
    let dir = comparison_pack("unmodelled");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::UnmodelledTerm])
        .expect("the harness runs");

    let unmodelled: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::UnmodelledTerm)
        .collect();
    assert_eq!(unmodelled.len(), 2, "each recorded term is a site");
    for record in &unmodelled {
        assert!(
            record.killed,
            "an unmodelled term must not survive: {record:?}"
        );
    }
}

/// A value that cannot hold its declared shape is the #380 boundary:
/// referred to a person, never a finding.
#[test]
fn a_value_that_cannot_hold_its_shape_is_routed_to_a_person() {
    let dir = comparison_pack("value-shape");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::ValueShapeBreak])
        .expect("the harness runs");

    let shapes: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::ValueShapeBreak)
        .collect();
    assert_eq!(shapes.len(), 2, "each recorded value is a site");
    for record in &shapes {
        assert!(
            record.killed,
            "a shapeless value must not survive: {record:?}"
        );
        assert!(
            record.contained_in_review || record.retry_demanded,
            "the referral is containment: {record:?}"
        );
    }
}

/// The acceptance criterion: the operator set exercises every
/// model-to-report guardrail a response mutation can reach. The
/// containments Rust owns outright — derivation, review routing, the
/// report and action links — have no response to mutate; they are
/// covered by the deterministic unit tests instead.
#[test]
fn mutants_exercise_every_reachable_guardrail() {
    use runner::claim_trace::Guardrail;
    use runner::eval::mutation::Containment;

    let reached: Vec<Containment> = MutationOperator::ALL
        .iter()
        .map(|operator| operator.expected_containment())
        .collect();
    for guardrail in [
        Guardrail::Schema,
        Guardrail::Pairing,
        Guardrail::Quote,
        Guardrail::ValueShape,
        Guardrail::PackCoverage,
    ] {
        assert!(
            reached.contains(&Containment::Guardrail(guardrail)),
            "no operator exercises {guardrail:?}"
        );
    }
    assert!(reached.contains(&Containment::Scoring));
    assert!(reached.contains(&Containment::NoEffect));
}

/// A letter big enough for provable pooled ceilings, gated the way
/// the real letter pack is: pooled step bar plus a declared stratum
/// every authored item carries. A 0.5 ceiling needs n >= 3.84/0.5 -
/// 3.84 ~ 4 decisions per class, and this letter carries four asks
/// and four silent passages — one wrong distinct decision breaches
/// either ceiling.
fn pooled_letter_pack(name: &str) -> PathBuf {
    let dir = letter_pack(name);
    let manifest = std::fs::read_to_string(dir.join("pack.json")).expect("manifest reads");
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace(
            r#""eval": { "obligations": 0.5 },
          "eval_gate": "per_fixture","#,
            r#""eval": { "obligations": 0.2 },
          "eval_gate": "pooled",
          "eval_strata": {
            "any-letter": {
              "description": "Every letter decision, pooled.",
              "classes": {
                "obligation": { "max_wilson_95": 0.5, "reason": "A wrong deadline is the harm this pack exists to prevent.", "date": "2026-08-07" },
                "no_obligation": { "max_wilson_95": 0.5, "reason": "An invented obligation costs a phone call.", "date": "2026-08-07" }
              }
            }
          },"#,
        ),
    )
    .expect("manifest writes");
    let letter = "3 March 2026\n\nPlease pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.\n\nPlease also return the enclosed reply slip to Harborne Parking Services by 28 March 2026.\n\nPlease attend the hearing at Kelsford Borough Council on 2 May 2026.\n\nPlease pay the outstanding balance of £45.50 to Kelsford Borough Council by 14 April 2026.\n\nOur offices are open Monday to Friday.\n\nThis letter requires no response if you have already paid.\n\nWe thank you for your co-operation.";
    std::fs::write(dir.join("fixtures/letter-01.txt"), letter).expect("write");
    let item = |id: &str, segment: &str, expect: serde_json::Value| serde_json::json!({ "id": id, "strata": ["any-letter"], "segment": segment, "expect": expect });
    let expected = serde_json::json!({
        "fixture_id": "mutation-letter-01",
        "eval_set": "development",
        "obligations": [
            item("pooled-date-01", "3 March 2026", serde_json::Value::Null),
            item("pooled-payment-01",
                 "Please pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.",
                 serde_json::json!({ "kind": "payment", "party": "Harborne Parking Services",
                     "deadline": "within 14 days", "anchor": "the date of this letter", "due": "2026-03-17" })),
            item("pooled-response-01",
                 "Please also return the enclosed reply slip to Harborne Parking Services by 28 March 2026.",
                 serde_json::json!({ "kind": "response", "party": "Harborne Parking Services",
                     "deadline": "by 28 March 2026", "anchor": "28 March 2026", "due": "2026-03-28" })),
            item("pooled-attendance-01",
                 "Please attend the hearing at Kelsford Borough Council on 2 May 2026.",
                 serde_json::json!({ "kind": "attendance", "party": "Kelsford Borough Council",
                     "deadline": "on 2 May 2026", "anchor": "2 May 2026", "due": "2026-05-02" })),
            item("pooled-payment-02",
                 "Please pay the outstanding balance of £45.50 to Kelsford Borough Council by 14 April 2026.",
                 serde_json::json!({ "kind": "payment", "party": "Kelsford Borough Council",
                     "deadline": "by 14 April 2026", "anchor": "14 April 2026", "due": "2026-04-14" })),
            item("pooled-no-ask-01", "Our offices are open Monday to Friday.", serde_json::Value::Null),
            item("pooled-no-ask-02", "This letter requires no response if you have already paid.", serde_json::Value::Null),
            item("pooled-no-ask-03", "We thank you for your co-operation.", serde_json::Value::Null)
        ]
    });
    std::fs::write(
        dir.join("fixtures/letter-01.expected.json"),
        serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    )
    .expect("write");
    dir
}

/// #442: an obligation invented on a passage the bed never authored
/// must be scored as the invention it is, not fall between the
/// authored expectations. The letter here carries one passage with no
/// expectation at all — the harness must kill an invention on it.
#[test]
fn an_obligation_invented_on_an_unscored_passage_is_killed() {
    // Pooled, as the real letter pack gates: the per-fixture path
    // catches unauthored inventions through END_TO_END_BAR, but a
    // pooled verdict reads step bars and ceilings only — which is
    // exactly how 1,750 of these escaped on the real bed (#442).
    let dir = pooled_letter_pack("unscored-invention");
    // A passage the bed says nothing about, before the closing. The
    // authored expectations are untouched.
    let letter = std::fs::read_to_string(dir.join("fixtures/letter-01.txt"))
        .expect("letter reads")
        .replace(
            "\n\nWe thank you",
            "\n\nPlease quote your reference in all correspondence.\n\nWe thank you",
        );
    std::fs::write(dir.join("fixtures/letter-01.txt"), &letter).expect("write");

    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::InventObligation])
        .expect("the harness runs");

    let inventions: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::InventObligation)
        .collect();
    assert!(
        inventions.len() >= 3,
        "the unauthored passage is a site too: {:?}",
        report.records
    );
    for record in &inventions {
        assert!(
            record.killed,
            "an invention must not survive anywhere, authored or not: {record:?}"
        );
    }
}

/// #443: a found-but-wrong assertion is the harm the ceilings exist
/// for — a person told the wrong deadline confidently. It must count
/// confident-wrong in the expected class and fail a declared ceiling,
/// with the step bar set low enough that the ceiling is what kills.
#[test]
fn a_wrong_assertion_fails_the_harm_ceiling() {
    let dir = pooled_letter_pack("wrong-assertion");

    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::DeadlineDigit])
        .expect("the harness runs");

    let digits: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::DeadlineDigit)
        .collect();
    assert!(!digits.is_empty(), "digit deadlines are sites");
    for record in &digits {
        assert!(
            record.killed,
            "a wrong assertion must fail the ceiling: {record:?}"
        );
    }
}

/// Rich's taxonomy decision (#426): forcing every confidence low is a
/// cost mutation, not a harm — the answers stay right. The safeguard
/// on trial is that the system responds to uncertainty by surfacing:
/// the mutant is killed by the cost shift, the review rate rising,
/// and would survive only if forced-low readings sailed through as
/// confident assertions.
#[test]
fn forcing_confidence_low_is_killed_by_the_cost_shift() {
    let dir = letter_pack("force-low");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::ForceConfidenceLow])
        .expect("the harness runs");

    let forced: Vec<_> = report
        .records
        .iter()
        .filter(|record| record.operator == MutationOperator::ForceConfidenceLow)
        .collect();
    assert!(
        !forced.is_empty(),
        "every high-confidence answer is a site: {:?}",
        report.records
    );
    for record in &forced {
        assert!(
            record.killed,
            "uncertainty must surface, not sail through: {record:?}"
        );
        assert!(
            record.cost_shift || record.contained_in_review,
            "the kill is a cost shift, not a verdict failure: {record:?}"
        );
    }
}

/// Forcing confidence high has no observable harm on a correct
/// recording — false confidence only bites on wrong answers, which an
/// oracle recording cannot contain by construction. The operator
/// declares that: no sites on an all-high recording, meaning only on
/// real model recordings via `mutate --recording`.
#[test]
fn forcing_confidence_high_has_no_sites_on_an_oracle_recording() {
    let dir = letter_pack("force-high");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::ForceConfidenceHigh])
        .expect("the harness runs");

    assert!(
        report
            .records
            .iter()
            .all(|record| record.operator != MutationOperator::ForceConfidenceHigh),
        "an all-high recording offers nothing to force: {:?}",
        report.records
    );
}

const REFERRAL_PREVIOUS: &str = "Your policy schedule for the year to 31 August 2026.\n\n\
Total annual premium: £840.00.\n\n\
Excess: £250.00 each and every claim.";

const REFERRAL_RENEWAL: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Total annual premium: £840.00.\n\n\
Excess: £300.00 each and every claim.";

/// A comparison pack whose bed authors #461's bare excess line in both
/// documents: the correct outcome is a referral, written `review: true`
/// with no term named, exactly as the renewal bed's
/// `excess_unqualified` shape does (#493).
fn referral_pack(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kettle-mutation-ref-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-mutation-referral",
          "name": "Referral mutation test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [
            { "role": "previous", "label": "Last year's policy", "accept": ["text/plain"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["text/plain"], "multiple": false }
          ],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "policy-terms", "prompt": "prompts/terms.md", "schema": "schemas/terms.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:term-diff" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "value_kinds": {
            "premium": "money",
            "compulsory_excess": "money",
            "voluntary_excess": "money",
            "total_excess": "money"
          },
          "term_families": {
            "excess": ["compulsory_excess", "voluntary_excess", "total_excess"]
          },
          "eval": { "policy-terms": 0.5 },
          "eval_gate": "per_fixture",
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Surfacing uncertainty is a cost, never a wrong answer.", "date": "2026-08-12" } },
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/terms.md",
        "Which named terms does each passage state?\n{{ batch_json }}\n",
    );
    write(
        "schemas/terms.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "terms": { "type": "array", "items": { "type": "object", "properties": {
                    "term": { "enum": ["premium", "compulsory_excess", "voluntary_excess", "total_excess", "other"] },
                    "basis": { "enum": ["annual", "per_claim", "per_policy"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/referral-01-previous.txt", REFERRAL_PREVIOUS);
    write("fixtures/referral-01-renewal.txt", REFERRAL_RENEWAL);
    let previous: Vec<&str> = REFERRAL_PREVIOUS.split("\n\n").collect();
    let renewal: Vec<&str> = REFERRAL_RENEWAL.split("\n\n").collect();
    let expected = serde_json::json!({
        "fixture_id": "mutation-referral-01",
        "eval_set": "development",
        "inputs": {
            "previous": "referral-01-previous.txt",
            "renewal": "referral-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "mutation-ref-prev-dateline", "strata": [],
              "role": "previous", "segment": previous[0], "expect": null },
            { "id": "mutation-ref-prev-premium", "strata": [],
              "role": "previous", "segment": previous[1],
              "expect": { "term": "premium", "basis": "annual",
                          "value": "£840.00", "quote": "Total annual premium: £840.00." } },
            { "id": "mutation-ref-prev-excess-unqualified", "strata": [],
              "role": "previous", "segment": previous[2],
              "review": true, "expect": null },
            { "id": "mutation-ref-renewal-dateline", "strata": [],
              "role": "renewal", "segment": renewal[0], "expect": null },
            { "id": "mutation-ref-renewal-premium", "strata": [],
              "role": "renewal", "segment": renewal[1],
              "expect": { "term": "premium", "basis": "annual",
                          "value": "£840.00", "quote": "Total annual premium: £840.00." } },
            { "id": "mutation-ref-renewal-excess-unqualified", "strata": [],
              "role": "renewal", "segment": renewal[2],
              "review": true, "expect": null }
        ]
    });
    write(
        "fixtures/referral-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

/// #484's census blocker, found 12 August: a bed that authors a
/// referral expectation (#461, generated into the renewal bed by #493)
/// must still yield a passing oracle recording, or `kettle mutate`
/// refuses at its own sanity gate and no census can be taken. The
/// oracle answers the bare line as `other` — the escape hatch every
/// terms pack models (#445) — so the pack-coverage guardrail routes it
/// to a person and the review-true expectation is satisfied by the
/// referral the bed authored as the correct outcome.
#[test]
fn a_bed_that_authors_a_referral_still_gets_a_passing_oracle_recording() {
    let dir = referral_pack("referral");
    let pack = load_pack(&dir).expect("the pack loads");
    let recording = oracle::recording(&pack).expect("the oracle answers its own bed");

    let harness = MutationHarness { machine: machine() };
    let report = harness
        .run(&pack, &recording, &[MutationOperator::BatchReorder])
        .expect("the unmutated replay passes the sanity gate");

    assert_eq!(report.pack, "app.kttl.test-mutation-referral");
}
