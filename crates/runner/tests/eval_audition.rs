//! The audition set (#539): a committed, citable go/no-go bed for
//! candidate models.
//!
//! Membership is declared in the pack manifest (`eval_items.audition`),
//! never in a fixture's own `expected.json`: those bytes feed the
//! recorded bed digests (#320) and every resume key, so tagging a
//! development fixture in-file would change the development digest and
//! read as "the bed changed" when no question or expectation moved.
//! The manifest already carries eval membership this way (`retired`).
//!
//! Audition draws on development only, never exam: the holdout's job is
//! to be unseen, and a set candidate models run against during triage
//! is the opposite of unseen.

mod support;

use runner::eval::fixture::{fixtures_at_for_eval, model_info, EvalSelection, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::fs;
use std::path::Path;
use support::{completion_envelope, MockModel};

fn write_fixture(dir: &std::path::Path, name: &str, expected: &str) {
    fs::write(dir.join(format!("{name}.txt")), "A statement line.\n").expect("statement written");
    fs::write(dir.join(format!("{name}.expected.json")), expected).expect("expectations written");
}

/// Selecting the audition set returns exactly the fixtures the manifest
/// names: an unlisted development fixture is not auditioning.
#[test]
fn audition_selection_is_exactly_the_declared_fixtures() {
    let dir =
        std::env::temp_dir().join(format!("kettle-audition-{}-selection", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("fixture dir");

    write_fixture(
        &dir,
        "aud-01",
        r#"{"normalise": [{"raw": "J HENDERSON WINDOWS", "name": "J Henderson Windows"}]}"#,
    );
    write_fixture(
        &dir,
        "dev-01",
        r#"{"normalise": [{"raw": "APRICOT MUSIC LTD", "name": "Apricot Music"}]}"#,
    );

    let fixtures = fixtures_at_for_eval(
        &dir,
        &[],
        &["aud-01.txt".to_owned()],
        EvalSelection::Audition,
        &[],
    )
    .expect("fixtures readable");

    let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["aud-01.txt"],
        "audition selects the declared fixture and nothing else"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// An audition list naming an exam fixture is a contradiction the
/// selection must refuse, not quietly filter: the person declared the
/// holdout into the one set that exists to be spent freely.
#[test]
fn an_audition_list_naming_an_exam_fixture_is_refused() {
    let dir = std::env::temp_dir().join(format!("kettle-audition-{}-exam", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("fixture dir");

    write_fixture(
        &dir,
        "exam-01",
        r#"{"eval_set": "exam", "normalise": [{"raw": "ORCHARD MUSIC LTD", "name": "Orchard Music"}]}"#,
    );

    let err = fixtures_at_for_eval(
        &dir,
        &[],
        &["exam-01.txt".to_owned()],
        EvalSelection::Audition,
        &[],
    )
    .expect_err("an exam fixture cannot audition");

    assert!(
        err.contains("audition") && err.contains("exam"),
        "the refusal names the contradiction: {err}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A name with no fixture behind it is refused, not skipped: a silently
/// shrinking audition would still print a digest and read as the
/// committed instrument.
#[test]
fn an_audition_list_naming_a_missing_fixture_is_refused() {
    let dir = std::env::temp_dir().join(format!("kettle-audition-{}-missing", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("fixture dir");

    write_fixture(
        &dir,
        "aud-01",
        r#"{"normalise": [{"raw": "J HENDERSON WINDOWS", "name": "J Henderson Windows"}]}"#,
    );

    let err = fixtures_at_for_eval(
        &dir,
        &[],
        &["aud-01.txt".to_owned(), "aud-02.txt".to_owned()],
        EvalSelection::Audition,
        &[],
    )
    .expect_err("a named fixture that is not there must be said out loud");

    assert!(
        err.contains("aud-02.txt"),
        "the refusal names the missing fixture: {err}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A pack with no declared audition cannot audition a model: zero
/// fixtures scoring zero questions would read as a pass, and a test
/// that needs fixtures must fail without them, never skip quietly into
/// green (PR #99's lesson, applied to the set that gatekeeps full
/// runs).
#[test]
fn an_empty_audition_selection_is_refused_not_a_vacuous_pass() {
    let dir = std::env::temp_dir().join(format!("kettle-audition-{}-empty", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("fixture dir");

    write_fixture(
        &dir,
        "dev-01",
        r#"{"normalise": [{"raw": "APRICOT MUSIC LTD", "name": "Apricot Music"}]}"#,
    );

    let err = fixtures_at_for_eval(&dir, &[], &[], EvalSelection::Audition, &[])
        .expect_err("no declared fixtures means no audition, said out loud");

    assert!(
        err.contains("audition"),
        "the refusal says which selection was empty: {err}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Every shipped pack declares an audition set, and it resolves: the
/// go/no-go instrument exists per pack, stays small enough to run in
/// minutes, and every declared name is a real development fixture. The
/// resolve call is the same one an audition run makes, so a name that
/// rots fails here before it fails a candidate measurement.
#[test]
fn every_shipped_pack_declares_a_resolving_audition_set() {
    for pack_name in [
        "app.kttl.subscription-audit",
        "app.kttl.letter-to-actions",
        "app.kttl.renewal-diff",
    ] {
        let pack_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs")
            .join(pack_name);
        let pack = load_pack(&pack_dir).expect("pack loads");

        let declared = &pack.manifest.eval_items.audition;
        assert!(
            !declared.is_empty(),
            "{pack_name} declares no audition set (eval_items.audition)"
        );
        assert!(
            (4..=12).contains(&declared.len()),
            "{pack_name} declares {} audition fixtures; the audition is minutes, not hours",
            declared.len()
        );

        let fixtures = fixtures_at_for_eval(
            &pack_dir.join("fixtures"),
            &pack.manifest.eval_items.retired,
            declared,
            EvalSelection::Audition,
            &pack.manifest.inputs,
        )
        .unwrap_or_else(|err| panic!("{pack_name}'s audition set does not resolve: {err}"));
        assert_eq!(fixtures.len(), declared.len(), "{pack_name}");
    }
}

/// An audition report says it is one. A report that borrowed
/// "development" would enter a verdict comment claiming a set it never
/// ran, and a recording must describe itself (#303).
///
/// The mock carries answers for statement-01 alone, so the selection
/// holding through the whole evaluator is load-bearing: if the unlisted
/// statement-02 leaked in, the second fixture would drain the sequence
/// and fail loudly.
#[test]
fn an_audition_report_names_its_set_honestly() {
    let pack_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit");
    let dir = std::env::temp_dir().join(format!("kettle-audition-{}-report", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("fixture dir");

    for file in [
        "statement-01.csv",
        "statement-01.expected.json",
        "statement-02-messy.csv",
        "statement-02-messy.expected.json",
    ] {
        fs::copy(pack_dir.join("fixtures").join(file), dir.join(file)).expect("copy fixture");
    }

    let mut pack = load_pack(&pack_dir).expect("pack loads");
    // The declaration the shipped manifest will carry once the subsets
    // are authored; injected here so the test does not wait for that
    // authoring decision.
    pack.manifest.eval_items.audition = vec!["statement-01.csv".to_owned()];

    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "raw": "PUREGYM LTD", "name": "PureGym", "recognised": true},
                {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "recognised": true},
                {"id": 2, "raw": "SPOTIFY LTD", "name": "Spotify", "recognised": true},
                {"id": 3, "raw": "BRITISH GAS", "name": "British Gas", "recognised": true},
                {"id": 4, "raw": "TESCO STORES 3412", "name": "Tesco", "recognised": true},
                {"id": 5, "raw": "ACME PAYROLL", "name": "Acme Payroll", "recognised": true}
            ]}"#,
            ),
        ),
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "name": "PureGym", "kind": "subscription", "category": "fitness", "confidence": "high"},
                {"id": 1, "name": "Netflix", "kind": "subscription", "category": "streaming", "confidence": "high"},
                {"id": 2, "name": "Spotify", "kind": "subscription", "category": "streaming", "confidence": "high"},
                {"id": 3, "name": "British Gas", "kind": "utility", "category": "energy", "confidence": "high"},
                {"id": 4, "name": "Tesco", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
                {"id": 5, "name": "Acme Payroll", "kind": "regular_spend", "category": "other", "confidence": "high"}
            ]}"#,
            ),
        ),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: MachineInfo {
            cpu: "Apple M1 Pro".to_owned(),
            ram_gb: 16,
            os: "macOS 15.5".to_owned(),
        },
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(dir.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator
        .evaluate_audition(&pack)
        .expect("the audition runs");

    assert_eq!(
        report.eval_set.as_str(),
        "audition",
        "the report names the set that actually ran"
    );

    let _ = fs::remove_dir_all(&dir);
}
