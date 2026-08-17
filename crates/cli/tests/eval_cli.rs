//! `kettle eval` — the CLI surface, the baseline comparison and the exit
//! codes (#38). Everything here is decided without a model in the room:
//! the evaluation itself sits behind [`Evaluator`], and these tests drive
//! the command through canned [`EvalReport`]s. The model-backed
//! implementation is #25.
//!
//! CI runs no model and touches no network (CLAUDE.md), so these tests
//! must not either.

use cli::eval::{self, EvalRequest, Evaluator, ExitCode, Options};
use runner::eval::{
    classification_metrics, extraction_metrics, Classification, ClassificationOutcome,
    ClassificationStratum, ConfidentWrongCeiling, ContainmentBoundary, ContainmentMetrics,
    EvalMetric, EvalReport, ExpectedObligation, Extracted, ExtractionOutcome, FixtureResult,
    HarmClass, MachineInfo, MetricReport, ModelExchange, ModelInfo, Perf, RuntimePolicy,
    ScoredDecision, ScoredItem, SidecarInfo, StepScore, Verdict,
};
use runner::sidecar::Reasoning;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Scratch space and canned reports

/// A per-test scratch directory. Tests share one process, so pid + name
/// is unique enough (same posture as the cache and pack-loader tests).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-eval-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// A packs directory holding the named (empty but present) packs — the
/// command only needs to know a pack exists before it starts.
fn packs_dir(dir: &Path, packs: &[&str]) -> PathBuf {
    let packs_dir = dir.join("packs");
    for pack in packs {
        let pack_dir = packs_dir.join(pack);
        std::fs::create_dir_all(&pack_dir).expect("create pack dir");
        std::fs::write(pack_dir.join("pack.json"), "{}").expect("write pack.json");
    }
    packs_dir
}

/// A report every fixture of which passes cleanly.
fn report(pack: &str, model: &str) -> EvalReport {
    EvalReport {
        reused_fixtures: 0,
        pack: pack.to_owned(),
        pack_version: "1.0.0".to_owned(),
        eval_set: runner::eval::fixture::EvalSet::Development,
        model: Some(ModelInfo {
            file: model.to_owned(),
            params: "3B".to_owned(),
            quant: "Q4_K_M".to_owned(),
            context: 8192,
        }),
        machine: MachineInfo {
            cpu: "Apple M1".to_owned(),
            ram_gb: 8,
            os: "macOS 15.5".to_owned(),
        },
        evidence: None,
        relations: Vec::new(),
        sidecar: None,
        fixtures: vec![fixture("statement-01.csv", 0.88, 0.91)],
        // The fixture predates bed identity, so the note path (#320) is
        // what most of these tests exercise. Tests that care set it.
        bed: None,
        // Likewise for the runtime policy (#232).
        runtime: None,
        metrics: BTreeMap::new(),
        verdict: Verdict::Pass,
    }
}

fn fixture(name: &str, normalise: f32, classify: f32) -> FixtureResult {
    let mut step_scores = BTreeMap::new();
    step_scores.insert("normalise".to_owned(), step(normalise));
    step_scores.insert("classify".to_owned(), step(classify));
    FixtureResult {
        fixture: name.to_owned(),
        step_scores,
        items: Vec::new(),
        containment: Default::default(),
        end_to_end: 0.96,
        needs_review_rate: 0.12,
        perf: Perf {
            wall_ms: 250_000,
            model_ms: 240_000,
            tokens_per_second: 21.5,
            peak_rss_mb: 3_400,
            retries: 0,
        },
        stability: None,
    }
}

fn step(score: f32) -> StepScore {
    StepScore {
        score,
        expected: 50,
        correct: (score * 50.0).round() as usize,
    }
}

/// An [`Evaluator`] that hands back prepared answers in order, and
/// remembers what it was asked.
struct Canned {
    answers: Mutex<std::collections::VecDeque<Result<EvalReport, String>>>,
    asked: Mutex<Vec<String>>,
}

impl Canned {
    fn new(answers: Vec<EvalReport>) -> Canned {
        Canned {
            answers: Mutex::new(answers.into_iter().map(Ok).collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn failing(message: &str) -> Canned {
        Canned {
            answers: Mutex::new([Err(message.to_owned())].into()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("asked").clone()
    }
}

impl Evaluator for Canned {
    fn evaluate(&self, request: &EvalRequest) -> Result<EvalReport, String> {
        self.asked.lock().expect("asked").push(format!(
            "{} {} run {} of {} fixtures {} exam {}",
            request.pack,
            request
                .model
                .map(|model| model.file.as_str())
                .unwrap_or("without a model"),
            request.run,
            request.runs,
            request
                .fixture_dir
                .map(|dir| dir.display().to_string())
                .unwrap_or_else(|| "<pack>".to_owned()),
            request.exam,
        ));
        self.answers
            .lock()
            .expect("answers")
            .pop_front()
            .unwrap_or_else(|| Err("the test ran out of canned answers".to_owned()))
    }
}

/// The options a plain `kettle eval <pack> --model <file>` would build.
fn options(packs_dir: &Path, pack: &str) -> Options {
    Options {
        resume: false,
        replay: None,
        pack: Some(pack.to_owned()),
        all: false,
        packs_dir: packs_dir.to_path_buf(),
        model: Some(PathBuf::from("qwen2.5-3b-instruct-q4_k_m.gguf")),
        models: None,
        no_model: false,
        runs: 1,
        baseline: None,
        write_baseline: None,
        write_tiers: false,
        fixture_dir: None,
        exam: false,
    }
}

const PACK: &str = "app.kttl.subscription-audit";

/// The other declared metric's pack: one where the model extracts
/// obligations from a letter rather than classifying a line (#279).
const LETTER_PACK: &str = "app.kttl.letter-to-actions";

/// The moment tests that don't care about the date are run at. #84
/// made the date real; these tests are about scores, so they pin it and
/// forget it.
const NOW: &str = "2026-07-21T09:30:00Z";

/// [`eval::run`] at [`NOW`].
fn run_at(options: &Options, evaluator: &dyn Evaluator) -> eval::Outcome {
    eval::run(options, evaluator, at(NOW))
}

fn write_baseline(dir: &Path, reports: &[EvalReport]) -> PathBuf {
    let path = dir.join("baseline.json");
    std::fs::write(&path, eval::baseline::to_json(reports, at(NOW))).expect("write baseline");
    path
}

// ---------------------------------------------------------------------------
// The safety net: --baseline exits non-zero on regression

#[test]
fn baseline_regression_exits_nonzero() {
    let dir = scratch("baseline-regression");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(&dir, &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);

    // The same model on the same fixture, now worse at normalising.
    let mut worse = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    worse.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.71));
    worse.verdict = Verdict::Fail;

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![worse]));

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
    assert_eq!(outcome.code.as_i32(), 1, "{}", outcome.text);
    assert!(outcome.text.contains("normalise"), "{}", outcome.text);
    assert!(outcome.text.contains("0.88"), "{}", outcome.text);
    assert!(outcome.text.contains("0.71"), "{}", outcome.text);
    assert!(outcome.text.contains("PASS"), "{}", outcome.text);
    assert!(outcome.text.contains("FAIL"), "{}", outcome.text);
}

fn classification_item(actual_kind: &str, response: &str) -> ScoredItem {
    ScoredItem {
        id: format!("{PACK}/clean-everyday-01/monthly-fitness-membership-01"),
        item_id: "monthly-fitness-membership-01".to_owned(),
        pack: PACK.to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:classify-prompt".to_owned(),
        fixture: "statement-01.csv".to_owned(),
        fixture_id: "clean-everyday-01".to_owned(),
        strata: vec!["clean".to_owned()],
        raw_input: "PUREGYM LTD".to_owned(),
        decision_key: "PureGym".to_owned(),
        decision: ScoredDecision::Classification {
            expected: Classification {
                kind: "subscription".to_owned(),
                category: "fitness".to_owned(),
            },
            actual: ClassificationOutcome::Classified {
                classification: Classification {
                    kind: actual_kind.to_owned(),
                    category: "fitness".to_owned(),
                },
            },
            // A gym membership with a detected series: the pack's
            // category map chose the kind (#272).
            kind_from: Some(runner::kinds::KindFrom::CategoryMap),
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: vec![ModelExchange {
            step: "classify".to_owned(),
            batch: 1,
            request: format!("classify PureGym for {response}"),
            response: response.to_owned(),
        }],
    }
}

fn obligation() -> ExpectedObligation {
    ExpectedObligation {
        kind: "respond".to_owned(),
        party: "the council".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "You must respond within 14 days.".to_owned(),
        due: chrono::NaiveDate::from_ymd_opt(2026, 8, 14),
    }
}

fn found(obligation: ExpectedObligation) -> ExtractionOutcome {
    ExtractionOutcome::Found {
        extracted: Extracted::Obligation(obligation),
    }
}

fn extraction_item(
    item_id: &str,
    expected: Option<ExpectedObligation>,
    actual: ExtractionOutcome,
) -> ScoredItem {
    ScoredItem {
        id: format!("{LETTER_PACK}/letter-01/{item_id}"),
        item_id: item_id.to_owned(),
        pack: LETTER_PACK.to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:obligations-prompt".to_owned(),
        fixture: "letter-01.txt".to_owned(),
        fixture_id: "letter-01".to_owned(),
        strata: vec!["letter".to_owned()],
        raw_input: "You must respond within 14 days.".to_owned(),
        decision_key: "you must respond within 14 days.".to_owned(),
        decision: ScoredDecision::Extraction {
            expected_review: false,
            expected: expected.map(Extracted::Obligation),
            unauthored_negative: false,
            actual,
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: vec![ModelExchange {
            step: "obligations".to_owned(),
            batch: 1,
            request: "what does this passage oblige?".to_owned(),
            response: "an obligation".to_owned(),
        }],
    }
}

fn clean_classification_declarations() -> BTreeMap<String, ClassificationStratum> {
    BTreeMap::from([(
        "clean".to_owned(),
        ClassificationStratum {
            description: "Clear merchant strings.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Initial appliance-risk ceiling.".to_owned(),
                    date: chrono::NaiveDate::from_ymd_opt(2026, 7, 29).unwrap(),
                },
            )]),
        },
    )])
}

/// The item diff is the finding and must precede the aggregate that it
/// explains. Both answers and both raw exchanges stay together so a
/// prompt edit can be diagnosed from this output, rather than by
/// hunting through two run directories.
#[test]
fn baseline_diffs_put_discordant_items_and_raw_exchanges_before_aggregates() {
    let dir = scratch("baseline-item-diff");
    let packs = packs_dir(&dir, &[PACK]);

    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before.fixtures[0].items = vec![classification_item("subscription", "baseline raw response")];
    before.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(classification_metrics(&before.fixtures[0].items)),
    );
    before.fixtures[0]
        .step_scores
        .insert("classify".into(), step(1.0));
    let baseline = write_baseline(&dir, &[before]);

    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.fixtures[0].items = vec![classification_item("regular_spend", "current raw response")];
    now.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(classification_metrics(&now.fixtures[0].items)),
    );
    now.fixtures[0]
        .step_scores
        .insert("classify".into(), step(0.5));
    now.verdict = Verdict::Fail;

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);
    let outcome = run_at(&options, &Canned::new(vec![now]));

    let item_at = outcome
        .text
        .find("monthly-fitness-membership-01")
        .expect("the discordant item is named");
    let aggregate_at = outcome
        .text
        .find("paired classification comparison")
        .expect("the paired result is named");
    assert!(item_at < aggregate_at, "{}", outcome.text);
    assert!(
        outcome.text.contains("subscription / fitness")
            && outcome.text.contains("regular_spend / fitness"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("baseline raw response")
            && outcome.text.contains("current raw response"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("1 discordant pair"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("too few"), "{}", outcome.text);
    assert!(outcome.text.contains("McNemar"), "{}", outcome.text);
}

#[test]
fn multiple_builds_are_compared_as_paired_classifications() {
    let dir = scratch("paired-builds");
    let packs = packs_dir(&dir, &[PACK]);
    let models = dir.join("models.toml");
    std::fs::write(
        &models,
        "[[model]]\nfile = \"models/build-a.gguf\"\nparams = \"7B\"\nquant = \"Q4_K_M\"\n\n\
         [[model]]\nfile = \"models/build-b.gguf\"\nparams = \"7B\"\nquant = \"Q4_K_M\"\n",
    )
    .expect("write models.toml");

    let mut first = report(PACK, "build-a.gguf");
    first.fixtures[0].items = vec![classification_item("subscription", "build A raw response")];
    first.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(classification_metrics(&first.fixtures[0].items)),
    );
    let mut second = report(PACK, "build-b.gguf");
    second.fixtures[0].items = vec![classification_item("regular_spend", "build B raw response")];
    second.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(classification_metrics(&second.fixtures[0].items)),
    );

    let mut options = options(&packs, PACK);
    options.model = None;
    options.models = Some(models);
    let outcome = run_at(&options, &Canned::new(vec![first, second]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    let item_at = outcome
        .text
        .find("monthly-fitness-membership-01")
        .expect("the changed item is the finding");
    let aggregate_at = outcome
        .text
        .find("paired classification comparison")
        .expect("the builds have a paired result");
    assert!(item_at < aggregate_at, "{}", outcome.text);
    assert!(
        outcome.text.contains("build-a.gguf") && outcome.text.contains("build-b.gguf"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("build A raw response")
            && outcome.text.contains("build B raw response"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("1 discordant pair"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("too few"), "{}", outcome.text);
    assert!(outcome.text.contains("McNemar"), "{}", outcome.text);
}

#[test]
fn baseline_unchanged_exits_zero() {
    let dir = scratch("baseline-unchanged");
    let packs = packs_dir(&dir, &[PACK]);
    let same = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let baseline = write_baseline(&dir, std::slice::from_ref(&same));

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![same]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert_eq!(outcome.code.as_i32(), 0);
    assert!(
        outcome.text.contains("Nothing got worse than the baseline"),
        "{}",
        outcome.text
    );
}

#[test]
fn getting_better_is_not_a_regression() {
    let dir = scratch("baseline-improvement");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(&dir, &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);

    let mut better = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    better.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.94));
    better.fixtures[0].needs_review_rate = 0.05;

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![better]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome.text.contains("Better than the baseline"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("0.94"), "{}", outcome.text);
}

#[test]
fn review_rate_movement_is_reported_as_cost_not_regression() {
    let dir = scratch("baseline-review-rate");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(&dir, &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);

    // Every score identical; a fifth of the statement now goes to a
    // person instead of an eighth.
    let mut busier = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    busier.fixtures[0].needs_review_rate = 0.20;

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![busier]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome.text.contains("the share sent for review"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("cost"), "{}", outcome.text);
}

#[test]
fn a_fixture_the_baseline_scored_and_this_eval_didnt_is_a_regression() {
    let dir = scratch("baseline-missing-fixture");
    let packs = packs_dir(&dir, &[PACK]);
    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before
        .fixtures
        .push(fixture("statement-02-messy.csv", 0.86, 0.90));
    let baseline = write_baseline(&dir, &[before]);

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
    assert!(
        outcome.text.contains("statement-02-messy.csv"),
        "{}",
        outcome.text
    );
}

#[test]
fn a_new_fixture_is_not_a_regression() {
    let dir = scratch("baseline-new-fixture");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(&dir, &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);

    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.fixtures
        .push(fixture("statement-03-new.csv", 0.20, 0.30));

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
}

#[test]
fn a_model_the_baseline_measured_and_this_eval_didnt_is_a_regression() {
    let dir = scratch("baseline-missing-model");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(
        &dir,
        &[
            report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf"),
            report(PACK, "gemma-3-4b-it-q4_k_m.gguf"),
        ],
    );

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
    assert!(
        outcome.text.contains("gemma-3-4b-it-q4_k_m.gguf"),
        "{}",
        outcome.text
    );
}

#[test]
fn renaming_a_pack_out_from_under_a_baseline_is_visible() {
    let dir = scratch("baseline-renamed-pack");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(
        &dir,
        &[report(
            "subscription-audit",
            "qwen2.5-3b-instruct-q4_k_m.gguf",
        )],
    );

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);
    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
    assert!(
        outcome
            .text
            .contains("subscription-audit: the baseline measured"),
        "the old id must be visible rather than silently losing the guard: {}",
        outcome.text
    );
}

#[test]
fn a_missing_baseline_file_is_a_problem_not_a_regression() {
    let dir = scratch("baseline-missing-file");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.baseline = Some(dir.join("evals").join("nothing-here.json"));

    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert_eq!(outcome.code.as_i32(), 2);
    assert!(
        outcome.text.contains("--write-baseline"),
        "{}",
        outcome.text
    );
}

#[test]
fn write_baseline_records_what_this_eval_measured() {
    let dir = scratch("write-baseline");
    let packs = packs_dir(&dir, &[PACK]);
    let mut measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    measured.fixtures[0].items = vec![classification_item("subscription", "raw response")];
    measured.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(
            classification_metrics(&measured.fixtures[0].items)
                .with_gates(&clean_classification_declarations()),
        ),
    );

    let mut options = options(&packs, PACK);
    let path = dir.join("evals").join("baseline.json");
    options.write_baseline = Some(path.clone());

    let outcome = run_at(&options, &Canned::new(vec![measured.clone()]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    let written = std::fs::read_to_string(&path).expect("baseline written");
    assert_eq!(
        written,
        eval::baseline::to_json(std::slice::from_ref(&measured), at(NOW))
    );
    let json: serde_json::Value = serde_json::from_str(&written).expect("baseline JSON");
    let recall = &json["reports"][0]["metrics"]["classification"]["overall"]["kinds"]
        ["subscription"]["recall"];
    assert_eq!(recall["n"], 1);
    assert!(
        recall["wilson_95"]["low"].is_number(),
        "baseline.json must carry the evidence behind every classification proportion: {written}"
    );
    let gate = &json["reports"][0]["metrics"]["classification"]["gates"]["clean"]["subscription"];
    assert_eq!(gate["observed"]["n"], 1);
    assert_eq!(gate["max_wilson_95"], 0.05);
    // One decision cannot demonstrate a 5% ceiling however it turns
    // out, and the baseline records which of the three that was (#310).
    assert_eq!(gate["outcome"], "unproven");
    assert_eq!(gate["decisions_needed"], 73);
    assert_eq!(gate["reason"], "Initial appliance-risk ceiling.");
    assert_eq!(gate["date"], "2026-07-29");

    // And it reads back as a baseline that matches itself.
    let mut again = options;
    again.write_baseline = None;
    again.baseline = Some(path);
    let outcome = run_at(&again, &Canned::new(vec![measured]));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
}

// ---------------------------------------------------------------------------
// #39: tiers.json, the file the model-manager screen quotes from

/// Read the `tiers.json` a pack now has.
fn tiers_of(packs_dir: &Path, pack: &str) -> serde_json::Value {
    let path = packs_dir.join(pack).join("tiers.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("tiers.json is JSON")
}

/// The one sentence the file exists to make true is "on a machine like
/// yours, this task is typically 95% automatic" — and "automatic" is the
/// share that did *not* go to a person, not the end-to-end score. The
/// brief spells the sentence out both ways ("68% automatic — you'll check
/// about 1 in 3 items yourself"), and only the review rate reconciles the
/// two halves.
#[test]
fn write_tiers_records_the_share_that_never_reached_a_person() {
    let dir = scratch("write-tiers");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.write_tiers = true;
    let mut measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    measured.fixtures[0].items = vec![classification_item("subscription", "raw response")];
    measured.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(
            classification_metrics(&measured.fixtures[0].items)
                .with_gates(&clean_classification_declarations()),
        ),
    );

    let outcome = run_at(&options, &Canned::new(vec![measured]));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let written = tiers_of(&packs, PACK);
    assert_eq!(written["pack"], PACK);

    let tiers = written["tiers"].as_array().expect("a tiers array");
    assert_eq!(tiers.len(), 1, "{written}");
    let tier = &tiers[0];
    assert_eq!(tier["model"]["file"], "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(tier["machine"]["cpu"], "Apple M1");
    // Provenance is the entry's, not the file's — see
    // `an_entry_keeps_the_provenance_it_was_measured_under`.
    assert_eq!(tier["pack_version"], "1.0.0");
    assert_eq!(
        tier["scoring_version"],
        eval::baseline::SCORING_VERSION,
        "{written}"
    );
    assert_eq!(tier["verdict"], "pass");
    assert_eq!(tier["runs"], 1, "{written}");
    assert_eq!(
        tier["steady"], true,
        "one run cannot disagree with itself: {written}"
    );

    // The fixture sent 12% for review and scored 0.96 end-to-end.
    assert_close(tier["automatic"].as_f64(), 1.0 - 0.12);
    assert_close(tier["end_to_end"].as_f64(), 0.96);
    assert_close(tier["steps"]["normalise"]["score"].as_f64(), 0.88);
    assert_eq!(tier["steps"]["normalise"]["n"], 50);
    assert!(
        tier["steps"]["normalise"]["wilson_95"]["low"].is_number(),
        "a tier score must carry its sample size and interval: {written}"
    );
    assert_eq!(
        tier["metrics"]["classification"]["overall"]["kinds"]["subscription"]["recall"]["n"], 1,
        "tiers.json must carry the evidence behind every classification proportion: {written}"
    );
    assert_close(
        tier["metrics"]["classification"]["overall"]["kinds"]["subscription"]["recall"]
            ["wilson_95"]["low"]
            .as_f64(),
        0.206_549_314_377_237_45,
    );
    let gate = &tier["metrics"]["classification"]["gates"]["clean"]["subscription"];
    assert_eq!(gate["observed"]["n"], 1);
    assert_eq!(gate["max_wilson_95"], 0.05);
    assert_eq!(gate["reason"], "Initial appliance-risk ceiling.");
    assert_eq!(gate["date"], "2026-07-29");
}

fn assert_close(got: Option<f64>, want: f64) {
    let got = got.unwrap_or_else(|| panic!("expected a number, near {want}"));
    assert!(
        (got - want).abs() < 1e-6,
        "expected about {want}, got {got}"
    );
}

/// A tier is one model *on one machine*, so a second measurement of the
/// same pair replaces the first rather than piling up beside it. Nobody
/// wants to read a file that grows a row every time an eval is run.
#[test]
fn measuring_the_same_model_on_the_same_machine_replaces_its_entry() {
    let dir = scratch("write-tiers-replace");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.write_tiers = true;

    let first = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(
        run_at(&options, &Canned::new(vec![first.clone()])).code,
        ExitCode::Ok
    );

    // The same weights, the same laptop, a prompt edit later.
    let mut better = first;
    better.fixtures[0].needs_review_rate = 0.04;
    assert_eq!(
        run_at(&options, &Canned::new(vec![better])).code,
        ExitCode::Ok
    );

    let written = tiers_of(&packs, PACK);
    let tiers = written["tiers"].as_array().expect("a tiers array");
    assert_eq!(tiers.len(), 1, "{written}");
    assert_close(tiers[0]["automatic"].as_f64(), 1.0 - 0.04);
}

/// The one behaviour the file lives or dies by. Brief §5 keeps an old
/// 8GB laptop as the honesty check; a measurement taken on a 32GB machine
/// must not silently delete its numbers, because those are the ones that
/// make the baseline tier claim tested fact rather than assumption.
#[test]
fn a_measurement_on_another_machine_does_not_delete_the_old_one() {
    let dir = scratch("write-tiers-other-machine");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.write_tiers = true;

    // The honesty check: the old 8GB laptop, which `report` already is.
    let small = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(
        run_at(&options, &Canned::new(vec![small])).code,
        ExitCode::Ok
    );

    // The same weights on the machine the eval usually runs on.
    let mut roomy = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    roomy.machine = MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 32,
        os: "macOS 26.5.2".to_owned(),
    };
    roomy.fixtures[0].needs_review_rate = 0.02;
    assert_eq!(
        run_at(&options, &Canned::new(vec![roomy])).code,
        ExitCode::Ok
    );

    let written = tiers_of(&packs, PACK);
    let tiers = written["tiers"].as_array().expect("a tiers array");
    assert_eq!(tiers.len(), 2, "{written}");
    // The 8GB numbers are still there, and still say 8GB.
    assert_eq!(tiers[0]["machine"]["ram_gb"], 8, "{written}");
    assert_close(tiers[0]["automatic"].as_f64(), 1.0 - 0.12);
    assert_eq!(tiers[1]["machine"]["ram_gb"], 32, "{written}");
    assert_close(tiers[1]["automatic"].as_f64(), 1.0 - 0.02);
}

/// Two models on one machine are two claims, not one.
#[test]
fn another_model_on_the_same_machine_is_another_tier() {
    let dir = scratch("write-tiers-other-model");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.write_tiers = true;

    assert_eq!(
        run_at(
            &options,
            &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf",)])
        )
        .code,
        ExitCode::Ok
    );
    assert_eq!(
        run_at(
            &options,
            &Canned::new(vec![report(PACK, "gemma-3-4b-it-q4_k_m.gguf",)])
        )
        .code,
        ExitCode::Ok
    );

    let written = tiers_of(&packs, PACK);
    let files: Vec<&str> = written["tiers"]
        .as_array()
        .expect("a tiers array")
        .iter()
        .map(|tier| tier["model"]["file"].as_str().expect("a file name"))
        .collect();
    assert_eq!(
        files,
        [
            "qwen2.5-3b-instruct-q4_k_m.gguf",
            "gemma-3-4b-it-q4_k_m.gguf"
        ],
        "{written}"
    );
}

/// A tiers.json holds measurements taken on machines that may not be to
/// hand. If it cannot be read it cannot be merged into, and writing
/// anyway would throw away the ones that are hardest to take again — so
/// the command stops, says why in a sentence, and leaves the file be.
#[test]
fn a_tiers_file_that_cannot_be_read_is_explained_and_left_alone() {
    let dir = scratch("write-tiers-corrupt");
    let packs = packs_dir(&dir, &[PACK]);

    let path = packs.join(PACK).join("tiers.json");
    let mangled = "{ \"pack\": \"app.kttl.subscription-audit\", \"tiers\": [ truncated";
    std::fs::write(&path, mangled).expect("write a half-written tiers.json");

    let mut options = options(&packs, PACK);
    options.write_tiers = true;

    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(
        outcome.text.contains("tiers.json"),
        "the message should name the file: {}",
        outcome.text
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        mangled,
        "a file that could not be understood must not be written over"
    );
}

/// Decision #52: the file records the measurement for every tier,
/// including the ones that failed, and the screen applies the policy. A
/// data file records facts.
#[test]
fn a_model_that_failed_is_recorded_with_its_verdict() {
    let dir = scratch("write-tiers-fail");
    let packs = packs_dir(&dir, &[PACK]);

    let mut hopeless = report(PACK, "llama-3.2-1b-instruct-q4_k_m.gguf");
    hopeless.verdict = Verdict::Fail;
    hopeless.fixtures[0].needs_review_rate = 0.38;

    let mut options = options(&packs, PACK);
    options.write_tiers = true;

    let outcome = run_at(&options, &Canned::new(vec![hopeless]));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let written = tiers_of(&packs, PACK);
    assert_eq!(written["tiers"][0]["verdict"], "fail", "{written}");
    assert_close(written["tiers"][0]["automatic"].as_f64(), 1.0 - 0.38);
}

/// The screen makes a claim to a person about their own laptop, so every
/// number is the worst run and the worst fixture — never the kinder one,
/// and never a mean of the two.
#[test]
fn a_tier_claims_the_worst_run_and_the_worst_fixture() {
    let dir = scratch("write-tiers-worst");
    let packs = packs_dir(&dir, &[PACK]);

    let mut good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    // A second, messier statement: worse everywhere, and slower.
    let mut messy = fixture("statement-02-messy.csv", 0.79, 0.83);
    messy.end_to_end = 0.90;
    messy.needs_review_rate = 0.22;
    messy.perf.wall_ms = 310_000;
    good.fixtures.push(messy);

    // And one repeat that did worse still on classify.
    let mut wobbly = good.clone();
    wobbly.fixtures[1]
        .step_scores
        .insert("classify".into(), step(0.64));

    let mut options = options(&packs, PACK);
    options.write_tiers = true;
    options.runs = 2;

    let outcome = run_at(&options, &Canned::new(vec![good, wobbly]));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let tier = &tiers_of(&packs, PACK)["tiers"][0];
    assert_close(tier["steps"]["classify"]["score"].as_f64(), 0.64);
    assert_close(tier["steps"]["normalise"]["score"].as_f64(), 0.79);
    assert_close(tier["end_to_end"].as_f64(), 0.90);
    assert_close(tier["automatic"].as_f64(), 1.0 - 0.22);
    assert_eq!(tier["wall_ms"], 310_000, "{tier}");
    assert_eq!(tier["runs"], 2, "{tier}");
    assert_eq!(
        tier["steady"], false,
        "the repeats disagreed, and the file has to say so: {tier}"
    );
}

/// Repeats that agreed are worth recording as such: "we checked and it
/// did not move" is the finding `--runs` was asked for.
#[test]
fn repeats_that_agreed_are_recorded_as_steady() {
    let dir = scratch("write-tiers-steady");
    let packs = packs_dir(&dir, &[PACK]);

    let steady = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");

    let mut options = options(&packs, PACK);
    options.write_tiers = true;
    options.runs = 3;

    let outcome = run_at(
        &options,
        &Canned::new(vec![steady.clone(), steady.clone(), steady]),
    );
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let tier = &tiers_of(&packs, PACK)["tiers"][0];
    assert_eq!(tier["runs"], 3, "{tier}");
    assert_eq!(tier["steady"], true, "{tier}");
}

/// `--all` measures several packs at once, which is why the flag takes no
/// path: each pack's tiers go in its own directory.
#[test]
fn write_tiers_writes_one_file_per_pack_measured() {
    let dir = scratch("write-tiers-all");
    let packs = packs_dir(&dir, &[PACK, "warranty-watch"]);

    let mut options = options(&packs, PACK);
    options.pack = None;
    options.all = true;
    options.write_tiers = true;

    let outcome = run_at(
        &options,
        &Canned::new(vec![
            report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf"),
            report("warranty-watch", "qwen2.5-3b-instruct-q4_k_m.gguf"),
        ]),
    );
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    assert_eq!(tiers_of(&packs, PACK)["pack"], PACK);
    assert_eq!(tiers_of(&packs, "warranty-watch")["pack"], "warranty-watch");
}

/// Without the flag, nothing is written. An eval is a measurement; it
/// only becomes a claim shipped to people when someone says so.
#[test]
fn no_flag_writes_no_tiers() {
    let dir = scratch("write-tiers-absent");
    let packs = packs_dir(&dir, &[PACK]);

    let outcome = run_at(
        &options(&packs, PACK),
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(!packs.join(PACK).join("tiers.json").exists());
}

// ---------------------------------------------------------------------------
// The table

#[test]
fn the_table_has_a_row_per_model_and_a_column_per_step() {
    let dir = scratch("table");
    let packs = packs_dir(&dir, &[PACK]);
    let models = dir.join("models.toml");
    std::fs::write(
        &models,
        "[[model]]\nfile = \"models/qwen2.5-3b-instruct-q4_k_m.gguf\"\n\n\
         [[model]]\nfile = \"models/llama-3.2-1b-instruct-q4_k_m.gguf\"\n",
    )
    .expect("write models.toml");

    let mut weak = report(PACK, "llama-3.2-1b-instruct-q4_k_m.gguf");
    weak.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.61));
    weak.fixtures[0].end_to_end = 0.74;
    weak.fixtures[0].needs_review_rate = 0.38;
    weak.verdict = Verdict::Fail;

    let mut options = options(&packs, PACK);
    options.model = None;
    options.models = Some(models);

    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf"), weak]),
    );

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome.text.contains("app.kttl.subscription-audit v1.0.0"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("development set"), "{}", outcome.text);
    assert!(outcome.text.contains("1 fixture ·"), "{}", outcome.text);
    assert!(outcome.text.contains("1 run ·"), "{}", outcome.text);
    assert!(outcome.text.contains("Apple M1 8GB"), "{}", outcome.text);
    assert!(outcome.text.contains("normalise"), "{}", outcome.text);
    assert!(outcome.text.contains("classify"), "{}", outcome.text);
    assert!(
        outcome.text.contains("normalise (pooled)"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("classify (pooled)"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("e2e (mean)"), "{}", outcome.text);
    assert!(outcome.text.contains("review (mean)"), "{}", outcome.text);
    assert!(outcome.text.contains("verdict"), "{}", outcome.text);
    // Step columns are in the order the report holds them, which is
    // alphabetical: a scored report does not record pipeline order.
    let row = outcome
        .text
        .lines()
        .find(|line| line.starts_with("qwen2.5-3b"))
        .unwrap_or_else(|| panic!("a row for the model: {}", outcome.text));
    assert!(row.contains("0.92 (n=50; 95% CI 0.81–0.97)"), "{row}");
    assert!(row.contains("0.88 (n=50; 95% CI 0.76–0.94)"), "{row}");
    assert!(row.contains("0.96"), "{row}");
    assert!(row.contains("12%"), "{row}");
    assert!(row.contains("4m10s"), "{row}");
    assert!(row.contains("PASS"), "{row}");
    assert!(outcome.text.contains("0.62"), "{}", outcome.text);
    assert!(outcome.text.contains("38%"), "{}", outcome.text);
    assert!(outcome.text.contains("FAIL"), "{}", outcome.text);
}

#[test]
fn classification_table_reports_per_class_n_and_wilson_intervals() {
    let dir = scratch("classification-statistics-table");
    let packs = packs_dir(&dir, &[PACK]);
    let mut measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    measured.fixtures[0].items = vec![classification_item("subscription", "raw response")];
    let declarations = BTreeMap::from([(
        "clean".to_owned(),
        ClassificationStratum {
            description: "Clear merchant strings.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Initial appliance-risk ceiling.".to_owned(),
                    date: chrono::NaiveDate::from_ymd_opt(2026, 7, 29).unwrap(),
                },
            )]),
        },
    )]);
    measured.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(
            classification_metrics(&measured.fixtures[0].items).with_gates(&declarations),
        ),
    );

    let outcome = run_at(&options(&packs, PACK), &Canned::new(vec![measured]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome
            .text
            .contains("classification / overall / kind / subscription"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("precision 1.00 (n=1; 95% CI 0.21\u{2013}1.00)"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("recall 1.00 (n=1; 95% CI 0.21\u{2013}1.00)"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("classification / clean / kind / subscription"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("classification / clean / harm / subscription"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains(
            "gate / clean / subscription: confident-wrong over decisions 0.00 (n=1; 95% CI 0.00\u{2013}0.79) <= 0.05: UNPROVEN — needs 73 decisions, has 1"
        ),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("Initial appliance-risk ceiling. (2026-07-29)"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("classify (mean)"),
        "classification accuracy must not survive beside precision and recall: {}",
        outcome.text
    );
}

#[test]
fn the_table_reports_claim_containment_and_escapes_by_guardrail() {
    let dir = scratch("claim-containment-table");
    let packs = packs_dir(&dir, &[PACK]);
    let mut measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    measured.fixtures[0].containment = ContainmentMetrics {
        candidates: 3,
        accepted: 1,
        needs_review: 1,
        rejected: 1,
        contained: 1,
        escaped: 1,
        by_guardrail: BTreeMap::from([(
            runner::claim_trace::Guardrail::Quote,
            ContainmentBoundary {
                passed: 1,
                failed: 1,
                contained: 1,
                escaped: 1,
            },
        )]),
        ..ContainmentMetrics::default()
    };

    let outcome = run_at(&options(&packs, PACK), &Canned::new(vec![measured]));

    assert!(
        outcome.text.contains(
            "claim containment: 3 candidates; 1 scored decisions surfaced; 1 wrong assertions escaped"
        ),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("claim guardrail / quote: 1 failed; 1 contained; 1 escaped"),
        "{}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("pipeline introduced"),
        "a column nobody recorded must not print a reassuring zero: {}",
        outcome.text
    );
}

/// #470: an error the pipeline introduced prints in the containment
/// summary. A fate with its own column but no line would be invisible
/// exactly where the scorecard is read — and counting it under
/// containment or escapes is the collapse the disposition exists to
/// prevent.
#[test]
fn the_table_names_errors_the_pipeline_introduced() {
    let dir = scratch("pipeline-introduced-table");
    let packs = packs_dir(&dir, &[PACK]);
    let mut measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    measured.fixtures[0].containment = ContainmentMetrics {
        candidates: 2,
        accepted: 1,
        pipeline_introduced: 1,
        ..ContainmentMetrics::default()
    };

    let outcome = run_at(&options(&packs, PACK), &Canned::new(vec![measured]));

    assert!(
        outcome.text.contains(
            "claim containment: 2 candidates; 0 scored decisions surfaced; \
             0 wrong assertions escaped; 1 errors the pipeline introduced"
        ),
        "{}",
        outcome.text
    );
}

/// #272: the confident-wrong line says which decisions produced it.
///
/// Without this, the only way to answer "what do I fix?" is to read raw
/// exchanges by hand — which is how that cell was attributed backwards
/// twice.
#[test]
fn the_confident_wrong_line_names_the_decisions_behind_it() {
    let dir = scratch("confident-wrong-decomposition");
    let packs = packs_dir(&dir, &[PACK]);
    let mut measured = report(PACK, "qwen2.5-7b-instruct-q4_k_m.gguf");
    // One subscription denied because the category was wrong, one
    // because cadence found no series. Both read `regular_spend`.
    measured.fixtures[0].items = vec![classification_item("regular_spend", "wrong category"), {
        let mut cadence = classification_item("regular_spend", "no series");
        cadence.item_id = "monthly-fitness-membership-02".to_owned();
        if let ScoredDecision::Classification { kind_from, .. } = &mut cadence.decision {
            *kind_from = Some(runner::kinds::KindFrom::CadenceDespitePeriodic);
        }
        cadence
    }];
    measured.metrics.insert(
        EvalMetric::Classification,
        MetricReport::Classification(classification_metrics(&measured.fixtures[0].items)),
    );

    let outcome = run_at(&options(&packs, PACK), &Canned::new(vec![measured]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome
            .text
            .contains("via category_map 1, cadence_despite_periodic 1"),
        "{}",
        outcome.text
    );
}

/// The letter pack's 115-minute run printed one table row and nothing
/// else: its declared ceilings were computed, written to the baseline and
/// never shown (#286). The gates are the part a person acts on.
#[test]
fn an_extraction_run_prints_its_harm_classes_and_gates() {
    let dir = scratch("extraction-statistics-table");
    let packs = packs_dir(&dir, &[LETTER_PACK]);
    let mut measured = report(LETTER_PACK, "qwen2.5-7b-instruct-q4_k_m.gguf");
    measured.fixtures[0].items = vec![
        // An obligation the run found: nothing missed, so the obligation
        // ceiling has room to pass.
        extraction_item("obligation-01", Some(obligation()), found(obligation())),
        // A passage that obliges nothing, from which the run asserted an
        // obligation anyway: an invention, so no_obligation's ceiling
        // fails.
        extraction_item("no-obligation-01", None, found(obligation())),
    ];
    let declarations = BTreeMap::from([(
        "letter".to_owned(),
        ClassificationStratum {
            description: "One-page letters.".to_owned(),
            classes: BTreeMap::from([
                (
                    HarmClass::Obligation,
                    ConfidentWrongCeiling {
                        max_wilson_95: 0.85,
                        reason: "A missed obligation is the harm.".to_owned(),
                        date: chrono::NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
                    },
                ),
                (
                    HarmClass::NoObligation,
                    ConfidentWrongCeiling {
                        max_wilson_95: 0.05,
                        reason: "An invented obligation is the harm.".to_owned(),
                        date: chrono::NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
                    },
                ),
            ]),
        },
    )]);
    measured.metrics.insert(
        EvalMetric::Extraction,
        MetricReport::Extraction(
            extraction_metrics(&measured.fixtures[0].items).with_gates(&declarations),
        ),
    );

    let outcome = run_at(&options(&packs, LETTER_PACK), &Canned::new(vec![measured]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    // Per-class performance, overall and per stratum, as classification's.
    assert!(
        outcome
            .text
            .contains("extraction / overall / harm / obligation:"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("extraction / overall / harm / no_obligation:"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("extraction / letter / harm / obligation:"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("precision 0.50 (n=2; 95% CI 0.09\u{2013}0.91)"),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("recall 1.00 (n=1; 95% CI 0.21\u{2013}1.00)"),
        "{}",
        outcome.text
    );
    // Both declared ceilings, each naming its stratum, its class and its
    // provenance.
    assert!(
        outcome.text.contains(
            "gate / letter / obligation: confident-wrong over decisions 0.00 (n=1; 95% CI 0.00\u{2013}0.79) <= 0.85: PASS"
        ),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("A missed obligation is the harm. (2026-07-31)"),
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains(
            "gate / letter / no_obligation: confident-wrong over decisions 1.00 (n=1; 95% CI 0.21\u{2013}1.00) <= 0.05: UNPROVEN — needs 73 decisions, has 1"
        ),
        "{}",
        outcome.text
    );
    assert!(
        outcome
            .text
            .contains("An invented obligation is the harm. (2026-07-31)"),
        "{}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// Choosing packs, models and fixtures

#[test]
fn all_evaluates_every_pack_in_order() {
    let dir = scratch("all-packs");
    let packs = packs_dir(&dir, &[PACK, "warranty-watch"]);

    let mut options = options(&packs, PACK);
    options.pack = None;
    options.all = true;

    let canned = Canned::new(vec![
        report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf"),
        report("warranty-watch", "qwen2.5-3b-instruct-q4_k_m.gguf"),
    ]);
    let outcome = run_at(&options, &canned);

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert_eq!(canned.asked().len(), 2);
    assert!(canned.asked()[0].starts_with(PACK));
    assert!(canned.asked()[1].starts_with("warranty-watch"));
    assert!(
        outcome.text.contains("warranty-watch v1.0.0"),
        "{}",
        outcome.text
    );
}

#[test]
fn an_unknown_pack_is_a_problem() {
    let dir = scratch("unknown-pack");
    let packs = packs_dir(&dir, &[PACK]);

    let outcome = run_at(&options(&packs, "no-such-pack"), &Canned::new(vec![]));

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(
        outcome.text.contains("No pack called no-such-pack"),
        "{}",
        outcome.text
    );
}

#[test]
fn naming_no_model_at_all_is_a_problem() {
    let dir = scratch("no-model");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.model = None;

    let outcome = run_at(&options, &Canned::new(vec![]));

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(outcome.text.contains("--models"), "{}", outcome.text);
}

#[test]
fn fixture_dir_reaches_the_evaluator() {
    let dir = scratch("fixture-dir");
    let packs = packs_dir(&dir, &[PACK]);
    let statements = dir.join("my-statements");
    std::fs::create_dir_all(&statements).expect("create fixture dir");

    let mut options = options(&packs, PACK);
    options.fixture_dir = Some(statements.clone());

    let canned = Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);
    let outcome = run_at(&options, &canned);

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        canned.asked()[0].contains(&statements.display().to_string()),
        "{:?}",
        canned.asked()
    );
}

#[test]
fn exam_selection_reaches_the_evaluator_explicitly() {
    let dir = scratch("exam");
    let packs = packs_dir(&dir, &[PACK]);
    let mut options = options(&packs, PACK);
    options.exam = true;

    let canned = Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);
    let outcome = run_at(&options, &canned);

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        canned.asked()[0].ends_with("exam true"),
        "{:?}",
        canned.asked()
    );
}

#[test]
fn an_evaluator_that_cannot_measure_stops_the_command() {
    let dir = scratch("evaluator-error");
    let packs = packs_dir(&dir, &[PACK]);

    let outcome = run_at(
        &options(&packs, PACK),
        &Canned::failing("that model file isn't there"),
    );

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(outcome.text.contains("isn't there"), "{}", outcome.text);
}

#[test]
fn a_failing_model_with_no_baseline_is_a_measurement_not_an_error() {
    let dir = scratch("fail-no-baseline");
    let packs = packs_dir(&dir, &[PACK]);

    let mut hopeless = report(PACK, "llama-3.2-1b-instruct-q4_k_m.gguf");
    hopeless.verdict = Verdict::Fail;

    let outcome = run_at(&options(&packs, PACK), &Canned::new(vec![hopeless]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(outcome.text.contains("FAIL"), "{}", outcome.text);
}

// ---------------------------------------------------------------------------
// Repeats (--runs)

#[test]
fn runs_repeats_the_measurement() {
    let dir = scratch("runs-repeat");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.runs = 3;

    let steady = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let canned = Canned::new(vec![steady.clone(), steady.clone(), steady]);
    let outcome = run_at(&options, &canned);

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert_eq!(canned.asked().len(), 3);
    assert!(
        canned.asked()[0].contains("run 1 of 3"),
        "{:?}",
        canned.asked()
    );
    assert!(
        canned.asked()[2].contains("run 3 of 3"),
        "{:?}",
        canned.asked()
    );
    // Steady repeats say nothing: no news is the good news.
    assert!(!outcome.text.contains("Warning"), "{}", outcome.text);
    assert!(outcome.text.contains("3 runs"), "{}", outcome.text);
}

#[test]
fn repeats_that_disagree_report_the_worst_run_and_say_so() {
    let dir = scratch("runs-wobble");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.runs = 3;

    let good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let mut wobbly = good.clone();
    wobbly.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.71));
    wobbly.verdict = Verdict::Fail;

    let outcome = run_at(&options, &Canned::new(vec![good.clone(), wobbly, good]));

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    // The worst run is the one in the table.
    assert!(outcome.text.contains("0.71"), "{}", outcome.text);
    assert!(outcome.text.contains("FAIL"), "{}", outcome.text);
    assert!(outcome.text.contains("Warning"), "{}", outcome.text);
    assert!(
        outcome
            .text
            .contains("normalise on statement-01.csv ranged 0.71 to 0.88"),
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("the verdict"), "{}", outcome.text);
}

#[test]
fn the_worst_repeat_is_what_the_baseline_is_judged_against() {
    let dir = scratch("runs-baseline");
    let packs = packs_dir(&dir, &[PACK]);
    let baseline = write_baseline(&dir, &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]);

    let mut options = options(&packs, PACK);
    options.runs = 2;
    options.baseline = Some(baseline);

    let good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let mut bad = good.clone();
    bad.fixtures[0]
        .step_scores
        .insert("classify".into(), step(0.55));
    bad.verdict = Verdict::Fail;

    let outcome = run_at(&options, &Canned::new(vec![good, bad]));

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
}

#[test]
fn zero_runs_is_a_problem() {
    let dir = scratch("zero-runs");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.runs = 0;

    let outcome = run_at(&options, &Canned::new(vec![]));

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(
        outcome.text.contains("at least one run"),
        "{}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// Finding the weights

/// A weights file that is only a name and a size on disk.
///
/// Resolution asks the filesystem whether a path exists and nothing
/// more, so an empty file is a faithful stand-in — and one these tests
/// can rely on. Pointing them at the real `models/` is what broke CI:
/// the directory is gitignored, so the weights are there on the machine
/// that wrote the test and nowhere else (CLAUDE.md: CI downloads no
/// weights).
fn fake_weights(dir: &Path, name: &str) -> PathBuf {
    let models_dir = dir.join("models");
    std::fs::create_dir_all(&models_dir).expect("create models dir");
    let path = models_dir.join(name);
    std::fs::write(&path, b"").expect("write fake weights");
    path
}

/// `--model qwen2.5-7b-instruct-q4_k_m.gguf` is what anyone types, and
/// the weights live in `models/` (CLAUDE.md, workspace layout). A bare
/// file name that isn't in the current directory is looked for there
/// before the eval gives up.
#[test]
fn a_bare_model_name_is_looked_for_in_the_models_directory() {
    let dir = scratch("bare-model-name");
    let weights = fake_weights(&dir, "qwen2.5-7b-instruct-q4_k_m.gguf");
    let models_dir = dir.join("models");

    let resolved = cli::eval::models::resolve_in(
        Some(Path::new("qwen2.5-7b-instruct-q4_k_m.gguf")),
        None,
        &models_dir,
    )
    .expect("resolves");

    assert_eq!(resolved[0].file, "qwen2.5-7b-instruct-q4_k_m.gguf");
    assert_eq!(resolved[0].path, weights);
}

/// A bare name that is nowhere is left as given, so the error names
/// what the person typed rather than a path they never mentioned.
#[test]
fn a_bare_model_name_that_is_nowhere_is_left_alone() {
    let dir = scratch("bare-model-missing");
    std::fs::create_dir_all(dir.join("models")).expect("create empty models dir");

    let resolved = cli::eval::models::resolve_in(
        Some(Path::new("not-a-real-model.gguf")),
        None,
        &dir.join("models"),
    )
    .expect("resolves");

    assert_eq!(resolved[0].path, Path::new("not-a-real-model.gguf"));
}

/// A path the person actually gave is never second-guessed: if it
/// exists, it is the one measured, wherever it points.
///
/// The weights must really be on disk for this to test anything. When
/// they are not, resolution takes the "not a bare name" branch and the
/// assertion holds for the wrong reason — which is how this test spent
/// its whole life passing vacuously in CI.
#[test]
fn a_path_that_exists_is_left_exactly_as_given() {
    let dir = scratch("path-as-given");
    let given = fake_weights(&dir, "somewhere-else.gguf");
    assert!(given.exists(), "the branch under test needs a real file");

    let resolved =
        cli::eval::models::resolve_in(Some(&given), None, Path::new("nowhere")).expect("resolves");

    assert_eq!(resolved[0].path, given);
}

// ---------------------------------------------------------------------------
// #84: a baseline is a document about a moment, and has to say which

/// A fixed "now" for tests, so nothing here depends on the day it runs.
fn at(iso: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .expect("a test timestamp")
        .into()
}

#[test]
fn a_written_baseline_says_when_and_by_which_harness() {
    let dir = scratch("baseline-provenance");
    let packs = packs_dir(&dir, &[PACK]);
    let measured = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");

    let mut options = options(&packs, PACK);
    let path = dir.join("written.json");
    options.write_baseline = Some(path.clone());

    let outcome = eval::run(
        &options,
        &Canned::new(vec![measured]),
        at("2026-07-21T09:30:00Z"),
    );
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let written = std::fs::read_to_string(&path).expect("read the written baseline");
    assert!(
        written.contains("2026-07-21T09:30:00Z"),
        "a baseline that cannot say when it was recorded is a document about \
         a moment it will not name: {written}"
    );
    assert!(
        written.contains(&format!(
            "\"scoring_version\": {}",
            eval::baseline::SCORING_VERSION
        )),
        "{written}"
    );
}

#[test]
fn a_baseline_from_a_different_scoring_version_is_refused_not_compared() {
    let dir = scratch("baseline-scoring-version");
    let packs = packs_dir(&dir, &[PACK]);

    // Recorded when `similarity` still meant something else — every
    // stored score is incomparable to one scored today.
    let mut older = eval::baseline::to_json(
        &[report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")],
        at("2026-05-01T09:00:00Z"),
    );
    assert_eq!(
        eval::baseline::SCORING_VERSION,
        14,
        "a quote must now contain the value it evidences (#460). \
         `quote_is_in` — is this text on the page — was necessary and not \
         sufficient: the 8 August v12 renewal run quoted the bare label \
         `Excess` as evidence for three different numbers, and a quote that \
         supports three values supports none of them. A term whose quote \
         does not contain its value is refused to review at the quote \
         guardrail, so review rates, harm cells and verdicts move wherever \
         a model quoted a label instead of a value, and no version 13 \
         baseline may be compared"
    );
    older = older.replace(
        &format!("\"scoring_version\": {}", eval::baseline::SCORING_VERSION),
        "\"scoring_version\": 3",
    );
    let path = dir.join("older.json");
    std::fs::write(&path, older).expect("write the older baseline");

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
        at("2026-07-21T09:30:00Z"),
    );

    // Not `Regression`: nothing was shown to have got worse. The
    // comparison could not honestly be made at all.
    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
    assert!(
        outcome.text.contains("--write-baseline"),
        "the way out is to re-record it, and the message should say so: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("scoring version 3"),
        "the just-retired baseline version is named: {}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("Nothing got worse"),
        "a comparison that cannot be trusted must not report a clean bill of \
         health: {}",
        outcome.text
    );
}

#[test]
fn a_baseline_predating_the_version_field_is_refused_too() {
    let dir = scratch("baseline-no-version");
    let packs = packs_dir(&dir, &[PACK]);

    // What a pre-#84 harness wrote: reports, and nothing about itself.
    let path = dir.join("ancient.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "reports": [report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")],
        }))
        .expect("serialise an old-shaped baseline"),
    )
    .expect("write it");

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
        at("2026-07-21T09:30:00Z"),
    );

    assert_eq!(outcome.code, ExitCode::CouldNotRun, "{}", outcome.text);
}

#[test]
fn a_stale_baseline_is_compared_but_says_how_old_it_is() {
    let dir = scratch("baseline-stale");
    let packs = packs_dir(&dir, &[PACK]);
    let same = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");

    let path = dir.join("stale.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&same), at("2026-01-02T09:00:00Z")),
    )
    .expect("write a stale baseline");

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![same]),
        at("2026-07-21T09:30:00Z"),
    );

    // Age is not a regression — the scoring means the same thing, so
    // the comparison is honest. It is a fact worth putting in front of
    // whoever is reading a clean result.
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome.text.contains("200 days old"),
        "a stale baseline compares clean and says nothing about being old: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("Nothing got worse than the baseline"),
        "{}",
        outcome.text
    );
}

#[test]
fn a_fresh_baseline_is_not_nagged_about() {
    let dir = scratch("baseline-fresh");
    let packs = packs_dir(&dir, &[PACK]);
    let same = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");

    let path = dir.join("fresh.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&same), at("2026-07-20T09:00:00Z")),
    )
    .expect("write a fresh baseline");

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![same]),
        at("2026-07-21T09:30:00Z"),
    );

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        !outcome.text.contains("days old"),
        "a net that cries wolf gets switched off: {}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// #74: attributing a regression to the sidecar rather than the prompt

#[test]
fn a_regression_across_a_sidecar_upgrade_names_both_versions() {
    let dir = scratch("sidecar-upgrade");
    let packs = packs_dir(&dir, &[PACK]);

    let mut was = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    was.sidecar = Some(SidecarInfo {
        version: "10050 (b15ca938a)".to_owned(),
        file: "llama-server-macos-arm64".to_owned(),
        device: Some("MTL0 (Apple M1 Pro)".to_owned()),
    });
    let path = dir.join("baseline.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&was), at("2026-07-20T09:00:00Z")),
    )
    .expect("write baseline");

    // Same weights, same prompts, newer llama-server — and a worse score.
    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.sidecar = Some(SidecarInfo {
        version: "10199 (c0ffee123)".to_owned(),
        file: "llama-server-macos-arm64".to_owned(),
        device: Some("MTL0 (Apple M1 Pro)".to_owned()),
    });
    now.fixtures[0]
        .step_scores
        .insert("classify".into(), step(0.72));

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![now]),
        at("2026-07-21T09:30:00Z"),
    );

    assert_eq!(outcome.code, ExitCode::Regression, "{}", outcome.text);
    assert!(
        outcome.text.contains("10050 (b15ca938a)") && outcome.text.contains("10199 (c0ffee123)"),
        "the weights are identical and the sidecar is not — a regression report \
         that doesn't say so sends someone to re-read their prompt edits: {}",
        outcome.text
    );
}

#[test]
fn the_same_sidecar_is_not_worth_mentioning() {
    let dir = scratch("sidecar-same");
    let packs = packs_dir(&dir, &[PACK]);

    let sidecar = SidecarInfo {
        version: "10050 (b15ca938a)".to_owned(),
        file: "llama-server-macos-arm64".to_owned(),
        device: Some("MTL0 (Apple M1 Pro)".to_owned()),
    };
    let mut same = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    same.sidecar = Some(sidecar);

    let path = dir.join("baseline.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&same), at("2026-07-20T09:00:00Z")),
    )
    .expect("write baseline");

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![same]),
        at("2026-07-21T09:30:00Z"),
    );

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(!outcome.text.contains("llama-server"), "{}", outcome.text);
    assert!(
        !outcome.text.contains("device"),
        "the same device is not worth mentioning either: {}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// #490: the device that answered travels with the score

/// The same weights, the same scores, a different device — a note,
/// never a refusal. Whether two devices' scores are comparable at all
/// is `evals/RENTED-GPU.md`'s open question; the comparison's job is
/// only to make sure nobody reads a cross-device hold as a same-
/// instrument one without being told.
#[test]
fn a_comparison_across_devices_says_so_without_refusing() {
    let dir = scratch("device-differs");
    let packs = packs_dir(&dir, &[PACK]);

    let mut was = report(PACK, "qwen3.5-4b-q4_k_m.gguf");
    was.sidecar = Some(SidecarInfo {
        version: "10145 (ad256ded3)".to_owned(),
        file: "llama-server".to_owned(),
        device: Some("MTL0 (Apple M1 Pro)".to_owned()),
    });
    let path = dir.join("baseline.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&was), at("2026-08-11T09:00:00Z")),
    )
    .expect("write baseline");

    let mut now = was.clone();
    now.sidecar = Some(SidecarInfo {
        version: "10145 (ad256ded3)".to_owned(),
        file: "llama-server".to_owned(),
        device: Some("CUDA0 (NVIDIA GeForce RTX 5090)".to_owned()),
    });

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![now]),
        at("2026-08-12T09:30:00Z"),
    );

    assert_eq!(
        outcome.code,
        ExitCode::Ok,
        "a device change alone is never a regression or a refusal: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("MTL0 (Apple M1 Pro)")
            && outcome.text.contains("CUDA0 (NVIDIA GeForce RTX 5090)"),
        "both devices are named, so a held score is read as cross-device: {}",
        outcome.text
    );
}

/// A baseline from before the device was recorded compares with a note,
/// exactly as a pre-bed or pre-runtime report does — refusing would
/// retire every baseline on disk for a property none of them could
/// have carried.
#[test]
fn a_baseline_without_a_device_is_compared_with_a_note() {
    let dir = scratch("device-unrecorded");
    let packs = packs_dir(&dir, &[PACK]);

    let mut was = report(PACK, "qwen3.5-4b-q4_k_m.gguf");
    was.sidecar = Some(SidecarInfo {
        version: "10145 (ad256ded3)".to_owned(),
        file: "llama-server".to_owned(),
        device: None,
    });
    let path = dir.join("baseline.json");
    std::fs::write(
        &path,
        eval::baseline::to_json(std::slice::from_ref(&was), at("2026-08-11T09:00:00Z")),
    )
    .expect("write baseline");

    let mut now = was.clone();
    now.sidecar = Some(SidecarInfo {
        version: "10145 (ad256ded3)".to_owned(),
        file: "llama-server".to_owned(),
        device: Some("CUDA0 (NVIDIA GeForce RTX 5090)".to_owned()),
    });

    let mut options = options(&packs, PACK);
    options.baseline = Some(path);

    let outcome = eval::run(
        &options,
        &Canned::new(vec![now]),
        at("2026-08-12T09:30:00Z"),
    );

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(
        outcome.text.contains("does not say which device"),
        "an unrecorded device is said out loud, never assumed silently: {}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// #83: --runs has somewhere to record variance

/// Three runs where normalise wobbled on the one fixture, and the
/// baseline the command wrote for them.
fn wrote_wobbly_baseline(name: &str, runs: u32) -> serde_json::Value {
    let dir = scratch(name);
    let packs = packs_dir(&dir, &[PACK]);

    let good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let mut wobbly = good.clone();
    wobbly.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.71));

    let mut repeats = vec![good.clone(), wobbly, good];
    repeats.truncate(runs as usize);

    let mut options = options(&packs, PACK);
    options.runs = runs;
    let path = dir.join("written.json");
    options.write_baseline = Some(path.clone());

    let outcome = run_at(&options, &Canned::new(repeats));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    serde_json::from_str(&std::fs::read_to_string(&path).expect("read what was written"))
        .expect("it is JSON")
}

#[test]
fn repeats_record_the_spread_they_found() {
    let written = wrote_wobbly_baseline("stability-recorded", 3);
    let stability = &written["reports"][0]["fixtures"][0]["stability"];

    assert_eq!(stability["runs"], 3);
    // Not the mean of 0.88, 0.71, 0.88 — the ends, which is the only
    // shape that can tell this run set apart from a steady 0.82.
    assert_eq!(stability["steps"]["normalise"]["low"], 0.71);
    assert_eq!(stability["steps"]["normalise"]["high"], 0.88);
    // classify held across all three, and says so rather than being
    // left out: "we checked and it did not move" is the finding.
    assert_eq!(stability["steps"]["classify"]["low"], 0.91);
    assert_eq!(stability["steps"]["classify"]["high"], 0.91);
}

#[test]
fn one_run_has_nothing_to_be_stable_about() {
    let written = wrote_wobbly_baseline("stability-single", 1);

    assert!(
        written["reports"][0]["fixtures"][0]["stability"].is_null(),
        "a single run cannot disagree with itself, and an empty spread \
         beside every score would read as a stability claim nobody made: {written}"
    );
}

#[test]
fn the_table_marks_a_score_that_moved() {
    let dir = scratch("stability-table");
    let packs = packs_dir(&dir, &[PACK]);

    let good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let mut wobbly = good.clone();
    wobbly.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.71));

    let mut options = options(&packs, PACK);
    options.runs = 3;

    let outcome = run_at(&options, &Canned::new(vec![good.clone(), wobbly, good]));

    let table: Vec<&str> = outcome.text.lines().collect();
    let row = table
        .iter()
        .find(|line| line.starts_with("qwen2.5-3b"))
        .unwrap_or_else(|| panic!("a row for the model: {}", outcome.text));
    assert!(
        row.contains("0.72 (n=50; 95% CI 0.58–0.83) ⚠"),
        "the number that moved should be marked where it is read, not only in \
         a warning underneath it: {row}"
    );
    // classify held, so it is not marked — a table where everything
    // carries a warning is a table nobody reads.
    assert!(!row.contains("0.92 (n=50; 95% CI 0.81–0.97) ⚠"), "{row}");
}

#[test]
fn a_steady_table_carries_no_markers() {
    let dir = scratch("stability-steady-table");
    let packs = packs_dir(&dir, &[PACK]);

    let steady = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");

    let mut options = options(&packs, PACK);
    options.runs = 3;

    let outcome = run_at(
        &options,
        &Canned::new(vec![steady.clone(), steady.clone(), steady]),
    );

    assert!(!outcome.text.contains('⚠'), "{}", outcome.text);
}

#[test]
fn a_spread_that_appeared_is_not_a_regression() {
    let dir = scratch("stability-not-regression");
    let packs = packs_dir(&dir, &[PACK]);

    // The baseline was one steady run. Now three runs, wobbling — but
    // the worst of them scores exactly what the baseline did.
    let steady = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let baseline = write_baseline(&dir, std::slice::from_ref(&steady));

    let mut better = steady.clone();
    better.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.93));

    let mut options = options(&packs, PACK);
    options.runs = 3;
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![better.clone(), steady, better]));

    // Verdict is three-way and stays that way (decision #52): the run
    // is unstable, which is a fault to chase, but nothing scored worse
    // than the baseline said it would.
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert!(outcome.text.contains("Warning"), "{}", outcome.text);
}

/// The worst run has to be worst on the scores too, not only on the
/// verdict and the end result.
///
/// A repeat can score lower on a step while still passing its bar and
/// still ending up at the same end-to-end number — the step bar is
/// 0.85, so 0.71 and 0.88 can both sit under a `PASS`. Reporting the
/// kinder of those two is the same averaging-away this issue exists to
/// stop, one run later.
#[test]
fn the_worst_run_is_the_one_with_the_worst_scores() {
    let dir = scratch("worst-by-score");
    let packs = packs_dir(&dir, &[PACK]);

    let good = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    let mut wobbly = good.clone();
    wobbly.fixtures[0]
        .step_scores
        .insert("normalise".into(), step(0.71));
    // Same verdict, same end result — only the step score is worse.
    assert_eq!(wobbly.verdict, good.verdict);
    assert_eq!(wobbly.fixtures[0].end_to_end, good.fixtures[0].end_to_end);

    let mut options = options(&packs, PACK);
    options.runs = 3;

    let outcome = run_at(&options, &Canned::new(vec![good.clone(), wobbly, good]));

    let row = outcome
        .text
        .lines()
        .find(|line| line.starts_with("qwen2.5-3b"))
        .unwrap_or_else(|| panic!("a row for the model: {}", outcome.text))
        .to_owned();
    assert!(
        row.contains("0.72 (n=50; 95% CI 0.58–0.83)"),
        "the reported run should be the one that did worst, not whichever \
         repeat happened to be first: {row}"
    );
}

/// A merged file cannot carry one `scoring_version` for everything in
/// it, because its whole purpose is to hold measurements taken at
/// different times on machines that are not all to hand.
///
/// The field exists so the app can refuse numbers that are no longer
/// comparable (#84). A single header cannot keep that promise once
/// entries accumulate: re-measuring on one machine would relabel every
/// other machine's numbers as having been scored under a version they
/// were never scored under — and the honesty-check machines (brief §5)
/// are exactly the ones that cannot simply be re-measured on demand.
#[test]
fn an_entry_keeps_the_provenance_it_was_measured_under() {
    let dir = scratch("tiers-provenance");
    let packs = packs_dir(&dir, &[PACK]);

    // A measurement taken on a machine that is now in a drawer, under an
    // older pack and older scoring.
    let existing = serde_json::json!({
        "pack": PACK,
        "tiers": [{
            "model": { "file": "qwen2.5-1.5b-instruct-q4_k_m.gguf", "params": "1.5B",
                       "quant": "Q4_K_M", "context": 8192 },
            "machine": { "cpu": "Cortex-A76", "ram_gb": 8, "os": "Pi OS 12" },
            "pack_version": "0.9.0",
            "scoring_version": 0,
            "measured_at": "2026-01-02T09:00:00Z",
            "verdict": "fail",
            "automatic": 0.31,
            "wall_ms": 900_000,
            "steps": { "normalise": 0.55, "classify": 0.42 },
            "end_to_end": 0.4,
            "runs": 3,
            "steady": false
        }]
    });
    let path = packs.join(PACK).join("tiers.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&existing).expect("json"),
    )
    .expect("write an existing tiers.json");

    let mut options = options(&packs, PACK);
    options.write_tiers = true;
    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let tiers = written["tiers"].as_array().expect("tiers");
    assert_eq!(
        tiers.len(),
        2,
        "both machines are still recorded: {written}"
    );

    let pi = tiers
        .iter()
        .find(|tier| tier["machine"]["cpu"] == "Cortex-A76")
        .expect("the machine in the drawer");
    assert_eq!(
        pi["scoring_version"], 0,
        "re-measuring elsewhere must not relabel numbers it did not score: {written}"
    );
    assert_eq!(pi["pack_version"], "0.9.0", "{written}");

    let fresh = tiers
        .iter()
        .find(|tier| tier["machine"]["cpu"] == "Apple M1")
        .expect("the machine that was just measured");
    assert_eq!(
        fresh["scoring_version"],
        serde_json::json!(cli::eval::baseline::SCORING_VERSION)
    );
    assert_eq!(fresh["pack_version"], "1.0.0");

    // And nothing at the top level claims to speak for both.
    assert!(
        written["scoring_version"].is_null() && written["pack_version"].is_null(),
        "a file-level version is a claim about entries it cannot check: {written}"
    );
}

// ---------------------------------------------------------------------------
// #73: the no-model floor
//
// CONTRACT (#73): these tests are the specification. If one seems wrong,
// stop and report it rather than editing it — a reported defect in the
// contract is a good outcome.

/// What the floor's report looks like: same shape as any other, and
/// honest about there having been no model.
fn floor_report(pack: &str) -> EvalReport {
    let mut floor = report(pack, "ignored");
    floor.model = None;
    // The floor classifies nothing: everything goes to a person.
    floor.fixtures[0]
        .step_scores
        .insert("classify".into(), step(0.0));
    floor.fixtures[0].end_to_end = 0.0;
    floor.fixtures[0].needs_review_rate = 1.0;
    floor.verdict = Verdict::Fail;
    floor
}

/// `--no-model` needs no weights named, asks the evaluator for the
/// floor, and a FAIL verdict exits 0 — measuring the floor is the
/// point, not a problem.
#[test]
fn no_model_measures_the_floor_without_naming_weights() {
    let dir = scratch("no-model-flag");
    let packs = packs_dir(&dir, &[PACK]);

    let mut options = options(&packs, PACK);
    options.model = None; // no --model,
    options.models = None; // no --models,
    options.no_model = true; // and no complaint about either.

    let canned = Canned::new(vec![floor_report(PACK)]);
    let outcome = run_at(&options, &canned);

    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);
    assert_eq!(canned.asked().len(), 1);
    assert!(
        canned.asked()[0].contains("without a model"),
        "the evaluator was asked for the floor, not for weights: {:?}",
        canned.asked()
    );
    // The table row names the measurement in plain words.
    assert!(outcome.text.contains("without a model"), "{}", outcome.text);
    assert!(outcome.text.contains("FAIL"), "{}", outcome.text);
}

/// The floor lands in tiers.json under `baseline`, never under `tiers`
/// — a floor is not a tier, and the model-manager screen must not be
/// able to offer it for install by iterating tiers. Tiers already
/// recorded survive, and the floor merges machine by machine like
/// everything else.
#[test]
fn the_floor_lands_under_baseline_in_tiers_json() {
    let dir = scratch("no-model-tiers");
    let packs = packs_dir(&dir, &[PACK]);

    // A real tier is already on file.
    let mut options = options(&packs, PACK);
    options.write_tiers = true;
    let outcome = run_at(
        &options,
        &Canned::new(vec![report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf")]),
    );
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    // Now the floor is measured on the same machine.
    let mut options = options_for_floor(&packs);
    options.write_tiers = true;
    let outcome = run_at(&options, &Canned::new(vec![floor_report(PACK)]));
    assert_eq!(outcome.code, ExitCode::Ok, "{}", outcome.text);

    let path = packs.join(PACK).join("tiers.json");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");

    let tiers = written["tiers"].as_array().expect("tiers");
    assert_eq!(
        tiers.len(),
        1,
        "the real tier survives untouched: {written}"
    );
    assert_eq!(tiers[0]["model"]["file"], "qwen2.5-3b-instruct-q4_k_m.gguf");

    let baseline = written["baseline"].as_array().expect("a baseline array");
    assert_eq!(baseline.len(), 1, "{written}");
    assert!(
        baseline[0]["model"].is_null(),
        "the floor names no model: {written}"
    );
    assert_eq!(baseline[0]["machine"]["cpu"], "Apple M1");
    assert_eq!(baseline[0]["verdict"], "fail");
    assert_eq!(baseline[0]["automatic"], 0.0, "{written}");

    // Measured again on the same machine, it replaces itself rather
    // than accumulating.
    let mut options = options_for_floor(&packs);
    options.write_tiers = true;
    run_at(&options, &Canned::new(vec![floor_report(PACK)]));
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(
        written["baseline"].as_array().expect("baseline").len(),
        1,
        "one floor per machine: {written}"
    );
}

/// `options()` with the floor asked for instead of a model.
fn options_for_floor(packs_dir: &Path) -> Options {
    let mut options = options(packs_dir, PACK);
    options.model = None;
    options.no_model = true;
    options
}

// ---------------------------------------------------------------------------
// #320: a measurement must say which bed it ran against

/// A recording records what answered and what the numbers mean, and
/// used to record nothing about what was *asked*.
///
/// Neither existing guard covers this. A fixture-only change must not
/// bump the pack version — #319 rewrote 154 exam fixtures and left the
/// development bed byte for byte identical, and bumping would have
/// retired every valid development measurement for a change that could
/// not affect them. `SCORING_VERSION` is equally correctly scoped: what
/// a score *means* did not move either.
///
/// So the bed could be rewritten under a baseline and the comparison
/// would report a drop or a hold, both readings wrong in a way the exit
/// code could not express. Refused rather than compared, because a
/// comparison that cannot be trusted should not be reported as a number.
#[test]
fn a_measurement_from_another_bed_is_refused_rather_than_compared() {
    let dir = scratch("baseline-other-bed");
    let packs = packs_dir(&dir, &[PACK]);
    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before.bed = Some("blake3:aaaa".to_owned());
    let baseline = write_baseline(&dir, &[before]);

    // The same model, the same scores — and a different set of
    // questions behind them.
    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.bed = Some("blake3:bbbb".to_owned());

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::CouldNotRun,
        "a comparison across two beds is refused, not scored: {}",
        outcome.text
    );
    assert_eq!(outcome.code.as_i32(), 2, "{}", outcome.text);
    assert!(
        outcome.text.contains("bed"),
        "the refusal says what it could not compare: {}",
        outcome.text
    );
}

/// The judgement a naive implementation gets wrong.
///
/// #319 is the worked example: it left the development bed untouched
/// and rewrote the exam's. A pack-wide digest would have retired every
/// development measurement for a change that could not have reached
/// them — punishing the honest, cheap fix and making a fixture-only
/// change look expensive. The digest is per eval set, as `eval_set`
/// already is.
#[test]
fn a_change_to_one_set_does_not_retire_the_other_sets_measurements() {
    let dir = scratch("baseline-one-set-only");
    let packs = packs_dir(&dir, &[PACK]);
    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before.eval_set = runner::eval::fixture::EvalSet::Development;
    before.bed = Some("blake3:development-bed".to_owned());
    let baseline = write_baseline(&dir, &[before]);

    // The exam bed was rewritten; this development measurement stands.
    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.eval_set = runner::eval::fixture::EvalSet::Development;
    now.bed = Some("blake3:development-bed".to_owned());

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::Ok,
        "an untouched set's measurement still compares: {}",
        outcome.text
    );
}

/// A baseline recorded before beds were identified cannot say which one
/// it ran against, and must not be treated as though it matched.
///
/// It is compared, with a note — the same shape as a report from before
/// `sidecar` was recorded. Refusing outright would retire every
/// baseline on disk the day this shipped, for a property none of them
/// could have carried.
#[test]
fn a_baseline_from_before_beds_were_identified_is_compared_with_a_note() {
    let dir = scratch("baseline-no-bed");
    let packs = packs_dir(&dir, &[PACK]);
    let before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(before.bed, None, "the fixture predates bed identity");
    let baseline = write_baseline(&dir, &[before]);

    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.bed = Some("blake3:cccc".to_owned());

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::Ok,
        "an older baseline is still usable: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("bed"),
        "but it says the comparison is standing on less than it looks: {}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// #232: a measurement must say the runtime policy it ran under

/// The runtime policy — context, reasoning, answer bound — can move a
/// score and a wall time without touching the weights, the prompt, the
/// bed or the scoring: leaving reasoning at llama-server's `auto` cost
/// Gemma 4 ten minutes on two fixtures where Qwen2.5 7B took 21.7s.
/// None of the existing guards sees that change, so a baseline measured
/// under one policy compared against a run under another would report
/// movement that is only the policy changing. Refused, exactly as a bed
/// change is (#320): a number that cannot be trusted must not be
/// printed as one.
#[test]
fn a_measurement_under_another_runtime_policy_is_refused_rather_than_compared() {
    let dir = scratch("baseline-other-runtime");
    let packs = packs_dir(&dir, &[PACK]);
    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before.runtime = Some(RuntimePolicy {
        context: 8192,
        parallel: 1,
        reasoning: Reasoning::Auto,
        max_answer_tokens: 4096,
    });
    let baseline = write_baseline(&dir, &[before]);

    // The same model, the same scores — measured with reasoning chosen
    // instead of left to the server.
    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.runtime = Some(RuntimePolicy {
        context: 8192,
        parallel: 1,
        reasoning: Reasoning::Off,
        max_answer_tokens: 4096,
    });

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::CouldNotRun,
        "a comparison across two runtime policies is refused, not scored: {}",
        outcome.text
    );
    assert_eq!(outcome.code.as_i32(), 2, "{}", outcome.text);
    assert!(
        outcome.text.contains("runtime policy"),
        "the refusal says what it could not compare: {}",
        outcome.text
    );
    // Both sides of the drift are named, so the reader does not have to
    // open two JSON files to learn what moved.
    assert!(
        outcome.text.contains("reasoning auto") && outcome.text.contains("reasoning off"),
        "the refusal names both policies: {}",
        outcome.text
    );
}

/// A baseline recorded before the policy was recorded cannot say what
/// it ran under, and must not be treated as though it matched. Compared
/// with a note — the same shape as a pre-bed baseline (#320): refusing
/// outright would retire every baseline on disk the day this shipped,
/// for a property none of them could have carried.
#[test]
fn a_baseline_predating_runtime_policy_is_compared_with_a_note() {
    let dir = scratch("baseline-no-runtime");
    let packs = packs_dir(&dir, &[PACK]);
    let before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    assert_eq!(before.runtime, None, "the fixture predates recorded policy");
    let baseline = write_baseline(&dir, &[before]);

    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.runtime = Some(RuntimePolicy {
        context: 8192,
        parallel: 1,
        reasoning: Reasoning::Off,
        max_answer_tokens: 4096,
    });

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::Ok,
        "an older baseline is still usable: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("runtime policy"),
        "but it says the comparison is standing on less than it looks: {}",
        outcome.text
    );
}

/// Two measurements under the same recorded policy compare exactly as
/// before — the guard bites on drift, never on agreement.
#[test]
fn the_same_runtime_policy_on_both_sides_compares_cleanly() {
    let dir = scratch("baseline-same-runtime");
    let packs = packs_dir(&dir, &[PACK]);
    let policy = RuntimePolicy {
        context: 8192,
        parallel: 1,
        reasoning: Reasoning::Off,
        max_answer_tokens: 4096,
    };
    let mut before = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    before.runtime = Some(policy.clone());
    let baseline = write_baseline(&dir, &[before]);

    let mut now = report(PACK, "qwen2.5-3b-instruct-q4_k_m.gguf");
    now.runtime = Some(policy);

    let mut options = options(&packs, PACK);
    options.baseline = Some(baseline);

    let outcome = run_at(&options, &Canned::new(vec![now]));

    assert_eq!(
        outcome.code,
        ExitCode::Ok,
        "matching policies must not trip the guard: {}",
        outcome.text
    );
    assert!(
        !outcome.text.contains("runtime policy"),
        "nothing to say when the policy held: {}",
        outcome.text
    );
}

// ---------------------------------------------------------------------------
// #293: where a measurement's own artefacts live

/// A recording is expensive and irreplaceable; `target/` is neither.
///
/// `runs_dir` defaulted to `target/eval-runs`, which is build output —
/// `cargo clean` is entitled to delete it and nothing warns anybody. A
/// worktree under `/private/tmp` can lose it to OS temp purging too.
///
/// That mattered less when a run directory was only a diagnostic
/// record. Since replay (#288/#289) it is the artefact that turns a
/// 115-minute measurement into a 29-second rescore — so the expensive
/// thing was living in the one directory licensed to delete it, while
/// `evals/`, version-controlled and durable, held only the baselines
/// that explain it.
#[test]
fn a_recording_does_not_default_into_build_output() {
    let runs = eval::default_runs_dir();

    assert!(
        !runs.starts_with("target"),
        "a recording must not default where `cargo clean` may delete it: {}",
        runs.display()
    );
    // Beside the baselines it explains, so there is one place to look.
    assert!(
        runs.starts_with("evals"),
        "a recording belongs with the baselines: {}",
        runs.display()
    );
}

/// Resume state is the same class of artefact and had the same defect.
///
/// It is what an interrupted eval continues from, so losing it costs
/// the hours the eval had already spent — the loss this issue is about,
/// with a different name on it. Not asked for in #293; moved because
/// leaving it would be knowingly leaving the trap half-set.
#[test]
fn resume_state_does_not_default_into_build_output_either() {
    let resume = eval::default_resume_dir();

    assert!(
        !resume.starts_with("target"),
        "resume state is hours of measuring, not build output: {}",
        resume.display()
    );
}

/// The sidecar's own log stays in `target/`, deliberately.
///
/// It is the model server's stderr, regenerable by running again, and
/// it explains nothing a report cannot say for itself. The line this
/// draws is what the rest of the change rests on: expensive or
/// irreplaceable artefacts leave `target/`, genuinely disposable output
/// stays. Asserted so the distinction is a decision on the record
/// rather than an oversight anyone has to reconstruct.
#[test]
fn a_regenerable_log_may_stay_in_build_output() {
    assert!(eval::default_log_dir().starts_with("target"));
}
