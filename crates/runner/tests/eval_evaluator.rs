//! The eval harness end to end (#25): a pack, its fixtures, a mock
//! model, a scored `EvalReport`. The half that needs a model, driven
//! without one — CI downloads no weights (CLAUDE.md).

mod support;

use runner::eval::fixture::{model_info, EvalSelection, FixtureEvaluator};
use runner::eval::{EvalMetric, MachineInfo, MetricReport, SidecarInfo, Verdict};
use runner::kinds::KindFrom;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
}

/// The llama-server that answered. #74: pinned weights don't pin
/// sampling behaviour, so the binary travels with the score.
fn sidecar() -> SidecarInfo {
    SidecarInfo {
        version: "10050 (b15ca938a)".to_owned(),
        file: "llama-server-macos-arm64".to_owned(),
        device: Some("MTL0 (Apple M1 Pro)".to_owned()),
    }
}

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// statement-01's six merchants, answered exactly as its
/// `expected.json` says a good model would.
fn statement_01_normalise() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "raw": "PUREGYM LTD", "name": "PureGym", "recognised": true},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "recognised": true},
        {"id": 2, "raw": "SPOTIFY LTD", "name": "Spotify", "recognised": true},
        {"id": 3, "raw": "BRITISH GAS", "name": "British Gas", "recognised": true},
        {"id": 4, "raw": "TESCO STORES 3412", "name": "Tesco", "recognised": true},
        {"id": 5, "raw": "ACME PAYROLL", "name": "Acme Payroll", "recognised": true}
    ]}"#,
    )
}

fn statement_01_classify() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "name": "PureGym", "kind": "subscription", "category": "fitness", "confidence": "high"},
        {"id": 1, "name": "Netflix", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 2, "name": "Spotify", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 3, "name": "British Gas", "kind": "utility", "category": "energy", "confidence": "high"},
        {"id": 4, "name": "Tesco", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
        {"id": 5, "name": "Acme Payroll", "kind": "regular_spend", "category": "other", "confidence": "high"}
    ]}"#,
    )
}

fn statement_02_normalise() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "raw": "DISNEYPLUS", "name": "Disney+", "recognised": true},
        {"id": 1, "raw": "AMAZON PRIME", "name": "Amazon Prime", "recognised": true},
        {"id": 2, "raw": "KAFFA COFFEE", "name": "Kaffa Coffee", "recognised": true},
        {"id": 3, "raw": "Amazon Marketplace", "name": "Amazon Marketplace", "recognised": true},
        {"id": 4, "raw": "COFFEECART", "name": "Coffee Cart", "recognised": true}
    ]}"#,
    )
}

fn statement_02_classify() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "name": "Disney+", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 1, "name": "Amazon Prime", "kind": "subscription", "category": "retail", "confidence": "high"},
        {"id": 2, "name": "Kaffa Coffee", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
        {"id": 3, "name": "Amazon Marketplace", "kind": "one_off", "category": "retail", "confidence": "high"},
        {"id": 4, "name": "Coffee Cart", "kind": "regular_spend", "category": "food_drink", "confidence": "high"}
    ]}"#,
    )
}

/// #237 Stage 1's first failing test. A scored classification is a
/// durable record, not merely one contribution to an aggregate: it
/// carries the authored identity and strata, what was expected, what
/// the run decided (including review as its own outcome), the exact
/// exchanges that produced that decision, and the pack/prompt versions
/// needed to compare it honestly later.
#[test]
fn scored_classifications_are_emitted_as_diffable_items() {
    let fixtures = original_two_fixtures("eval-diffable-item-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    assert_eq!(
        pack.thresholds().step("classify"),
        None,
        "the retired classification-accuracy threshold must not gate the pack"
    );
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator.evaluate(&pack).expect("the eval runs");
    let MetricReport::Classification(metrics) = &report.metrics[&EvalMetric::Classification] else {
        panic!("an audit pack reports classification metrics");
    };
    assert_eq!(metrics.overall.n, 10);
    assert_eq!(metrics.strata["clean"].n, 5);
    assert_eq!(metrics.strata["messy-merchant-strings"].n, 5);

    let first = &report.fixtures[0];
    let puregym = first
        .items
        .iter()
        .find(|item| item.item_id == "monthly-fitness-membership-01")
        .expect("PureGym is a scored item");
    let (expected, actual) = puregym
        .decision
        .as_classification()
        .expect("a classification decision");

    assert_eq!(
        puregym.id,
        "app.kttl.subscription-audit/clean-everyday-01/monthly-fitness-membership-01"
    );
    assert_eq!(puregym.fixture, "statement-01.csv");
    assert_eq!(puregym.fixture_id, "clean-everyday-01");
    assert_eq!(puregym.strata, ["clean"]);
    assert_eq!(puregym.pack_version, "1.5.0");
    assert!(
        puregym.prompt_version.starts_with("blake3:"),
        "{}",
        puregym.prompt_version
    );
    assert_eq!(expected.kind, "subscription");
    assert_eq!(expected.category, "fitness");
    assert_eq!(
        actual.classification().expect("classified").kind,
        "subscription"
    );
    assert!(
        puregym
            .exchanges
            .iter()
            .any(|exchange| exchange.request.contains("PureGym")
                && exchange.response.contains("\"kind\":\"subscription\"")),
        "the item keeps the raw model exchange that produced it: {puregym:#?}"
    );

    assert_eq!(
        first.step_scores.get("classify"),
        None,
        "classification must not survive as an accuracy aggregate beside per-class precision and recall"
    );
}

/// #272: a wrong answer must say which decision produced it.
///
/// The #237 confident-wrong cell was attributed by hand, twice, and
/// wrongly both times — because the path was inferred from the
/// resulting `kind`, and two different branches emit the same string.
/// `regular_spend` is what a detected series in a `food_drink`-shaped
/// category maps to *and* what three or more scattered debit days come
/// to; reading the kind cannot tell those apart, and nothing else was
/// written down.
#[test]
fn a_scored_item_says_which_decision_produced_its_kind() {
    let fixtures = original_two_fixtures("eval-kind-provenance-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator.evaluate(&pack).expect("the eval runs");
    let items = &report.fixtures[0].items;
    let path_of = |item_id: &str| {
        items
            .iter()
            .find(|item| item.item_id == item_id)
            .unwrap_or_else(|| panic!("{item_id} is a scored item"))
            .decision
            .kind_from()
    };

    // PureGym pays monthly: a series was detected, so the pack's
    // category→kind map decided what it is.
    assert_eq!(
        path_of("monthly-fitness-membership-01"),
        Some(KindFrom::CategoryMap),
    );
    // Tesco is the weekly shop: no series, so the kind came from
    // counting distinct debit days — cadence, not category.
    assert_eq!(
        path_of("recurring-grocery-shop-01"),
        Some(KindFrom::Cadence),
    );
    assert_ne!(
        path_of("monthly-fitness-membership-01"),
        path_of("recurring-grocery-shop-01"),
        "the two paths must be distinguishable without reading the kind"
    );
}

#[test]
fn scored_item_metrics_must_be_declared_by_the_pack() {
    let mut pack = load_pack(&pack_dir()).expect("pack loads");
    pack.manifest.eval_metrics.clear();
    let evaluator = FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let error = evaluator
        .evaluate(&pack)
        .expect_err("undeclared scored shapes must be refused");

    assert!(error.contains("classification"), "{error}");
    assert!(error.contains("eval_metrics"), "{error}");
}

#[test]
fn review_cost_must_have_pack_provenance_before_it_is_reported() {
    let mut pack = load_pack(&pack_dir()).expect("pack loads");
    pack.manifest.eval_costs.clear();
    let evaluator = FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let error = evaluator
        .evaluate(&pack)
        .expect_err("an unexplained cost metric must be refused");

    assert!(error.contains("review_rate"), "{error}");
    assert!(error.contains("eval_costs"), "{error}");
}

/// The scored record is the joint system's decision (#253, scoring
/// v5): kind is derived — income detection writes `kind: income`, and
/// no model answer can overrule the credits — while category remains
/// the model's. The old assertion here was the inverse (keep the
/// model's kind even where aggregation knew better), and it retired
/// with the model's right to answer kind at all. The raw exchange
/// still carries everything the model said, so nothing is lost — it
/// just stopped being the decision.
#[test]
fn item_records_carry_the_derived_kind_after_aggregation() {
    let fixtures = original_two_fixtures("eval-pre-aggregate-item-fixtures");
    let expected_path = fixtures.0.join("statement-01.expected.json");
    let mut expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&expected_path).expect("read copied expectations"),
    )
    .expect("expectations are JSON");
    expected["classify"]
        .as_array_mut()
        .expect("classification expectations")
        .push(serde_json::json!({
            "id": "monthly-payroll-income-01",
            "strata": ["clean"],
            "name": "Acme Payroll",
            "kind": "regular_spend",
            "category": "other"
        }));
    std::fs::write(
        expected_path,
        serde_json::to_string_pretty(&expected).expect("serialise expectations"),
    )
    .expect("write copied expectations");

    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);
    let report = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    }
    .evaluate(&pack)
    .expect("the eval runs");

    let payroll = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "monthly-payroll-income-01")
        .expect("payroll item");
    assert_eq!(
        payroll
            .decision
            .as_classification()
            .expect("a classification decision")
            .1
            .classification()
            .expect("classified")
            .kind,
        "income",
        "twelve credits are income whatever the model calls the name"
    );
}

fn with_timings(
    completion: String,
    prompt_ms: f64,
    predicted_ms: f64,
    predicted_per_second: f64,
) -> String {
    let mut envelope: serde_json::Value =
        serde_json::from_str(&completion).expect("completion envelope is JSON");
    envelope["timings"] = serde_json::json!({
        "prompt_ms": prompt_ms,
        "predicted_ms": predicted_ms,
        "predicted_per_second": predicted_per_second
    });
    envelope.to_string()
}

#[test]
fn perf_records_what_the_model_actually_cost() {
    let fixtures = original_two_fixtures("eval-perf-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            with_timings(statement_01_normalise(), 1.0, 2.0, 10.0),
        ),
        (
            "200 OK",
            with_timings(statement_01_classify(), 2.0, 5.0, 30.0),
        ),
        (
            "200 OK",
            with_timings(statement_02_normalise(), 1.0, 2.0, 20.0),
        ),
        (
            "200 OK",
            with_timings(statement_02_classify(), 2.0, 5.0, 20.0),
        ),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator.evaluate(&pack).expect("the eval runs");
    let perf = report.fixtures[0]
        .perf
        .as_ref()
        .expect("a receipt carries telemetry");

    assert_eq!(perf.model_ms, 10);
    let generation_weighted_rate = (10.0 * 2.0 + 30.0 * 5.0) / (2.0 + 5.0);
    assert!(
        (perf.tokens_per_second - generation_weighted_rate).abs() < 0.0001,
        "{perf:?}"
    );
    assert_eq!(report.fixtures[0].retries, 0);
    assert!(perf.wall_ms >= perf.model_ms, "{perf:?}");
    assert_eq!(report.fixtures[1].perf.as_ref().unwrap().model_ms, 10);
    assert_eq!(
        report.fixtures[1].perf.as_ref().unwrap().tokens_per_second,
        20.0
    );
}

/// A model that answers exactly as `expected.json` says it should
/// passes each selected fixture — and the report says which weights,
/// which pack and which machine produced that claim. A custom subset
/// cannot certify the whole stratified pack.
#[test]
fn a_model_answering_as_expected_passes_its_fixtures_not_unseen_pack_strata() {
    let fixtures = original_two_fixtures("eval-pass-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator.evaluate(&pack).expect("the eval runs");

    assert_eq!(report.pack, "app.kttl.subscription-audit");
    assert_eq!(report.pack_version, "1.5.0");
    assert_eq!(report.eval_set, EvalSelection::Development);
    let model = report.model.as_ref().expect("a measured model is named");
    assert_eq!(model.params, "7B");
    assert_eq!(report.machine.ram_gb, 16);
    // #74: which llama-server produced this, so a score that moves
    // after a sidecar upgrade can be attributed to the upgrade rather
    // than to the last prompt edit.
    let sidecar = report
        .sidecar
        .as_ref()
        .expect("the report names its sidecar");
    assert_eq!(sidecar.version, "10050 (b15ca938a)");
    assert!(!sidecar.version.is_empty());

    let first = &report.fixtures[0];
    assert_eq!(first.fixture, "statement-01.csv");
    assert_eq!(first.step_scores["normalise"].correct, 6);
    assert!(!first.step_scores.contains_key("classify"));
    assert_eq!(first.end_to_end, 1.0);
    assert_eq!(first.needs_review_rate, 0.0);
    assert_eq!(first.verdict(&pack.thresholds()), Verdict::Pass);
    assert_eq!(
        first.perf.as_ref().unwrap().model_ms,
        0,
        "a server without timings must not produce a partial estimate"
    );
    assert_eq!(first.perf.as_ref().unwrap().tokens_per_second, 0.0);

    // Both selected fixtures pass. They are still only ten decisions:
    // claiming the full pack passed would silently skip the other
    // declared strata and their 5% Wilson ceilings.
    assert_eq!(report.fixtures.len(), 2);
    assert_eq!(report.fixtures[1].end_to_end, 1.0);
    assert_eq!(
        report.fixtures[1].verdict(&pack.thresholds()),
        Verdict::Pass
    );
    assert_eq!(report.verdict, Verdict::Fail);
    let MetricReport::Classification(metrics) = &report.metrics[&EvalMetric::Classification] else {
        panic!("an audit pack reports classification metrics");
    };
    assert!(!metrics.gates["annual-subscription-once-yearly"]
        [&runner::eval::HarmClass::Subscription]
        .outcome
        .clears());
}

/// #490: a measurement records the device the sidecar answered on, not
/// the device the host has installed.
///
/// `evals/history/baseline-v13-letter.json` was recorded on a rented RTX 5090
/// and names only a Xeon — every word true, none of it saying a GPU was
/// involved. The host's inventory is the wrong fact: a CUDA build and a
/// CPU-only build on the same box (`scripts/vendor-sidecar.sh` falls
/// back to one when `nvcc` is absent) would produce byte-identical
/// provenance while measuring different instruments. The fact worth
/// recording is what `llama-server` loaded on, in its own startup
/// words — and a report made from each must differ.
#[test]
fn a_measurement_records_the_device_the_sidecar_answered_on() {
    // Startup output as the vendored llama-server (b10145) prints it,
    // canned rather than spawned — CI downloads no weights and no test
    // may read a gitignored path (CLAUDE.md).
    const CUDA_STARTUP: &str = "\
0.00.070.254 I cmn  common_param: common_params_print_info: build 10145 (ad256ded3) with cc 13.3.0 for x86_64-linux-gnu
0.00.070.257 I cmn  common_param: device_info:
0.00.070.265 I cmn  common_param:   - CUDA0   : NVIDIA GeForce RTX 5090 (32109 MiB, 31331 MiB free)
0.00.070.273 I cmn  common_param:   - CPU     : INTEL(R) XEON(R) GOLD 6530 (773849 MiB, 773849 MiB free)
0.00.612.001 I llama_prepare_model_devices: using device CUDA0 (NVIDIA GeForce RTX 5090) (unknown id) - 31331 MiB free
0.02.399.687 I srv  llama_server: model loaded
";
    // The CPU fallback build on the same box: the device table has no
    // accelerator to offer and no device is prepared, so no `using
    // device` line is ever printed.
    const CPU_ONLY_STARTUP: &str = "\
0.00.057.086 I cmn  common_param: common_params_print_info: build 10145 (ad256ded3) with cc 13.3.0 for x86_64-linux-gnu
0.00.057.089 I cmn  common_param: device_info:
0.00.057.093 I cmn  common_param:   - CPU     : INTEL(R) XEON(R) GOLD 6530 (773849 MiB, 773849 MiB free)
0.00.876.628 I load_tensors:   CPU_Mapped model buffer size =  2862.99 MiB
0.05.859.504 I srv  llama_server: model loaded
";

    let on_gpu = runner::sidecar::device_from_startup(CUDA_STARTUP);
    let on_cpu = runner::sidecar::device_from_startup(CPU_ONLY_STARTUP);
    assert_eq!(
        on_gpu.as_deref(),
        Some("CUDA0 (NVIDIA GeForce RTX 5090)"),
        "the device is the sidecar's own startup words, without the memory noise"
    );
    assert_eq!(
        on_cpu.as_deref(),
        Some("CPU"),
        "a CPU-only build positively says CPU — it is a recorded fact, not an absence"
    );

    // The same host, the same weights, the same mock answers — only the
    // sidecar's device differs. The two reports must not be
    // byte-identical provenance.
    let report_on = |device: Option<String>, scratch_name: &str| {
        let fixtures = original_two_fixtures(scratch_name);
        let pack = load_pack(&pack_dir()).expect("pack loads");
        let mock = MockModel::respond_sequence(vec![
            ("200 OK", statement_01_normalise()),
            ("200 OK", statement_01_classify()),
            ("200 OK", statement_02_normalise()),
            ("200 OK", statement_02_classify()),
        ]);
        FixtureEvaluator {
            answers: Answers::FromModel(mock.endpoint()),
            model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
            machine: machine(),
            sidecar: Some(SidecarInfo {
                device,
                ..sidecar()
            }),
            peak_rss: None,
            fixtures_dir: Some(fixtures.0.clone()),
            runs_dir: None,
            resume_dir: None,
            pdfium_dir: None,
        }
        .evaluate(&pack)
        .expect("the eval runs")
    };

    let cuda_report = report_on(on_gpu, "eval-device-cuda-fixtures");
    let cpu_report = report_on(on_cpu, "eval-device-cpu-fixtures");

    assert_ne!(
        cuda_report.sidecar, cpu_report.sidecar,
        "a CUDA run and a CPU-only run on the same host must produce different provenance"
    );
    assert_eq!(
        cuda_report
            .sidecar
            .as_ref()
            .expect("the report names its sidecar")
            .device
            .as_deref(),
        Some("CUDA0 (NVIDIA GeForce RTX 5090)"),
    );

    // A record from before the field existed still reads — as "not
    // recorded", which is never "no accelerator".
    let old: SidecarInfo = serde_json::from_str(
        r#"{"version": "10050 (b15ca938a)", "file": "llama-server-macos-arm64"}"#,
    )
    .expect("a pre-#490 sidecar record still parses");
    assert_eq!(old.device, None, "absent means not recorded, never CPU");
}

/// A directory that cleans itself up, so a failing test leaves no
/// litter.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let path = std::env::temp_dir().join(format!("kettle-test-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// These tests exercise evaluator plumbing with four canned model
/// responses. Keep that test input fixed even when the shipped pack
/// gains evidence: adding #234's broad fixture must not turn an HTTP
/// metrics test into an 80-merchant response fixture.
fn original_two_fixtures(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    let source = pack_dir().join("fixtures");
    for file in [
        "statement-01.csv",
        "statement-01.expected.json",
        "statement-02-messy.csv",
        "statement-02-messy.expected.json",
    ] {
        std::fs::copy(source.join(file), scratch.0.join(file)).expect("copy eval fixture");
    }
    scratch
}

/// An eval that scores 0.60 and cannot say which answers were wrong is
/// a number nobody can act on. Given somewhere to write, the harness
/// keeps each fixture's model exchanges exactly as they went over the
/// wire — the same per-run directory a real run gets (brief §11).
#[test]
fn eval_runs_keep_their_model_exchanges_when_given_somewhere_to_put_them() {
    let scratch = Scratch::new("eval-logs");
    let fixtures = original_two_fixtures("eval-log-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: Some(scratch.0.clone()),
        resume_dir: None,
        pdfium_dir: None,
    };

    evaluator.evaluate(&pack).expect("the eval runs");

    let written: Vec<String> = walk(&scratch.0);
    let item_file = written
        .iter()
        .find(|name| name.ends_with("eval-items.json"))
        .expect("the scored per-item document is emitted for every kept eval run");
    let items: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(item_file).expect("read eval-items.json"))
            .expect("eval-items.json is JSON");
    assert_eq!(
        items[0]["pack"], "app.kttl.subscription-audit",
        "{items:#?}"
    );
    assert!(items[0]["actual"]["state"].is_string(), "{items:#?}");

    let answers: Vec<&String> = written
        .iter()
        .filter(|name| name.contains("response") || name.contains("answer"))
        .collect();
    assert!(
        !answers.is_empty(),
        "no model answers were kept: {written:#?}"
    );

    // The names the model actually gave are on disk to be read back.
    let kept = written
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap_or_default())
        .collect::<String>();
    assert!(kept.contains("PureGym"), "the answers themselves are kept");
}

/// #118: eval run ids are deliberately deterministic — pack, model,
/// fixture — so re-running an eval into the same directory reuses them.
/// The second run must replace the first outright: a directory holding
/// half of one run's exchanges and half of another's is worse than no
/// log at all, because it reads as one run.
#[test]
fn re_running_an_eval_replaces_its_exchanges_rather_than_mixing_them() {
    let scratch = Scratch::new("eval-logs-rerun");
    let fixtures = original_two_fixtures("eval-rerun-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");

    let evaluate_once = || {
        let mock = MockModel::respond_sequence(vec![
            ("200 OK", statement_01_normalise()),
            ("200 OK", statement_01_classify()),
            ("200 OK", statement_02_normalise()),
            ("200 OK", statement_02_classify()),
        ]);
        FixtureEvaluator {
            answers: Answers::FromModel(mock.endpoint()),
            model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
            machine: machine(),
            sidecar: None,
            peak_rss: None,
            fixtures_dir: Some(fixtures.0.clone()),
            runs_dir: Some(scratch.0.clone()),
            resume_dir: None,
            pdfium_dir: None,
        }
        .evaluate(&pack)
        .expect("the eval runs");
    };

    evaluate_once();
    let first = walk(&scratch.0);
    // Litter only the first run could have left.
    let stale = Path::new(&first[0])
        .parent()
        .expect("a run directory")
        .join("9999-stale.response.json");
    std::fs::write(&stale, "an answer from the run before").expect("stale file");

    evaluate_once();

    assert!(
        !stale.exists(),
        "the earlier run's exchanges survived into the new one: {stale:?}"
    );
    assert_eq!(
        walk(&scratch.0).len(),
        first.len(),
        "the second run leaves exactly what one run writes"
    );
}

/// Every file under `dir`, recursively.
fn walk(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path.to_string_lossy().into_owned());
        }
    }
    found
}

/// #303: the eval writes whose answers these are, where a replay reads it.
///
/// The two halves of the fix meeting. `baseline::compare` joins reports
/// on the model, so a replayed report labelled "without a model" could
/// never be compared against a live measurement — which defeated the
/// case replay was built for, since a scoring change is exactly when
/// SCORING_VERSION bumps and every baseline must be re-recorded.
///
/// The directory name has carried the model all along. Reading it back
/// out of a file name would be guesswork the first time a name changes,
/// so the run records it instead: a recording describes itself.
#[test]
fn a_run_directory_names_the_model_so_a_replay_can_say_whose_answers_it_serves() {
    let fixtures = original_two_fixtures("eval-run-dir-model-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let runs = std::env::temp_dir().join(format!("kettle-run-dir-model-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&runs);

    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: Some(runs.clone()),
        resume_dir: None,
        pdfium_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs");

    let manifest_path = walk(&runs)
        .into_iter()
        .find(|path| path.ends_with("run.json"))
        .expect("the eval writes a run manifest");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("read the run manifest"),
    )
    .expect("the run manifest is JSON");
    assert_eq!(manifest["request"]["message_role"], "user");
    assert_eq!(
        manifest["request"]["max_tokens"],
        runner::exec::MAX_ANSWER_TOKENS
    );

    let recording =
        runner::eval::replay::Recording::from_run_dirs(&runs).expect("the recording loads");
    let model = recording
        .model()
        .expect("the eval recorded which model answered");
    assert_eq!(model.file, "qwen2.5-7b-instruct-q4_k_m.gguf");
    assert_eq!(model.context, 8192);

    let _ = std::fs::remove_dir_all(&runs);
}

/// One wrong category, asserted at high, and one correct answer given
/// at low so both buckets carry a decision.
fn statement_01_classify_one_wrong_at_high() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "name": "PureGym", "kind": "subscription", "category": "fitness", "confidence": "high"},
        {"id": 1, "name": "Netflix", "kind": "subscription", "category": "retail", "confidence": "high"},
        {"id": 2, "name": "Spotify", "kind": "subscription", "category": "streaming", "confidence": "low"},
        {"id": 3, "name": "British Gas", "kind": "utility", "category": "energy", "confidence": "high"},
        {"id": 4, "name": "Tesco", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
        {"id": 5, "name": "Acme Payroll", "kind": "regular_spend", "category": "other", "confidence": "high"}
    ]}"#,
    )
}

/// #429: a classification decision records the confidence the model
/// declared, and the risk table reads it.
///
/// Until now it recorded `None`, and `fixture.rs` gave the reason: a
/// merchant's answer looked like it carried two questions' confidences,
/// and assigning one of them would be #271's inherited-confidence
/// defect. The split that resolves it is not two levels but one
/// question. The model answers **`category`** and nothing else (#253) —
/// `kind` is Rust's, from cadence or the pack's category→kind map — so
/// the declared level is a claim about the category and only that.
///
/// Which decides both halves of this test. The level is taken from the
/// `Classification` claim trace, never the `Normalisation` one beside
/// it, because a name-recognition confidence is a different question
/// again. And correctness here is **category** correctness: charging a
/// cadence error to the model's category confidence would import an
/// answer from a question it never asked.
#[test]
fn a_classification_lands_in_the_bucket_its_answer_declared() {
    let fixtures = original_two_fixtures("eval-calibration-fixtures");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_01_normalise()),
        ("200 OK", statement_01_classify_one_wrong_at_high()),
        ("200 OK", statement_02_normalise()),
        ("200 OK", statement_02_classify()),
    ]);

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: Some(sidecar()),
        peak_rss: None,
        fixtures_dir: Some(fixtures.0.clone()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir: None,
    };

    let report = evaluator.evaluate(&pack).expect("the eval runs");
    let MetricReport::Classification(metrics) = &report.metrics[&EvalMetric::Classification] else {
        panic!("an audit pack reports classification metrics");
    };
    let calibration = &metrics.calibration;

    let high = calibration
        .buckets
        .get("high")
        .unwrap_or_else(|| panic!("the high answers have a bucket: {calibration:?}"));
    assert_eq!(
        high.errors, 1,
        "Netflix's category was wrong and was asserted at high: {calibration:?}"
    );

    // The low answer is *routed*, not asserted, and routed is not an
    // error: the pipeline already surfaces a low-confidence
    // classification to a person rather than putting it in the report.
    // Which is the fact this table has to be read against — the low
    // bucket of this pack is a review queue, not a worse-answers pile,
    // so "does low predict error" is unanswerable here by construction
    // and the honest question is what the *high* bucket costs and what
    // the routing buys.
    let low = calibration
        .buckets
        .get("low")
        .unwrap_or_else(|| panic!("the low answer has a bucket: {calibration:?}"));
    assert_eq!(low.decisions, 1, "{calibration:?}");
    assert_eq!(low.routed_to_review, 1, "{calibration:?}");
    assert_eq!(
        low.errors, 0,
        "a surfaced answer was never asserted, so it cannot be confidently wrong: {calibration:?}"
    );
    // And its rate is **undefined**, not zero (#429: error among
    // automatically asserted items). Every decision in this bucket was
    // routed, so there is nothing it could have got wrong — a 0.00 here
    // would be a fact about the routing wearing the costume of a fact
    // about the model, and `ranking_signal` would then rank against it.
    // The 14 August subscription replay is where that happened: 40 of
    // 40 low decisions routed, a guaranteed 0.00, and an INVERTED
    // verdict on a pack whose real comparison points the other way.
    assert_eq!(
        low.error_rate.n, 0,
        "a fully routed bucket asserts nothing, so its rate has no denominator: {calibration:?}"
    );
    assert!(
        low.error_rate.wilson_95.is_none(),
        "an undefined rate carries no interval to be ranked by: {calibration:?}"
    );

    // Ten decisions cannot establish that low is worse than high, and
    // the report must say so rather than let the counts read as a
    // ranking (#429).
    assert!(
        matches!(calibration.signal, runner::eval::RankingSignal::Unproven),
        "{calibration:?}"
    );
}
