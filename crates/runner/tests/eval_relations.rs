//! #427: metamorphic and contrastive relations between fixtures.
//!
//! A gold fixture asks whether one answer is right. It cannot ask
//! whether the system is invariant to irrelevant changes, or sensitive
//! to the smallest meaningful one. Relations score the *relationship*
//! between two fixtures' results — declared by the pack, judged by the
//! runner, joined on stable identities and never on output order.

use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::PathBuf;

mod support;
use support::{completion_envelope, MockModel};

/// Two wholly invented schedules. The swap twin binds the same two
/// documents to opposite roles, so the same reading must invert the
/// declared changes: what was added becomes removed, what rose fell.
const OLD_SCHEDULE: &str = "Your policy schedule for the year to 31 August 2026.\n\n\
Compulsory excess: £250 per claim.";

const NEW_SCHEDULE: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.";

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// A comparison pack with two fixtures over the same two documents,
/// roles swapped, and a declared inversion relation between them.
fn swap_twin_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-relations-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-relations",
          "name": "Relations test",
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
    write("fixtures/forward-old.txt", OLD_SCHEDULE);
    write("fixtures/forward-new.txt", NEW_SCHEDULE);
    write("fixtures/swapped-old.txt", OLD_SCHEDULE);
    write("fixtures/swapped-new.txt", NEW_SCHEDULE);
    let old: Vec<&str> = OLD_SCHEDULE.split("\n\n").collect();
    let new: Vec<&str> = NEW_SCHEDULE.split("\n\n").collect();
    // The forward fixture reads the documents the right way round.
    let forward = serde_json::json!({
        "fixture_id": "relations-forward-01",
        "eval_set": "development",
        "inputs": { "previous": "forward-old.txt", "renewal": "forward-new.txt" },
        "policy-terms": [
            { "id": "forward-prev-dateline", "strata": [], "role": "previous",
              "segment": old[0], "expect": null },
            { "id": "forward-prev-excess", "strata": [], "role": "previous",
              "segment": old[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250", "quote": "Compulsory excess: £250 per claim." } },
            { "id": "forward-renewal-dateline", "strata": [], "role": "renewal",
              "segment": new[0], "expect": null },
            { "id": "forward-renewal-excess", "strata": [], "role": "renewal",
              "segment": new[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500", "quote": "Compulsory excess: £500 per claim." } }
        ]
    });
    // The swap twin binds the same documents the other way round.
    let swapped = serde_json::json!({
        "fixture_id": "relations-swapped-01",
        "eval_set": "development",
        "inputs": { "previous": "swapped-new.txt", "renewal": "swapped-old.txt" },
        "policy-terms": [
            { "id": "swapped-prev-dateline", "strata": [], "role": "previous",
              "segment": new[0], "expect": null },
            { "id": "swapped-prev-excess", "strata": [], "role": "previous",
              "segment": new[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500", "quote": "Compulsory excess: £500 per claim." } },
            { "id": "swapped-renewal-dateline", "strata": [], "role": "renewal",
              "segment": old[0], "expect": null },
            { "id": "swapped-renewal-excess", "strata": [], "role": "renewal",
              "segment": old[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250", "quote": "Compulsory excess: £250 per claim." } }
        ]
    });
    write(
        "fixtures/forward.expected.json",
        &serde_json::to_string_pretty(&forward).expect("expectations serialise"),
    );
    write(
        "fixtures/swapped.expected.json",
        &serde_json::to_string_pretty(&swapped).expect("expectations serialise"),
    );
    write(
        "fixtures/relations.json",
        r#"{
          "relations": [
            {
              "id": "swap-inverts-the-diff",
              "kind": { "algebraic": "term_change_inversion" },
              "left": "relations-forward-01",
              "right": "relations-swapped-01"
            }
          ]
        }"#,
    );
    dir
}

/// A faithful reading of one fixture's batch, in that fixture's own
/// document order — the swap twin shows the documents the other way
/// round, so its echoes must match its own questions.
fn faithful_answer(first: &str, second: &str) -> String {
    let first: Vec<&str> = first.split("\n\n").collect();
    let second: Vec<&str> = second.split("\n\n").collect();
    let term = |value: &str| {
        serde_json::json!([{
            "term": "compulsory_excess", "basis": "per_claim",
            "value": value,
            "quote": format!("Compulsory excess: {value} per claim.")
        }])
    };
    let value_in = |segment: &str| {
        if segment.contains("£250") {
            term("£250")
        } else {
            term("£500")
        }
    };
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": first[0], "confidence": "high", "terms": [] },
                { "id": 1, "segment": first[1], "confidence": "high", "terms": value_in(first[1]) },
                { "id": 2, "segment": second[0], "confidence": "high", "terms": [] },
                { "id": 3, "segment": second[1], "confidence": "high", "terms": value_in(second[1]) }
            ]
        })
        .to_string(),
    )
}

fn evaluate(dir: &std::path::Path) -> runner::eval::EvalReport {
    let pack = load_pack(dir).expect("the relations pack loads");
    // Fixtures run in name order: forward, then swapped — each batch
    // shows its documents in its own binding order.
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", faithful_answer(OLD_SCHEDULE, NEW_SCHEDULE)),
        ("200 OK", faithful_answer(NEW_SCHEDULE, OLD_SCHEDULE)),
    ]);
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

/// The issue's named first test. Two role-bound fixtures carry the
/// same values in opposite roles; the declared inversion relation must
/// be stated, judged, and held — the harness can already score both
/// fixtures, and could not previously say anything about their
/// relationship.
#[test]
fn swapping_comparison_roles_inverts_the_declared_term_changes() {
    let dir = swap_twin_pack("inversion");
    let report = evaluate(&dir);

    assert_eq!(
        report.relations.len(),
        1,
        "the declared relation is judged: {:?}",
        report.relations
    );
    let relation = &report.relations[0];
    assert_eq!(relation.id, "swap-inverts-the-diff");
    assert!(
        relation.held(),
        "a faithful reading inverts cleanly: {relation:?}"
    );
}

/// A declared relation that fails must fail the report (#427): a model
/// that reads a role swap as two rises is wrong in a way no per-item
/// gate can see. Here every item is individually correct — the
/// declaration demands invariance where the truth is inversion, so
/// only the relation fails, and the verdict must follow it.
#[test]
fn a_failed_relation_fails_the_verdict() {
    let dir = swap_twin_pack("verdict");
    std::fs::write(
        dir.join("fixtures/relations.json"),
        r#"{
          "relations": [
            {
              "id": "wrongly-declared-invariance",
              "kind": { "invariance": { "projection": "terms_by_role" } },
              "left": "relations-forward-01",
              "right": "relations-swapped-01"
            }
          ]
        }"#,
    )
    .expect("write relations");

    let report = evaluate(&dir);

    assert_eq!(report.fixtures.len(), 2);
    assert!(
        report
            .fixtures
            .iter()
            .all(|fixture| fixture.end_to_end == 1.0),
        "every item is individually right; only the relation is not"
    );
    let relation = &report.relations[0];
    assert!(
        !relation.held(),
        "a role swap does not satisfy invariance: {relation:?}"
    );
    assert_eq!(
        report.verdict,
        runner::eval::Verdict::Fail,
        "a failed relation fails the report"
    );
}

/// Relations are bed content (#427): the same fixtures with different
/// declared relationships are a different measurement, and the bed
/// digest must say so — that is how a baseline from other declarations
/// is refused with a note instead of silently compared.
#[test]
fn relations_participate_in_the_bed_digest() {
    let dir = swap_twin_pack("digest");
    let before = evaluate(&dir).bed.expect("a bed digest");

    let relations = std::fs::read_to_string(dir.join("fixtures/relations.json"))
        .expect("relations read")
        .replace("swap-inverts-the-diff", "swap-inverts-the-diff-renamed");
    std::fs::write(dir.join("fixtures/relations.json"), relations).expect("write");
    let after = evaluate(&dir).bed.expect("a bed digest");

    assert_ne!(before, after, "edited relations must move the bed identity");
}

/// A relation the runner cannot interpret is refused, never skipped —
/// a silently unjudged relation would read as a held one.
#[test]
fn an_uninterpretable_relation_is_refused_not_skipped() {
    let dir = swap_twin_pack("uninterpretable");
    std::fs::write(
        dir.join("fixtures/relations.json"),
        r#"{
          "relations": [
            {
              "id": "a-law-nobody-wrote",
              "kind": { "algebraic": "premium_teleportation" },
              "left": "relations-forward-01",
              "right": "relations-swapped-01"
            }
          ]
        }"#,
    )
    .expect("write relations");

    let pack = load_pack(&dir).expect("the pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", faithful_answer(OLD_SCHEDULE, NEW_SCHEDULE)),
        ("200 OK", faithful_answer(NEW_SCHEDULE, OLD_SCHEDULE)),
    ]);
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
    let refusal = evaluator
        .evaluate(&pack)
        .expect_err("an unknown law is a mistake, not a shrug");
    assert!(
        refusal.contains("premium_teleportation") || refusal.contains("relations"),
        "the refusal names the problem: {refusal}"
    );
}

/// A relation reaching across the development/exam boundary is refused:
/// the two sets are separate measurements, and a relation cannot join
/// what must never be compared.
#[test]
fn a_relation_across_the_set_boundary_is_refused() {
    let dir = swap_twin_pack("cross-set");
    // Reseal the swapped twin as exam: the declared relation now
    // reaches across the boundary.
    let expected = std::fs::read_to_string(dir.join("fixtures/swapped.expected.json"))
        .expect("expectations read")
        .replace(r#""eval_set": "development""#, r#""eval_set": "exam""#);
    std::fs::write(dir.join("fixtures/swapped.expected.json"), expected).expect("write");

    let pack = load_pack(&dir).expect("the pack loads");
    let mock = MockModel::respond_sequence(vec![(
        "200 OK",
        faithful_answer(OLD_SCHEDULE, NEW_SCHEDULE),
    )]);
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
    let refusal = evaluator
        .evaluate(&pack)
        .expect_err("a development run cannot judge a relation into the exam set");
    assert!(
        refusal.contains("relations-swapped-01"),
        "the refusal names the unreachable fixture: {refusal}"
    );
}

// ---------------------------------------------------------------------
// Classification relations (#427, the deferred slice): the subscription
// pack's scored decisions are merchants, so its relations need a
// projection over what was asserted about each one. Merchants below are
// public brands as descriptor text (#257) inside wholly invented
// statements — invented dates, invented amounts, no person's records.

/// The source statement: two merchants, six clean monthly charges each.
const PAIR_SOURCE_CSV: &str = "Date,Description,Amount\n\
2025-01-05,NETFLIX.COM,-10.99\n\
2025-01-12,SPOTIFY LTD,-11.99\n\
2025-02-05,NETFLIX.COM,-10.99\n\
2025-02-12,SPOTIFY LTD,-11.99\n\
2025-03-05,NETFLIX.COM,-10.99\n\
2025-03-12,SPOTIFY LTD,-11.99\n\
2025-04-05,NETFLIX.COM,-10.99\n\
2025-04-12,SPOTIFY LTD,-11.99\n\
2025-05-05,NETFLIX.COM,-10.99\n\
2025-05-12,SPOTIFY LTD,-11.99\n\
2025-06-05,NETFLIX.COM,-10.99\n\
2025-06-12,SPOTIFY LTD,-11.99\n";

/// The same statement with its rows reversed: every row is
/// self-contained, so the meaning has not moved an inch.
const PAIR_REORDERED_CSV: &str = "Date,Description,Amount\n\
2025-06-12,SPOTIFY LTD,-11.99\n\
2025-06-05,NETFLIX.COM,-10.99\n\
2025-05-12,SPOTIFY LTD,-11.99\n\
2025-05-05,NETFLIX.COM,-10.99\n\
2025-04-12,SPOTIFY LTD,-11.99\n\
2025-04-05,NETFLIX.COM,-10.99\n\
2025-03-12,SPOTIFY LTD,-11.99\n\
2025-03-05,NETFLIX.COM,-10.99\n\
2025-02-12,SPOTIFY LTD,-11.99\n\
2025-02-05,NETFLIX.COM,-10.99\n\
2025-01-12,SPOTIFY LTD,-11.99\n\
2025-01-05,NETFLIX.COM,-10.99\n";

/// The source with one merchant's rows removed — the smallest
/// meaningful change: exactly one classification should go with them.
const PAIR_REMOVED_CSV: &str = "Date,Description,Amount\n\
2025-01-05,NETFLIX.COM,-10.99\n\
2025-02-05,NETFLIX.COM,-10.99\n\
2025-03-05,NETFLIX.COM,-10.99\n\
2025-04-05,NETFLIX.COM,-10.99\n\
2025-05-05,NETFLIX.COM,-10.99\n\
2025-06-05,NETFLIX.COM,-10.99\n";

const PAIR_SOURCE_EXPECTED: &str = r#"{
  "fixture_id": "relations-pair-01",
  "normalise": [
    { "raw": "NETFLIX.COM", "name": "Netflix" },
    { "raw": "SPOTIFY LTD", "name": "Spotify" }
  ],
  "classify": [
    { "id": "relations-video-streaming-01", "strata": ["clean"],
      "name": "Netflix", "kind": "subscription", "category": "streaming" },
    { "id": "relations-music-streaming-01", "strata": ["clean"],
      "name": "Spotify", "kind": "subscription", "category": "streaming" }
  ],
  "recurring": [
    { "merchant": "Netflix", "period": "monthly" },
    { "merchant": "Spotify", "period": "monthly" }
  ],
  "tolerances": { "normalise": "fuzzy:0.85", "classify_kind": "exact",
                  "classify_category": "exact", "recurring": "exact" }
}
"#;

const PAIR_REORDERED_EXPECTED: &str = r#"{
  "fixture_id": "relations-pair-01-reordered",
  "normalise": [
    { "raw": "NETFLIX.COM", "name": "Netflix" },
    { "raw": "SPOTIFY LTD", "name": "Spotify" }
  ],
  "classify": [
    { "id": "reorder-relations-video-streaming-01", "strata": ["clean"],
      "name": "Netflix", "kind": "subscription", "category": "streaming" },
    { "id": "reorder-relations-music-streaming-01", "strata": ["clean"],
      "name": "Spotify", "kind": "subscription", "category": "streaming" }
  ],
  "recurring": [
    { "merchant": "Netflix", "period": "monthly" },
    { "merchant": "Spotify", "period": "monthly" }
  ],
  "tolerances": { "normalise": "fuzzy:0.85", "classify_kind": "exact",
                  "classify_category": "exact", "recurring": "exact" }
}
"#;

const PAIR_REMOVED_EXPECTED: &str = r#"{
  "fixture_id": "relations-pair-01-one-removed",
  "normalise": [
    { "raw": "NETFLIX.COM", "name": "Netflix" }
  ],
  "classify": [
    { "id": "removal-relations-video-streaming-01", "strata": ["clean"],
      "name": "Netflix", "kind": "subscription", "category": "streaming" }
  ],
  "recurring": [
    { "merchant": "Netflix", "period": "monthly" }
  ],
  "tolerances": { "normalise": "fuzzy:0.85", "classify_kind": "exact",
                  "classify_category": "exact", "recurring": "exact" }
}
"#;

/// The three statement fixtures the classification relations are
/// declared over, in a scratch directory the pack's own fixtures never
/// see. Each test writes its own `relations.json` beside them.
fn classification_fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kettle-relations-classify-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let write = |file: &str, content: &str| {
        std::fs::write(dir.join(file), content).expect("write fixture");
    };
    write("pair-a.csv", PAIR_SOURCE_CSV);
    write("pair-a.expected.json", PAIR_SOURCE_EXPECTED);
    write("pair-b-reordered.csv", PAIR_REORDERED_CSV);
    write("pair-b-reordered.expected.json", PAIR_REORDERED_EXPECTED);
    write("pair-c-removed.csv", PAIR_REMOVED_CSV);
    write("pair-c-removed.expected.json", PAIR_REMOVED_EXPECTED);
    dir
}

/// A normalise answer echoing the raws in the order this fixture's
/// statement first shows them, and the classify answer that follows.
fn merchant_answers(merchants: &[(&str, &str)], confidence: &str) -> Vec<(&'static str, String)> {
    let normalise: Vec<serde_json::Value> = merchants
        .iter()
        .enumerate()
        .map(|(id, (raw, name))| {
            serde_json::json!({ "id": id, "raw": raw, "name": name, "recognised": true })
        })
        .collect();
    let classify: Vec<serde_json::Value> = merchants
        .iter()
        .enumerate()
        .map(|(id, (_, name))| {
            serde_json::json!({
                "id": id, "name": name, "kind": "subscription",
                "category": "streaming", "confidence": confidence
            })
        })
        .collect();
    vec![
        (
            "200 OK",
            completion_envelope(&serde_json::json!({ "results": normalise }).to_string()),
        ),
        (
            "200 OK",
            completion_envelope(&serde_json::json!({ "results": classify }).to_string()),
        ),
    ]
}

const NETFLIX: (&str, &str) = ("NETFLIX.COM", "Netflix");
const SPOTIFY: (&str, &str) = ("SPOTIFY LTD", "Spotify");

/// Faithful answers for all three fixtures, in fixture-name order —
/// each normalise echoes its own statement's first-appearance order.
fn faithful_classification_answers() -> Vec<(&'static str, String)> {
    let mut answers = merchant_answers(&[NETFLIX, SPOTIFY], "high");
    answers.extend(merchant_answers(&[SPOTIFY, NETFLIX], "high"));
    answers.extend(merchant_answers(&[NETFLIX], "high"));
    answers
}

fn evaluate_classification(
    dir: &std::path::Path,
    responses: Vec<(&'static str, String)>,
) -> runner::eval::EvalReport {
    let pack_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit");
    let pack = load_pack(&pack_dir).expect("the subscription pack loads");
    let mock = MockModel::respond_sequence(responses);
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(dir.to_path_buf()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs")
}

/// The meaning-preserving pair: the same statement with its rows
/// reordered must yield the same classification of every merchant —
/// which is what the `classifications_by_merchant` projection compares.
/// Red until the runner's vocabulary carries that projection.
#[test]
fn reordering_statement_rows_holds_the_classification_projection() {
    let dir = classification_fixture_dir("reorder");
    std::fs::write(
        dir.join("relations.json"),
        r#"{
          "relations": [
            {
              "id": "reorder-holds",
              "kind": { "invariance": { "projection": "classifications_by_merchant" } },
              "left": "relations-pair-01",
              "right": "relations-pair-01-reordered"
            }
          ]
        }"#,
    )
    .expect("write relations");

    let report = evaluate_classification(&dir, faithful_classification_answers());

    assert_eq!(
        report.relations.len(),
        1,
        "the declared relation is judged: {:?}",
        report.relations
    );
    let relation = &report.relations[0];
    assert_eq!(relation.id, "reorder-holds");
    assert!(
        relation.held(),
        "row order is not meaning; the projection must match: {relation:?}"
    );
}

/// The meaning-changing pair: removing one merchant's rows must remove
/// exactly that merchant's classification and nothing else.
#[test]
fn removing_one_merchants_rows_removes_exactly_that_classification() {
    let dir = classification_fixture_dir("removal");
    std::fs::write(
        dir.join("relations.json"),
        r#"{
          "relations": [
            {
              "id": "removal-drops-one-merchant",
              "kind": { "algebraic": "merchant_removal" },
              "left": "relations-pair-01",
              "right": "relations-pair-01-one-removed"
            }
          ]
        }"#,
    )
    .expect("write relations");

    let report = evaluate_classification(&dir, faithful_classification_answers());

    let relation = &report.relations[0];
    assert_eq!(relation.id, "removal-drops-one-merchant");
    assert!(
        relation.held(),
        "one merchant gone, one classification gone, the rest untouched: {relation:?}"
    );
}

/// A removal law between two fixtures asserting the same merchants is
/// wrongly declared, and must fail rather than hold vacuously — the
/// declared change did not happen.
#[test]
fn a_declared_removal_that_removes_nothing_is_failed() {
    let dir = classification_fixture_dir("removal-nothing");
    std::fs::write(
        dir.join("relations.json"),
        r#"{
          "relations": [
            {
              "id": "wrongly-declared-removal",
              "kind": { "algebraic": "merchant_removal" },
              "left": "relations-pair-01",
              "right": "relations-pair-01-reordered"
            }
          ]
        }"#,
    )
    .expect("write relations");

    let report = evaluate_classification(&dir, faithful_classification_answers());

    let relation = &report.relations[0];
    assert!(
        !relation.held(),
        "no merchant was removed, so the declared removal cannot hold: {relation:?}"
    );
}

/// A review-routed classification was surfaced to a person, not
/// asserted — so the relation is unjudgeable, exactly as a
/// review-routed extraction already makes it. Without this, the honest
/// floor's all-review run would read as a failed invariance.
#[test]
fn a_review_routed_classification_leaves_the_relation_unjudgeable() {
    let dir = classification_fixture_dir("review-routed");
    std::fs::write(
        dir.join("relations.json"),
        r#"{
          "relations": [
            {
              "id": "reorder-holds",
              "kind": { "invariance": { "projection": "classifications_by_merchant" } },
              "left": "relations-pair-01",
              "right": "relations-pair-01-reordered"
            }
          ]
        }"#,
    )
    .expect("write relations");

    // The twin's answers come back at low confidence, so its merchants
    // land in "check these yourself" rather than being asserted.
    let mut answers = merchant_answers(&[NETFLIX, SPOTIFY], "high");
    answers.extend(merchant_answers(&[SPOTIFY, NETFLIX], "low"));
    answers.extend(merchant_answers(&[NETFLIX], "high"));
    let report = evaluate_classification(&dir, answers);

    let relation = &report.relations[0];
    assert!(
        matches!(
            relation.outcome,
            runner::eval::relations::RelationOutcome::Unjudgeable { .. }
        ),
        "a surfaced decision asserts nothing, so nothing can be judged: {relation:?}"
    );
}

// ---------------------------------------------------------------------
// Controlled-change relations (#465, the third of #427's three kinds).
// One authored edit separates the two fixtures, and the judged outputs
// must differ in exactly the declared projection of that edit —
// everything else invariant. Invariance cannot say it (the outputs are
// meant to differ) and the inversion law cannot either (nothing swaps
// roles); this is the relation that catches a model which under-reacts
// to a real one-day deadline change, a failure no per-item gate and no
// recorded-answer mutation can see, because no recording contains the
// changed document.

use runner::eval::fixture::score_fixture;
use runner::eval::relations::{self, RelationOutcome, RelationsFile};
use runner::eval::Perf;
use runner::run::{ExtractionOutcome, InputSeen, Obligation, Payload, RunOutcome};

fn perf() -> Perf {
    Perf {
        wall_ms: 0,
        model_ms: 0,
        tokens_per_second: 0.0,
        peak_rss_mb: 0,
    }
}

/// One letter fixture's expectations: a single payment obligation with
/// the given deadline wording, anchor and resolved date.
fn deadline_expected(
    fixture_id: &str,
    deadline: &str,
    anchor: &str,
    due: Option<&str>,
) -> runner::eval::fixture::Expected {
    let due = due
        .map(|d| format!("\"{d}\""))
        .unwrap_or_else(|| "null".to_owned());
    serde_json::from_str(&format!(
        r#"{{
            "fixture_id": "{fixture_id}",
            "obligations": [
                {{
                    "id": "{fixture_id}-payment-01",
                    "strata": [],
                    "segment": "Please pay £120.00 to Harborne Parking Services {deadline} of {anchor}.",
                    "expect": {{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "deadline": "{deadline}",
                        "anchor": "{anchor}",
                        "due": {due}
                    }}
                }}
            ]
        }}"#
    ))
    .expect("expectations parse")
}

/// A run that read the letter as asserting the given obligation.
fn deadline_outcome(deadline: &str, anchor: &str, due: Option<&str>) -> RunOutcome {
    RunOutcome {
        input: InputSeen {
            rows: 0,
            period: None,
        },
        inputs: Vec::new(),
        needs_review: Vec::new(),
        warnings: Vec::new(),
        claim_traces: Vec::new(),
        payload: Payload::Extraction(ExtractionOutcome {
            date_disputes: vec![],
            obligations: vec![Obligation {
                kind: "payment".to_owned(),
                party: "Harborne Parking Services".to_owned(),
                ask: "Pay £120.00".to_owned(),
                deadline: deadline.to_owned(),
                anchor: anchor.to_owned(),
                confidence: "high".to_owned(),
                due: due.map(|date| runner::timeline::Resolved {
                    date: date.parse().expect("a date"),
                    kind: runner::claim::Kind::WorkedOut,
                }),
                evidence: Vec::new(),
                dated_by: None,
                disputed: vec![],
            }],
        }),
    }
}

/// The one-day pair's declaration: what changed, and exactly what is
/// allowed to move — the projection entries only each side may assert.
fn one_day_declaration() -> Vec<relations::RelationDeclaration> {
    let file: RelationsFile = serde_json::from_str(
        r#"{
          "relations": [
            {
              "id": "deadline-moves-one-day",
              "kind": {
                "controlled_change": {
                  "projection": "obligations_set",
                  "edit": "the payment deadline moves one day later",
                  "only_left": {
                    "": ["payment/Harborne Parking Services/within 14 days/2026-03-17"]
                  },
                  "only_right": {
                    "": ["payment/Harborne Parking Services/within 15 days/2026-03-18"]
                  }
                }
              },
              "left": "controlled-deadline-left-01",
              "right": "controlled-deadline-right-01"
            }
          ]
        }"#,
    )
    .expect("a controlled-change declaration parses");
    file.relations
}

/// The issue's named first test (#465, the M7 milestone's). Two letter
/// fixtures identical but for one deadline digit; a faithful reading of
/// each moves exactly the declared deadline and nothing else, and the
/// declared controlled change holds.
#[test]
fn a_one_day_deadline_change_moves_only_the_declared_deadline() {
    let left = score_fixture(
        "controlled-deadline-left-01.txt",
        &deadline_expected(
            "controlled-deadline-left-01",
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        &deadline_outcome(
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        perf(),
    );
    let right = score_fixture(
        "controlled-deadline-right-01.txt",
        &deadline_expected(
            "controlled-deadline-right-01",
            "within 15 days",
            "the date of this letter",
            Some("2026-03-18"),
        ),
        &deadline_outcome(
            "within 15 days",
            "the date of this letter",
            Some("2026-03-18"),
        ),
        perf(),
    );

    let judged =
        relations::judge(&one_day_declaration(), &[left, right]).expect("the relation is judged");

    assert_eq!(judged.len(), 1);
    assert!(
        judged[0].held(),
        "a faithful reading moves exactly the declared deadline: {:?}",
        judged[0]
    );
}

/// The failure this kind exists to catch: a model that answers the
/// unchanged reading on both sides. Every item can be individually
/// plausible, and the declared difference never appears — which is the
/// under-reaction no per-item gate can see.
#[test]
fn a_reading_that_does_not_move_with_the_edit_fails_the_controlled_change() {
    let left = score_fixture(
        "controlled-deadline-left-01.txt",
        &deadline_expected(
            "controlled-deadline-left-01",
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        &deadline_outcome(
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        perf(),
    );
    // The right document says 15 days; the model repeats 14.
    let right = score_fixture(
        "controlled-deadline-right-01.txt",
        &deadline_expected(
            "controlled-deadline-right-01",
            "within 15 days",
            "the date of this letter",
            Some("2026-03-18"),
        ),
        &deadline_outcome(
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        perf(),
    );

    let judged =
        relations::judge(&one_day_declaration(), &[left, right]).expect("the relation is judged");

    let RelationOutcome::Failed { smallest_diff } = &judged[0].outcome else {
        panic!("an unmoved reading must fail the declared change: {judged:?}");
    };
    // An unmoved reading asserts the same obligation on both sides, so
    // neither declared one-sided entry appears; the judge names the
    // first, which is the left's.
    assert!(
        smallest_diff.contains("the declared change never appeared")
            && smallest_diff.contains("within 14 days"),
        "the failure names the declared difference that never appeared: {smallest_diff}"
    );
}

/// The other direction: a reading that moves more than the edit did.
/// The declared deadline moves, and an unrelated obligation appears on
/// the right — an undeclared movement, named as such.
#[test]
fn a_reading_that_moves_more_than_the_edit_fails_the_controlled_change() {
    let left = score_fixture(
        "controlled-deadline-left-01.txt",
        &deadline_expected(
            "controlled-deadline-left-01",
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        &deadline_outcome(
            "within 14 days",
            "the date of this letter",
            Some("2026-03-17"),
        ),
        perf(),
    );
    let mut moved_too_much = deadline_outcome(
        "within 15 days",
        "the date of this letter",
        Some("2026-03-18"),
    );
    if let Payload::Extraction(extraction) = &mut moved_too_much.payload {
        extraction.obligations.push(Obligation {
            kind: "response".to_owned(),
            party: "Harborne Parking Services".to_owned(),
            ask: "Confirm in writing".to_owned(),
            deadline: "within 28 days".to_owned(),
            anchor: "the date of this letter".to_owned(),
            confidence: "high".to_owned(),
            due: None,
            evidence: Vec::new(),
            dated_by: None,
            disputed: vec![],
        });
    }
    let right = score_fixture(
        "controlled-deadline-right-01.txt",
        &deadline_expected(
            "controlled-deadline-right-01",
            "within 15 days",
            "the date of this letter",
            Some("2026-03-18"),
        ),
        &moved_too_much,
        perf(),
    );

    let judged =
        relations::judge(&one_day_declaration(), &[left, right]).expect("the relation is judged");

    let RelationOutcome::Failed { smallest_diff } = &judged[0].outcome else {
        panic!("movement beyond the declared edit must fail: {judged:?}");
    };
    assert!(
        smallest_diff.contains("response"),
        "the failure names the undeclared movement: {smallest_diff}"
    );
}

/// The #292 case #427 absorbed: a dateless anchor that is semantically
/// false scores identically to a correct one, because scoring compares
/// anchors by the date they name and these name none. The
/// `obligations_with_anchors` projection puts the wording back the day
/// it becomes visible — inside a contrastive pair, where the *document*
/// changed its anchor and the reading must change with it.
fn anchor_declaration() -> Vec<relations::RelationDeclaration> {
    let file: RelationsFile = serde_json::from_str(
        r#"{
          "relations": [
            {
              "id": "anchor-substitution-moves-the-anchor",
              "kind": {
                "controlled_change": {
                  "projection": "obligations_with_anchors",
                  "edit": "the stated anchor changes from the letter's own date to an invoice nothing dates",
                  "only_left": {
                    "": ["payment/Harborne Parking Services/within 14 days/the date of this letter/undated"]
                  },
                  "only_right": {
                    "": ["payment/Harborne Parking Services/within 14 days/the invoice date/undated"]
                  }
                }
              },
              "left": "controlled-anchor-left-01",
              "right": "controlled-anchor-right-01"
            }
          ]
        }"#,
    )
    .expect("an anchor declaration parses");
    file.relations
}

#[test]
fn a_substituted_dateless_anchor_must_move_the_anchor_projection() {
    let expected_left = deadline_expected(
        "controlled-anchor-left-01",
        "within 14 days",
        "the date of this letter",
        None,
    );
    let expected_right = deadline_expected(
        "controlled-anchor-right-01",
        "within 14 days",
        "the invoice date",
        None,
    );

    // A faithful reading reports each document's own anchor: held.
    let left = score_fixture(
        "controlled-anchor-left-01.txt",
        &expected_left,
        &deadline_outcome("within 14 days", "the date of this letter", None),
        perf(),
    );
    let right = score_fixture(
        "controlled-anchor-right-01.txt",
        &expected_right,
        &deadline_outcome("within 14 days", "the invoice date", None),
        perf(),
    );
    let judged =
        relations::judge(&anchor_declaration(), &[left, right]).expect("the relation is judged");
    assert!(
        judged[0].held(),
        "a faithful reading moves exactly the anchor: {:?}",
        judged[0]
    );

    // The invisible case: the model answers "the date of this letter"
    // on both sides. Both anchors are dateless, so per-item scoring
    // cannot tell them apart (#292) — the relation can, and must fail.
    let left = score_fixture(
        "controlled-anchor-left-01.txt",
        &expected_left,
        &deadline_outcome("within 14 days", "the date of this letter", None),
        perf(),
    );
    let right = score_fixture(
        "controlled-anchor-right-01.txt",
        &expected_right,
        &deadline_outcome("within 14 days", "the date of this letter", None),
        perf(),
    );
    let judged =
        relations::judge(&anchor_declaration(), &[left, right]).expect("the relation is judged");
    let RelationOutcome::Failed { smallest_diff } = &judged[0].outcome else {
        panic!(
            "an unmoved anchor must fail the declared change: {:?}",
            judged[0]
        );
    };
    // The same anchor came back on both sides, so neither declared
    // one-sided entry appeared; the failure carries the anchor wording
    // per-item scoring cannot see.
    assert!(
        smallest_diff.contains("the declared change never appeared")
            && smallest_diff.contains("the date of this letter"),
        "the failure names the anchor that never moved: {smallest_diff}"
    );
}

/// The acceptance criterion: every current pack ships at least three
/// relations, mixing meaning-preserving and meaning-changing kinds —
/// a bed that can only score fixtures one at a time cannot say the
/// system is stable under irrelevant change.
#[test]
fn each_current_pack_ships_at_least_three_relations() {
    // Every shipped pack, found rather than named: a hardcoded list is
    // how the subscription pack dodged this criterion for two slices,
    // and how a fourth pack would dodge it again.
    let packs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
    let mut packs: Vec<PathBuf> = std::fs::read_dir(&packs_dir)
        .expect("packs directory")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            path.join("pack.json").is_file().then_some(path)
        })
        .collect();
    packs.sort();
    assert!(!packs.is_empty(), "no packs found to hold to the criterion");
    for pack in packs {
        let name = pack
            .file_name()
            .and_then(|n| n.to_str())
            .expect("pack name");
        let path = pack.join("fixtures/relations.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{name} ships no relations.json"));
        let file: serde_json::Value = serde_json::from_str(&text).expect("valid relations");
        let relations = file["relations"].as_array().expect("a relations array");
        assert!(
            relations.len() >= 3,
            "{name} ships {} relations; the bed needs at least three",
            relations.len()
        );
    }
}

/// An audition run judges only the relations it carries both halves of
/// (#539, #432).
///
/// The audition set is a *declared* subset — six to ten fixtures chosen
/// for what a go/no-go verdict needs — so it will routinely carry one
/// half of a twin pair and not the other. That is the set working as
/// designed, and #539 says so in as many words: relations never print
/// on a partial bed.
///
/// It must not be confused with the case
/// `a_relation_across_the_set_boundary_is_refused` pins, where a
/// relation reaches from development into exam. That one is a bed
/// error and stays a refusal. The difference is whether the fixture is
/// absent *by declaration* or absent *by mistake*, and only the
/// selection knows which.
///
/// Found the hard way on 20 August: `--audition` refused on all three
/// shipped packs, after running every fixture through the model and
/// discarding the results at the final step.
#[test]
fn an_audition_carrying_one_half_of_a_twin_judges_no_relation() {
    let dir = swap_twin_pack("audition-half");
    // Declare an audition naming one half of the twin pair, which is
    // what a real audition subset looks like.
    let manifest = std::fs::read_to_string(dir.join("pack.json"))
        .expect("manifest reads")
        .replace(
            r#""outputs": ["report.html"]"#,
            r#""eval_items": { "audition": ["forward"] },
               "outputs": ["report.html"]"#,
        );
    std::fs::write(dir.join("pack.json"), manifest).expect("write manifest");

    let pack = load_pack(&dir).expect("the pack loads");
    let mock = MockModel::respond_sequence(vec![(
        "200 OK",
        faithful_answer(OLD_SCHEDULE, NEW_SCHEDULE),
    )]);
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

    let report = evaluator
        .evaluate_audition(&pack)
        .expect("an audition that carries one half of a twin still runs");

    assert!(
        report.relations.is_empty(),
        "a partial bed judges no relation rather than refusing: {:?}",
        report.relations
    );
}
