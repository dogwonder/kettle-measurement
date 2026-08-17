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

    assert_eq!(fixtures.len(), 165);
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
