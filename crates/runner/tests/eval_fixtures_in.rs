//! Finding the fixtures a pack can be scored against (#25).

use runner::eval::fixture::{
    fixtures_at, fixtures_at_for_eval, fixtures_in, validate_declared_strata, EvalSelection,
    EvalSet, Expected,
};
use runner::eval::HarmClass;
use runner::packs::load_pack;
use std::path::Path;

/// Only statements with scoring expectations enter an eval. A statement
/// nobody wrote expectations for is still usable by hand through
/// `--fixture-dir`, but cannot produce an invented score.
#[test]
fn pairs_each_statement_with_the_expectations_beside_it() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");

    let fixtures = fixtures_in(&pack).expect("fixtures readable");

    let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
    // 165: the generated bed with its twins — reorder and one-removed
    // (#427), descriptor-command-text (#433) — plus the legacy trio.
    assert_eq!(names.len(), 165);
    for legacy in [
        "statement-01.csv",
        "statement-02-messy.csv",
        "statement-06-broad.csv",
    ] {
        assert!(names.contains(&legacy), "{legacy} is still scorable");
    }
    assert!(!names.contains(&"statement-03.pdf"));
}

/// #234: the original two fixtures each classify five merchants, so
/// one completely wrong merchant moves the fixture score by 10 points.
/// The broad statement makes one merchant at most 1.25 points while
/// keeping the expectation behind every mark reviewable.
#[test]
fn the_broad_fixture_is_not_decided_by_one_merchant() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    let broad = fixtures
        .iter()
        .find(|fixture| fixture.name == "statement-06-broad.csv")
        .expect("the broad fixture");

    assert_eq!(broad.expected.classify.len(), 80);
    assert_eq!(broad.expected.normalise.len(), 80);
    let transactions = std::fs::read_to_string(&broad.path)
        .expect("the broad statement")
        .lines()
        .skip(1)
        .count();
    assert_eq!(transactions, 80, "more transactions, not just more files");
    let one_merchant = 1.0 / broad.expected.classify.len() as f32;
    assert!(one_merchant <= 0.0125, "{one_merchant}");
}

#[test]
fn every_classification_item_uses_a_declared_stratum() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");

    for fixture in fixtures {
        for item in fixture.expected.classify {
            for stratum in item.strata {
                assert!(
                    pack.manifest.eval_strata.contains_key(&stratum),
                    "{} item {} uses undeclared stratum {stratum}",
                    fixture.name,
                    item.id
                );
            }
        }
    }
}

#[test]
fn an_undeclared_item_stratum_is_refused_before_a_model_runs() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    let mut declared = pack.manifest.eval_strata.clone();
    declared.remove("automatic-billing");

    let problem = validate_declared_strata(&fixtures, &declared)
        .expect_err("an undeclared tag must not become an invisible typo-shaped stratum");

    assert!(problem.contains("automatic-billing"), "{problem}");
    assert!(problem.contains("broad"), "{problem}");
}

/// The letter bed tags obligations, not classifications, so the check
/// above never sees them: a typo'd tag on an obligation would score
/// into a slice no ceiling reads (#430, found planning the adversarial
/// strata that make the letter bed's tags load-bearing).
#[test]
fn an_obligation_with_an_undeclared_stratum_is_refused_before_model_time() {
    let pack = load_pack(Path::new("../../packs/app.kttl.letter-to-actions")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    let mut declared = pack.manifest.eval_strata.clone();
    declared.remove("any-letter");

    let problem = validate_declared_strata(&fixtures, &declared)
        .expect_err("an undeclared obligation tag must not become an invisible stratum");

    assert!(problem.contains("any-letter"), "{problem}");
}

/// The expectations really are read, not just their file names noticed.
#[test]
fn reads_the_expectations_themselves() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");

    let fixtures = fixtures_in(&pack).expect("fixtures readable");
    let first = fixtures
        .iter()
        .find(|fixture| fixture.name == "statement-01.csv")
        .expect("the original clean fixture");

    assert!(first
        .expected
        .normalise
        .iter()
        .any(|e| e.raw == "NETFLIX.COM" && e.name == "Netflix"));
    assert_eq!(first.expected.tolerances["normalise"], "fuzzy:0.85");
}

/// A cadence Kettle doesn't know is a mistake in the fixture, and it is
/// said out loud. Scoring it silently would cost a perfect model its
/// pass and read as a model failure — the harness would be lying about
/// the one thing it exists to measure.
#[test]
fn expectations_with_an_unknown_cadence_are_refused() {
    let expected: Expected = serde_json::from_str(
        r#"{ "normalise": [], "classify": [],
             "recurring": [{ "merchant": "Amazon Prime", "period": "annual" }],
             "tolerances": {} }"#,
    )
    .expect("expected.json");

    let refusal = expected.validate().expect_err("'annual' is not a cadence");

    assert!(refusal.contains("annual"), "{refusal}");
    assert!(refusal.contains("yearly"), "{refusal}");
}

/// The spellings the rest of Kettle uses are accepted unchanged.
#[test]
fn the_cadences_kettle_writes_are_accepted() {
    let expected: Expected = serde_json::from_str(
        r#"{ "normalise": [], "classify": [],
             "recurring": [
                { "merchant": "Netflix", "period": "monthly" },
                { "merchant": "British Gas", "period": "quarterly" },
                { "merchant": "Amazon Prime", "period": "yearly" },
                { "merchant": "Window Cleaner", "period": "weekly" }
             ],
             "tolerances": {} }"#,
    )
    .expect("expected.json");

    assert!(expected.validate().is_ok());
}

/// `--fixture-dir` points the eval at real statements instead of the
/// pack's synthetic ones — they are gitignored and never come into the
/// repo (CLAUDE.md), so the directory has to be somewhere else entirely.
#[test]
fn fixtures_can_be_read_from_somewhere_other_than_the_pack() {
    let elsewhere = Path::new("../../packs/app.kttl.subscription-audit/fixtures");

    let fixtures = fixtures_at(elsewhere).expect("fixtures readable");

    let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names.len(), 165);
    assert!(names.contains(&"statement-01.csv"));
    assert!(names.contains(&"generated-exam-clean-no-subscriptions-glacier-path.csv"));
}

/// Prompt iteration sees development fixtures only. The sealed exam set
/// enters a run only when the caller explicitly says this is the
/// pack-version-bump measurement.
#[test]
fn exam_fixtures_are_excluded_unless_explicitly_requested() {
    let dir = std::env::temp_dir().join(format!(
        "kettle-eval-development-exam-split-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture directory");

    for (stem, fixture_id, item_id, eval_set) in [
        (
            "statement-development",
            "development-clean-statement-01",
            "development-clean-subscription-01",
            None,
        ),
        (
            "statement-exam",
            "exam-clean-statement-01",
            "exam-clean-subscription-01",
            Some("exam"),
        ),
    ] {
        std::fs::write(
            dir.join(format!("{stem}.csv")),
            "Date,Description,Debit\n2026-01-01,SYNTHETIC MERCHANT,9.99\n",
        )
        .expect("write synthetic statement");
        let mut expected = serde_json::json!({
            "fixture_id": fixture_id,
            "normalise": [{
                "raw": "SYNTHETIC MERCHANT",
                "name": "Synthetic Merchant"
            }],
            "classify": [{
                "id": item_id,
                "strata": ["clean"],
                "name": "Synthetic Merchant",
                "kind": "subscription",
                "category": "software"
            }],
            "recurring": [],
            "tolerances": {}
        });
        if let Some(eval_set) = eval_set {
            expected["eval_set"] = serde_json::Value::String(eval_set.to_owned());
        }
        std::fs::write(
            dir.join(format!("{stem}.expected.json")),
            serde_json::to_string_pretty(&expected).expect("expectations serialise"),
        )
        .expect("write expectations");
    }

    let development = fixtures_at_for_eval(&dir, &[], &[], EvalSelection::Development, &[])
        .expect("development fixtures readable");
    assert_eq!(
        development
            .iter()
            .map(|fixture| fixture.name.as_str())
            .collect::<Vec<_>>(),
        ["statement-development.csv"]
    );

    let pack_version_bump = fixtures_at_for_eval(&dir, &[], &[], EvalSelection::Exam, &[])
        .expect("exam fixtures readable");
    assert_eq!(
        pack_version_bump
            .iter()
            .map(|fixture| fixture.name.as_str())
            .collect::<Vec<_>>(),
        ["statement-exam.csv"]
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn generated_bed_has_evidence_for_every_gated_class_and_stratum() {
    let pack = load_pack(Path::new("../../packs/app.kttl.subscription-audit")).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures readable");

    for eval_set in [EvalSet::Development, EvalSet::Exam] {
        let selected = fixtures
            .iter()
            .filter(|fixture| fixture.expected.eval_set == eval_set)
            .collect::<Vec<_>>();
        let generated = selected
            .iter()
            .filter(|fixture| fixture.expected.fixture_id.contains("-generated-"))
            .collect::<Vec<_>>();
        assert!(
            generated.len() >= 70,
            "{eval_set:?} needs many small statements, found {}",
            generated.len()
        );
        assert!(
            generated
                .iter()
                .all(|fixture| fixture.expected.classify.len() <= 14),
            "{eval_set:?} contains an oversized generated statement"
        );

        for (stratum, declaration) in &pack.manifest.eval_strata {
            for class in declaration.classes.keys() {
                let n = selected
                    .iter()
                    .flat_map(|fixture| &fixture.expected.classify)
                    .filter(|item| item.strata.contains(stratum))
                    // This bed is the subscription pack's; a class from
                    // another metric's vocabulary counts nothing here.
                    .filter(|item| match class {
                        HarmClass::Subscription => item.kind == "subscription",
                        HarmClass::NotSubscription => item.kind != "subscription",
                        HarmClass::Obligation | HarmClass::NoObligation => false,
                    })
                    .count();
                assert!(
                    n >= 110,
                    "{eval_set:?} {stratum}/{class:?} has n={n}; one error must not decide its 5% ceiling"
                );
            }
        }
    }
}
