//! #511: the public score-card projection is derived from eval reports,
//! never copied into the website. The projection keeps stale evidence
//! visible as history and refuses to call it current.

use std::path::{Path, PathBuf};

fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-scores-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("evals")).expect("create evals");
    std::fs::create_dir_all(dir.join("app/src-tauri")).expect("create model manifest dir");
    std::fs::write(
        dir.join("app/src-tauri/models.json"),
        r#"[{"file_name":"measured.gguf","sha256":"sha256:abc123","url":"https://example.invalid/model","bytes":123}]"#,
    )
    .expect("write model manifest");
    dir
}

fn write_baseline(root: &Path, name: &str, scoring_version: u32, recorded_at: &str) {
    let report = |model: &str, sidecar: &str, runtime: &str, end_to_end: f32| {
        format!(
            r#"{{
              "pack":"app.kttl.letter-to-actions",
              "pack_version":"0.2.0",
              "eval_set":"development",
              {model}
              "machine":{{"cpu":"Test CPU","ram_gb":32,"os":"Test OS"}},
              {sidecar}
              "fixtures":[{{
                "fixture":"one.txt",
                "step_scores":{{}},
                "items":[],
                "containment":{{
                  "candidates":10,"accepted":6,"needs_review":2,"rejected":2,
                  "deduplicated":0,"absent_after_retry":0,"contained":2,
                  "escaped":1,"pipeline_introduced":1,
                  "by_guardrail":{{"review_routing":{{"passed":8,"failed":2,"contained":2,"escaped":1}}}}
                }},
                "end_to_end":{end_to_end},
                "needs_review_rate":0.2,
                "perf":{{"wall_ms":1250,"model_ms":1000,"tokens_per_second":20.0,"peak_rss_mb":512,"retries":0}}
              }}],
              "metrics":{{}},
              "bed":"blake3:bed",
              {runtime}
              "verdict":"pass"
            }}"#,
        )
    };
    let model = report(
        r#""model":{"file":"measured.gguf","params":"4B","quant":"Q4_K_M","context":8192},"#,
        r#""sidecar":{"version":"10145 (abc)","file":"llama-server","device":"Test GPU"},"#,
        r#""runtime":{"context":8192,"parallel":1,"reasoning":"off","max_answer_tokens":4096},"#,
        0.75,
    );
    let floor = report("", "", "", 0.5);
    std::fs::write(
        root.join("evals").join(name),
        format!(
            r#"{{"scoring_version":{scoring_version},"recorded_at":"{recorded_at}","reports":[{model},{floor}]}}"#,
        ),
    )
    .expect("write baseline");
}

#[test]
fn scores_are_a_validated_projection_of_measurements_not_page_copy() {
    let dir = root("projection");
    write_baseline(&dir, "baseline-current.json", 14, "2026-08-14T10:00:00Z");
    write_baseline(&dir, "baseline-history.json", 13, "2026-08-11T10:00:00Z");

    let outcome = cli::scores::run_json(&dir, 14);
    assert_eq!(outcome.code, cli::scores::ExitCode::Ok, "{}", outcome.text);
    let document: serde_json::Value = serde_json::from_str(&outcome.text).expect("scores JSON");

    assert_eq!(document["schema"], "kettle/public-scores@0");
    assert_eq!(document["current_scoring_version"], 14);
    let measurements = document["measurements"].as_array().expect("measurements");
    assert_eq!(measurements.len(), 4);
    assert_eq!(
        measurements
            .iter()
            .filter(|row| row["state"] == "current")
            .count(),
        2
    );
    assert_eq!(
        measurements
            .iter()
            .filter(|row| row["state"] == "historical")
            .count(),
        2
    );

    let model = measurements
        .iter()
        .find(|row| row["policy"]["kind"] == "model" && row["state"] == "current")
        .expect("current model");
    assert_eq!(model["policy"]["sha256"], "sha256:abc123");
    assert_eq!(model["scores"]["end_to_end"], 0.75);
    assert_eq!(model["scores"]["review_rate"], 0.2);
    assert_eq!(model["scores"]["contained"], 2);
    assert_eq!(model["scores"]["escaped"], 1);
    assert_eq!(model["scores"]["pipeline_introduced"], 1);
    assert_eq!(model["scores"]["wall_ms"], 1250);
    assert_eq!(model["scores"]["peak_rss_mb"], 512);
    assert_eq!(model["scores"]["tokens_per_second"], 20.0);

    assert!(measurements.iter().any(|row| {
        row["policy"]["kind"] == "deterministic_floor" && row["state"] == "current"
    }));
}

#[test]
fn a_malformed_measurement_stops_the_public_projection() {
    let dir = root("broken");
    std::fs::write(dir.join("evals/baseline-broken.json"), "not json")
        .expect("write broken baseline");

    let outcome = cli::scores::run_json(&dir, 14);
    assert_eq!(outcome.code, cli::scores::ExitCode::Broken);
    assert!(
        outcome.text.contains("baseline-broken.json"),
        "{}",
        outcome.text
    );
}
