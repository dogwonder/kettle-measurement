//! The real CLI evaluator must refuse a schema change on a disk recording.
#[path = "../../runner/tests/support/mod.rs"]
mod support;
use cli::eval::{sidecar_evaluator::SidecarEvaluator, EvalRequest, Evaluator};
use runner::eval::fixture::{EvalSelection, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::Path;

fn copy_pack(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "fixtures" {
            continue;
        }
        if entry.path().is_dir() {
            copy_pack(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn cli_disk_replay_refuses_a_widened_schema_and_reports_legacy_limits() {
    let root =
        std::env::temp_dir().join(format!("kettle-cli-replay-identity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let pack_id = "app.kttl.letter-to-actions";
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(pack_id);
    let pack_dir = root.join("pack");
    copy_pack(&original, &pack_dir);
    let fixtures = root.join("fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let text = "This letter is for information only.";
    std::fs::write(fixtures.join("letter.txt"), text).unwrap();
    std::fs::write(fixtures.join("letter.expected.json"), serde_json::json!({
        "fixture_id":"information-only-letter-01", "obligations":[{
            "id":"information-only-paragraph-01", "segment":text,"expect":null,"strata":["no-obligation"]
        }]
    }).to_string()).unwrap();
    let body = serde_json::json!({"results":[{"id":0,"segment":text,"confidence":"high","obligations":[]}]}).to_string();
    let mock = support::MockModel::respond_once("200 OK", support::completion_envelope(&body));
    let runs = root.join("runs");
    let machine = MachineInfo {
        cpu: "mock".into(),
        ram_gb: 0,
        os: "test".into(),
    };
    FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: None,
        machine: machine.clone(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(fixtures.clone()),
        runs_dir: Some(runs.clone()),
        resume_dir: None,
        pdfium_dir: None,
    }
    .evaluate(&load_pack(&pack_dir).unwrap())
    .unwrap();
    // A nonexistent sidecar proves this is the CLI's disk-only route.
    let cli = SidecarEvaluator {
        sidecar_binary: root.join("does-not-exist"),
        log_dir: root.join("logs"),
        runs_dir: root.join("unused-runs"),
        resume_dir: None,
        sidecars_dir: root.join("sidecars"),
        machine,
    };
    let request = EvalRequest {
        pack: pack_id,
        pack_dir: pack_dir.clone(),
        model: None,
        fixture_dir: Some(&fixtures),
        selection: EvalSelection::Development,
        run: 1,
        runs: 1,
        replay: Some(&runs),
    };
    let exact = cli.evaluate(&request).unwrap();
    assert_eq!(exact.replay_compatibility.unwrap().exact_requests, 1);
    assert!(exact.fixtures[0].items[0].exchanges[0].generation.is_some());
    let schema_path = pack_dir.join("schemas/obligations.schema.json");
    let original_schema = std::fs::read_to_string(&schema_path).unwrap();
    let mut schema: serde_json::Value = serde_json::from_str(&original_schema).unwrap();
    schema["properties"]["results"]["items"]["properties"]["confidence"]["enum"] =
        serde_json::json!(["high", "medium", "low", "unknown"]);
    assert!(jsonschema::is_valid(
        &schema,
        &serde_json::from_str::<serde_json::Value>(&body).unwrap()
    ));
    std::fs::write(&schema_path, schema.to_string()).unwrap();
    assert!(cli
        .evaluate(&request)
        .unwrap_err()
        .contains("no recorded answer"));

    // Synthetic reproduction of the legacy format, leaving real archives untouched.
    for dir in std::fs::read_dir(&runs).unwrap() {
        let run = dir.unwrap().path();
        let manifest = run.join("run.json");
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("generation_request_version");
        std::fs::write(manifest, value.to_string()).unwrap();
        for file in std::fs::read_dir(run.join("raw")).unwrap() {
            let file = file.unwrap().path();
            if file.to_string_lossy().ends_with(".generation.json") {
                std::fs::remove_file(file).unwrap();
            }
        }
    }
    let legacy = cli.evaluate(&request).unwrap();
    let compatibility = legacy.replay_compatibility.unwrap();
    assert_eq!(compatibility.legacy_prompt_only_requests, 1);
    assert_eq!(compatibility.exact_requests, 0);
    assert!(
        legacy.fixtures[0].items[0].exchanges[0]
            .generation
            .is_none(),
        "a legacy answer must not acquire invented provenance"
    );
    std::fs::remove_dir_all(root).unwrap();
}
