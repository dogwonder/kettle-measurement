//! The no-model floor (#73): the same pipeline, the same fixtures, the
//! same scorers — and no model anywhere in the room.
//!
//! The realistic benchmark for a model-powered service is not
//! perfection, it is beating the alternative that doesn't need the
//! model. These tests pin what that alternative honestly produces, so
//! every tier's margin over it is a measured fact.
//!
//! CONTRACT (#73): these tests are the specification. If one seems
//! wrong, stop and report it rather than editing it — a reported defect
//! in the contract is a good outcome, not a failure to finish.

use runner::eval::fixture::{EvalSet, FixtureEvaluator};
use runner::eval::{EvalMetric, MachineInfo, MetricReport, Verdict};
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
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
        // No model, and the report says so — never a placeholder.
        model: None,
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
    }
}

/// The issue's named first test. Running the eval with model steps
/// answered deterministically produces a stable, repeatable score
/// strictly below the pack's thresholds — if pass-through alone met the
/// bar, the fixture or the threshold would be wrong, and this test
/// exists to say so.
#[test]
fn no_model_baseline_scores_fixture_deterministically() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let thresholds = pack.thresholds();

    let first = floor_evaluator().evaluate(&pack).expect("the floor runs");
    let second = floor_evaluator().evaluate(&pack).expect("and runs again");

    // Deterministic means deterministic: everything a score is made of
    // is identical between two runs. (Not the whole report — perf
    // timings move with the machine, and that is fine.)
    for (a, b) in first.fixtures.iter().zip(&second.fixtures) {
        assert_eq!(a.step_scores, b.step_scores, "{}", a.fixture);
        assert_eq!(a.end_to_end, b.end_to_end, "{}", a.fixture);
        assert_eq!(a.needs_review_rate, b.needs_review_rate, "{}", a.fixture);
    }
    assert_eq!(first.verdict, second.verdict);

    let statement_01 = first
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture == "statement-01.csv")
        .expect("the original clean fixture");

    // Classification is not an accuracy step. With no model, every
    // item is surfaced for review: recall is therefore complete,
    // precision is undefined because Kettle asserted nothing, and the
    // 100% review rate below records the human cost.
    assert!(!statement_01.step_scores.contains_key("classify"));
    let MetricReport::Classification(classification) = &first.metrics[&EvalMetric::Classification]
    else {
        panic!("an audit pack reports classification metrics");
    };
    let subscription = &classification.overall.kinds["subscription"];
    assert_eq!(subscription.recall.estimate, Some(1.0));
    assert_eq!(subscription.precision.estimate, None);
    assert_eq!(
        subscription.confident_wrong_rate.estimate,
        Some(0.0),
        "the floor asserts nothing, so it can never be confidently wrong"
    );

    // normalise: pass-through cleaned strings earn whatever they earn,
    // and it must be under the bar. "PUREGYM LTD" is not "PureGym".
    let normalise = &statement_01.step_scores["normalise"];
    let bar = thresholds.step("normalise").expect("the pack sets a bar");
    assert!(
        normalise.score < bar,
        "pass-through scored {} against a bar of {bar} — if the cleaned \
         strings alone clear it, the fixture or the threshold is wrong",
        normalise.score,
    );

    // The transformation #253 bought: recurrence is deterministic, so
    // the floor finds every expected series *exactly* — with kinds
    // derived from cadence and the pack's policy, every one of them
    // low-confidence and in front of a person. The report is complete;
    // only the labels are missing.
    assert_eq!(
        statement_01.end_to_end, 1.0,
        "a floor that misses a series is a Rust bug, not a model gap"
    );
    assert_eq!(
        statement_01.needs_review_rate, 1.0,
        "and every merchant is a person's work — the cost the model \
         exists to reduce. This pair of numbers is the margin statement: \
         the model buys review-rate, nothing else."
    );

    // The floor does not pass, and the reason moved (#253): not
    // because it finds nothing — it finds everything — but because
    // pass-through merchant names sit under the normalise bar. A tier
    // is worth recommending for the review rate it removes and the
    // names it restores.
    assert_eq!(first.verdict, Verdict::Fail);

    // And the report is honest about there having been no model.
    assert!(first.model.is_none());
    assert_eq!(first.model_name(), "without a model");
}

/// The floor never opens a connection. `Answers::WithoutModel` carries
/// no endpoint by construction, so this is enforced by the type — the
/// test documents the claim where a person will read it, and holds the
/// door shut against a refactor that weakens the type.
#[test]
fn the_floor_asks_no_model_anything() {
    let pack = load_pack(&pack_dir()).expect("pack loads");

    // No sidecar, no mock, no port. If this evaluation ever needs a
    // server, it fails loudly here rather than measuring the wrong
    // thing quietly.
    let report = floor_evaluator().evaluate(&pack).expect("the floor runs");

    assert_eq!(
        report.fixtures.len(),
        84,
        "every development fixture is still scored; exam remains sealed"
    );
    let broad = report
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture == "statement-06-broad.csv")
        .expect("the broad fixture is part of the floor");
    assert_eq!(broad.end_to_end, 1.0, "it expects no recurring series");
    assert_eq!(broad.needs_review_rate, 1.0);
    assert!(
        report.sidecar.is_none(),
        "no server answered, none is named"
    );
}

#[test]
fn exam_floor_is_a_separate_sealed_measurement() {
    let pack = load_pack(&pack_dir()).expect("pack loads");

    let report = floor_evaluator()
        .evaluate_exam(&pack)
        .expect("the exam floor runs");

    assert_eq!(report.eval_set, EvalSet::Exam);
    assert_eq!(report.fixtures.len(), 81);
    assert!(report
        .fixtures
        .iter()
        .all(|fixture| fixture.fixture.starts_with("generated-exam-")));
}
