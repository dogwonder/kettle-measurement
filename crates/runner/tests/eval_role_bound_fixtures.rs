//! #354: a fixture is a set of role-bound documents, not one file.
//!
//! The runner compares two documents (#350) and the lens scores a
//! payload that is not an obligation (#351) — and the harness still ran
//! every fixture through `run_pack`, which binds a flat list to a pack's
//! *sole* role and refuses anything else. A two-role pack could not be
//! scored at all, which under #348 is the same as a pack shipped on
//! assertion.
//!
//! The single-document path must not move a millimetre: names appear in
//! baselines, and digests are what every resume key and recorded bed
//! digest is built from (#320).

use runner::eval::fixture::{fixtures_at, fixtures_in, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
mod support;
use support::{completion_envelope, MockModel};

fn payment_answer() -> String {
    completion_envelope(&serde_json::json!({"results":[
        {"id":0,"segment":"10 March 2026","confidence":"high","obligations":[]},
        {"id":1,"segment":"Example Services","confidence":"high","obligations":[]},
        {"id":2,"segment":"Please pay £120.00 within 14 days of the date of this letter.","confidence":"high","obligations":[{
            "kind":"payment","party":{"at":1,"value":"Example Services"},"ask":"Pay £120.00",
            "amount":{"at":2,"value":"£120.00"},
            "deadline":{"at":2,"value":"within 14 days of the date of this letter",
                "read":{"count":14,"unit":"days","qualifier":"none","counts_from":"letter_date"},
                "from":{"at":0,"value":"10 March 2026"}}
        }]}
    ]}).to_string())
}

#[test]
fn one_file_and_two_ordered_pages_preserve_the_ask_with_its_actual_evidence_page() {
    let (dir, pack) = page_fixture("evidence-page");
    let source = ["z-front.txt", "a-back.txt"]
        .map(|file| std::fs::read_to_string(dir.join(file)).unwrap())
        .join("\n\n");
    std::fs::write(dir.join("whole.txt"), source).unwrap();
    let mut readings = Vec::new();
    for (input, page) in [
        (serde_json::json!(["z-front.txt", "a-back.txt"]), 2),
        (serde_json::json!("whole.txt"), 1),
    ] {
        std::fs::write(
            dir.join("letter.expected.json"),
            serde_json::json!({"inputs":{"letter":input}}).to_string(),
        )
        .unwrap();
        let fixture =
            runner::eval::fixture::fixtures_at_with_roles(&dir, &[], &pack.manifest.inputs)
                .unwrap()
                .remove(0);
        let bound: Vec<_> = fixture
            .inputs
            .iter()
            .map(|(role, path)| (role.as_str(), path.clone()))
            .collect();
        let mock = MockModel::respond_once("200 OK", payment_answer());
        let outcome = runner::run::run_pack_bound(
            &pack,
            &bound,
            &Answers::FromModel(mock.endpoint()),
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_| {},
            &runner::run_dir::NoLog,
        )
        .unwrap();
        let runner::run::Payload::Extraction(extraction) = outcome.payload else {
            panic!("letter extraction")
        };
        assert_eq!(extraction.obligations.len(), 1);
        let ask = &extraction.obligations[0];
        assert_eq!(ask.evidence[0].document, 0);
        assert_eq!(ask.evidence[0].page, page);
        assert_eq!(ask.from.at, 0);
        assert_eq!(ask.due.unwrap().date.to_string(), "2026-03-24");
        readings.push((ask.kind.clone(), ask.amount.value.clone(), ask.due));
    }
    assert_eq!(readings[0], readings[1]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn grouped_fixture_results_resume_only_for_identical_pages_order_and_policy() {
    let (dir, mut pack) = page_fixture("resume");
    let evaluator = FixtureEvaluator {
        resume_dir: Some(dir.join("cache")),
        ..page_evaluator(&dir)
    };
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 0);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 1);
    std::fs::write(
        dir.join("letter.expected.json"),
        r#"{"inputs":{"letter":["a-back.txt","z-front.txt"]}}"#,
    )
    .unwrap();
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 0);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 1);
    std::fs::write(dir.join("a-back.txt"), "Please pay £140.00 within 14 days.").unwrap();
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 0);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 1);
    pack.manifest.inputs[0].max_pages = Some(2);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 0);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 1);
    pack.manifest.inputs[0].file_semantics = runner::packs::FileSemantics::Documents;
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 0);
    assert_eq!(evaluator.evaluate(&pack).unwrap().reused_fixtures, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_list_for_a_documents_role_does_not_turn_independent_letters_into_pages() {
    let (dir, mut pack) = page_fixture("separate-documents");
    pack.manifest.inputs[0].file_semantics = runner::packs::FileSemantics::Documents;
    let fixture = runner::eval::fixture::fixtures_at_with_roles(&dir, &[], &pack.manifest.inputs)
        .unwrap()
        .remove(0);
    let bound: Vec<_> = fixture
        .inputs
        .iter()
        .map(|(role, path)| (role.as_str(), path.clone()))
        .collect();
    let mock = MockModel::respond_sequence(support::per_batch(&payment_answer(), &[0, 2]));
    let outcome = runner::run::run_pack_bound(
        &pack,
        &bound,
        &Answers::FromModel(mock.endpoint()),
        &std::sync::atomic::AtomicBool::new(false),
        &mut |_| {},
        &runner::run_dir::NoLog,
    )
    .unwrap();
    let runner::run::Payload::Extraction(extraction) = outcome.payload else {
        panic!("letter extraction")
    };
    let ask = &extraction.obligations[0];
    assert_eq!(ask.evidence[0].document, 1);
    assert_eq!(ask.evidence[0].page, 1);
    assert!(
        ask.due.is_none(),
        "the second document cannot borrow the first one's dateline"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn page_fixture(name: &str) -> (PathBuf, runner::packs::Pack) {
    let dir =
        std::env::temp_dir().join(format!("kettle-page-fixture-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Filename order deliberately disagrees with authored page order.
    std::fs::write(dir.join("z-front.txt"), "10 March 2026\n\nExample Services").unwrap();
    std::fs::write(
        dir.join("a-back.txt"),
        "Please pay £120.00 within 14 days of the date of this letter.",
    )
    .unwrap();
    std::fs::write(
        dir.join("letter.expected.json"),
        serde_json::json!({
            "fixture_id": "ordered-letter-pages",
            "inputs": {"letter": ["z-front.txt", "a-back.txt"]}
        })
        .to_string(),
    )
    .unwrap();
    let pack = load_pack(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.letter-to-actions"),
    )
    .unwrap();
    (dir, pack)
}

fn page_evaluator(dir: &Path) -> FixtureEvaluator {
    FixtureEvaluator {
        fixtures_dir: Some(dir.to_path_buf()),
        ..floor_evaluator()
    }
}

#[test]
fn a_fixture_binds_ordered_pages_to_one_role_and_runs() {
    let (dir, pack) = page_fixture("runs");
    let fixtures =
        runner::eval::fixture::fixtures_at_with_roles(&dir, &[], &pack.manifest.inputs).unwrap();
    assert_eq!(fixtures.len(), 1);
    assert_eq!(
        fixtures[0].inputs,
        vec![
            ("letter".into(), dir.join("z-front.txt")),
            ("letter".into(), dir.join("a-back.txt")),
        ]
    );
    let report = page_evaluator(&dir).evaluate(&pack).unwrap();
    assert_eq!(report.fixtures.len(), 1);
    assert!(report.unrunnable.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_shipped_letter_accepts_three_files_and_refuses_four_without_a_model_request() {
    let (dir, pack) = page_fixture("file-limit");
    std::fs::write(dir.join("closing.txt"), "Yours faithfully.").unwrap();
    std::fs::write(
        dir.join("letter.expected.json"),
        serde_json::json!({"inputs":{
            "letter":["z-front.txt", "a-back.txt", "closing.txt"]
        }})
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        page_evaluator(&dir).evaluate(&pack).unwrap().fixtures.len(),
        1
    );
    std::fs::write(
        dir.join("letter.expected.json"),
        serde_json::json!({"inputs":{
            "letter":["z-front.txt", "a-back.txt", "closing.txt", "closing.txt"]
        }})
        .to_string(),
    )
    .unwrap();
    let mock = MockModel::respond_once("200 OK", payment_answer());
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        ..page_evaluator(&dir)
    };
    let error = evaluator.evaluate(&pack).unwrap_err();
    assert!(
        error.contains("between one and three files") && error.contains("got 4"),
        "{error}"
    );
    mock.assert_no_request();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_page_and_its_order_contribute_to_the_loaded_fixture_identity() {
    let (dir, pack) = page_fixture("identity");
    let load = || {
        runner::eval::fixture::fixtures_at_with_roles(&dir, &[], &pack.manifest.inputs)
            .unwrap()
            .remove(0)
    };
    let original = load();
    let first = runner::eval::fixture::digest_of(&original);
    // Same expectation bytes: the binding order alone must matter.
    let mut reversed = original.clone();
    reversed.inputs.reverse();
    assert_ne!(first, runner::eval::fixture::digest_of(&reversed));
    std::fs::write(dir.join("a-back.txt"), "Please pay £140.00 within 14 days.").unwrap();
    assert_ne!(first, runner::eval::fixture::digest_of(&load()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_group_refuses_empty_missing_excess_and_wrong_type_inputs_before_evaluation() {
    for (tag, input, expected) in [
        ("empty", serde_json::json!([]), "letter"),
        (
            "missing",
            serde_json::json!(["z-front.txt", "lost.txt"]),
            "lost.txt",
        ),
        (
            "excess",
            serde_json::json!(["z-front.txt", "a-back.txt", "z-front.txt", "a-back.txt"]),
            "letter",
        ),
        (
            "directory",
            serde_json::json!(["z-front.txt", "folder"]),
            "folder",
        ),
        (
            "wrong-type",
            serde_json::json!(["z-front.txt", "data.csv"]),
            "data.csv",
        ),
    ] {
        let (dir, pack) = page_fixture(tag);
        std::fs::create_dir_all(dir.join("folder")).unwrap();
        std::fs::write(dir.join("data.csv"), "Date,Amount\n").unwrap();
        std::fs::write(
            dir.join("letter.expected.json"),
            serde_json::json!({"inputs":{"letter":input}}).to_string(),
        )
        .unwrap();
        let error = runner::eval::fixture::fixtures_at_with_roles(&dir, &[], &pack.manifest.inputs)
            .unwrap_err();
        assert!(error.contains(expected), "{tag}: {error}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_named_fixture_must_supply_every_declared_role() {
    let dir = comparison_pack("missing-role");
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        r#"{"inputs":{"previous":"renewal-01-previous.txt"}}"#,
    )
    .unwrap();
    let pack = load_pack(&dir).unwrap();
    let error = fixtures_in(&pack).unwrap_err();
    assert!(error.contains("renewal"), "{error}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// A comparison pack (#350's shape) with a two-document fixture whose
/// `expected.json` names its own inputs by role.
fn comparison_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-roles-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-roles",
          "name": "Role-bound fixture test",
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
          "value_kinds": { "compulsory_excess": "money", "premium": "money" },
          "outputs": ["report.html"],
          "eval_costs": {
            "review_rate": {
              "reason": "How many passages a person reads themselves. Surfacing uncertainty is a cost, never a wrong answer.",
              "date": "2026-08-03"
            }
          }
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
                    "term": { "enum": ["compulsory_excess", "premium", "other"] },
                    "basis": { "enum": ["per_claim", "annual", "other"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");

    // Two wholly invented policy schedules (CLAUDE.md).
    write(
        "fixtures/renewal-01-previous.txt",
        "Your policy schedule for the year to 31 August 2026.\n\n\
         Compulsory excess: £250 per claim.",
    );
    write(
        "fixtures/renewal-01-renewal.txt",
        "Your renewal schedule for the year to 31 August 2027.\n\n\
         Compulsory excess: £500 per claim.",
    );
    write(
        "fixtures/renewal-01.expected.json",
        r#"{
          "fixture_id": "renewal-01",
          "eval_set": "development",
          "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
          }
        }"#,
    );
    dir
}

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

fn floor_evaluator() -> FixtureEvaluator {
    FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    }
}

/// The test #354 names. A fixture whose expectations name two roles is
/// discovered with both, and the evaluator runs it rather than refusing
/// it for not saying which document is which.
#[test]
fn a_fixture_may_be_several_documents_bound_to_roles() {
    let dir = comparison_pack("two-roles");
    let pack = load_pack(&dir).expect("the comparison pack loads");

    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    assert_eq!(fixtures.len(), 1, "{fixtures:#?}");
    let fixture = &fixtures[0];
    assert_eq!(
        fixture.inputs,
        vec![
            (
                "previous".to_owned(),
                dir.join("fixtures/renewal-01-previous.txt")
            ),
            (
                "renewal".to_owned(),
                dir.join("fixtures/renewal-01-renewal.txt")
            ),
        ],
        "bound by name, in the order the manifest declares them"
    );

    // And it runs. Before #354 this failed with RoleUnstated — the
    // harness could not put the pack's own fixture into the pack.
    let report = floor_evaluator()
        .evaluate(&pack)
        .expect("a two-document fixture runs");
    assert_eq!(report.fixtures.len(), 1, "{report:#?}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Discovery of every fixture that exists today must not move: `name`
/// is what a baseline records and what a resume key is built from, so a
/// changed name silently retires a recorded measurement.
#[test]
fn single_document_fixtures_are_discovered_exactly_as_before() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");

    let fixtures = fixtures_in(&pack).expect("fixtures readable");

    // 165 → 167 with #575, which gave `statement-04` a PDF route beside
    // its CSV. The number moves only in a commit that says why, which
    // is the whole point of pinning it; this one did not, and the
    // failure sat unseen because CI had no minutes to report it.
    assert_eq!(fixtures.len(), 167);
    for legacy in [
        "statement-01.csv",
        "statement-02-messy.csv",
        "statement-06-broad.csv",
    ] {
        let found = fixtures
            .iter()
            .find(|fixture| fixture.name == legacy)
            .unwrap_or_else(|| panic!("{legacy} is still scorable"));
        // One document, bound to the pack's sole role, exactly as
        // `bind_to_sole_role` did it.
        assert_eq!(found.inputs.len(), 1, "{legacy}: {:?}", found.inputs);
        assert_eq!(found.inputs[0].0, "statement");
        assert_eq!(found.inputs[0].1, found.path);
    }
}

/// A digest covers every document a fixture is made of. Two fixtures
/// differing only in their second file would otherwise share a resume
/// key, and the cache would answer one set of questions with another
/// set's answers (#282, #320).
#[test]
fn a_digest_covers_every_document_in_the_fixture() {
    let dir = comparison_pack("digest");
    let pack = load_pack(&dir).expect("pack loads");
    let before = fixtures_in(&pack).expect("fixtures readable");
    let first = runner::eval::fixture::digest_of(&before[0]);

    // Change only the second document.
    std::fs::write(
        dir.join("fixtures/renewal-01-renewal.txt"),
        "Your renewal schedule for the year to 31 August 2027.\n\n\
         Compulsory excess: £750 per claim.",
    )
    .expect("rewrite the renewal");

    let after = fixtures_in(&pack).expect("fixtures readable");
    let second = runner::eval::fixture::digest_of(&after[0]);

    assert_ne!(
        first, second,
        "a fixture whose second document changed is a different question"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A single-document fixture's digest must be the bytes it has always
/// been. Every recorded bed digest (#320) and every resume key is built
/// from it, so a changed hash refuses baselines that are still valid.
#[test]
fn a_single_document_digest_does_not_move() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    let fixture = fixtures
        .iter()
        .find(|fixture| fixture.name == "statement-01.csv")
        .expect("the first statement");

    assert_eq!(
        runner::eval::fixture::digest_of(fixture),
        runner::eval::resume::fixture_digest(
            &fixture.path,
            &fixture.path.with_extension("expected.json")
        ),
        "one document hashes exactly as it did before fixtures had roles"
    );
}

/// A role the pack never declared is refused at discovery, before a
/// sidecar is spawned and before the first fixtures are spent. The run
/// would catch it (`check_bindings`), but by then the measurement has
/// already cost minutes of model time.
#[test]
fn a_fixture_naming_a_role_the_pack_does_not_declare_is_refused() {
    let dir = comparison_pack("undeclared-role");
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        r#"{
          "fixture_id": "renewal-01",
          "inputs": {
            "previous": "renewal-01-previous.txt",
            "quote": "renewal-01-renewal.txt"
          }
        }"#,
    )
    .expect("rewrite expectations");
    let pack = load_pack(&dir).expect("pack loads");

    let error = fixtures_in(&pack).expect_err("a role the pack has never heard of is refused");
    assert!(
        error.contains("quote"),
        "the refusal names the role: {error}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A named input that is not there is refused too, and says which file.
/// Discovering it mid-run would blame the pack for a bed's typo.
#[test]
fn a_fixture_naming_a_missing_document_is_refused() {
    let dir = comparison_pack("missing-document");
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        r#"{
          "fixture_id": "renewal-01",
          "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-last-year.txt"
          }
        }"#,
    )
    .expect("rewrite expectations");
    let pack = load_pack(&dir).expect("pack loads");

    let error = fixtures_in(&pack).expect_err("a document that is not there is refused");
    assert!(
        error.contains("renewal-01-last-year.txt"),
        "the refusal names the file: {error}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `--fixture-dir` points at somebody's own documents, outside any pack
/// (CLAUDE.md's data rules keep real statements out of the repo). It has
/// no manifest to bind against, so it keeps the sole-role behaviour and
/// must not start refusing what it used to read.
#[test]
fn a_fixture_directory_outside_a_pack_still_reads() {
    let pack_fixtures = Path::new("../../packs/app.kttl.subscription-audit/fixtures").to_path_buf();

    let fixtures = fixtures_at(&pack_fixtures).expect("a bare directory still reads");

    assert!(!fixtures.is_empty());
    assert!(
        fixtures.iter().all(|fixture| fixture.inputs.is_empty()),
        "with no manifest there is no role to name a document with"
    );
    assert!(
        fixtures
            .iter()
            .all(|fixture| fixture.documents() == vec![fixture.path.clone()]),
        "and each fixture is still exactly the one document it was"
    );
}

#[test]
fn photographed_letters_are_discovered_as_eval_fixtures() {
    let dir = std::env::temp_dir().join(format!("kettle-photo-fixtures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    std::fs::write(
        dir.join("invented-letter.jpg"),
        b"invented test image bytes",
    )
    .expect("photo fixture");
    std::fs::write(dir.join("invented-letter.expected.json"), "{}").expect("expectations");

    let fixtures = fixtures_at(&dir).expect("photo fixtures are discoverable");
    assert_eq!(fixtures.len(), 1);
    assert_eq!(fixtures[0].name, "invented-letter.jpg");

    let _ = std::fs::remove_dir_all(dir);
}
