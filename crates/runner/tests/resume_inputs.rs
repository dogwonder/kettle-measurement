//! Cache invalidation at the evaluator boundary, using two canned exchanges.
mod support;
use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::{load_pack, Pack};
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

fn setup(name: &str) -> (PathBuf, Pack) {
    let root = std::env::temp_dir().join(format!(
        "kettle-resume-inputs-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit");
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == "fixtures" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }
    copy(&original, &root.join("pack"));
    let fixtures = root.join("fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    std::fs::write(fixtures.join("one.csv"), "Date,Description,Amount\n2026-01-04,NETFLIX.COM,-12.99\n2026-02-04,NETFLIX.COM,-12.99\n2026-03-04,NETFLIX.COM,-12.99\n").unwrap();
    std::fs::write(fixtures.join("one.expected.json"), serde_json::json!({
        "fixture_id":"resume-monthly-video-01", "normalise":[{"raw":"NETFLIX.COM","name":"Netflix"}],
        "classify":[{"id":"resume-netflix-monthly-01","strata":["clean"],"name":"Netflix","kind":"subscription","category":"streaming"}]
    }).to_string()).unwrap();
    std::fs::write(
        root.join("same-name-4b-q4_k_m.gguf"),
        b"synthetic weight identity, not loadable",
    )
    .unwrap();
    let pack = load_pack(&root.join("pack")).unwrap();
    (root, pack)
}

fn evaluator(root: &Path, context: u32) -> FixtureEvaluator {
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            completion_envelope(
                r#"{"results":[{"id":0,"raw":"NETFLIX.COM","name":"Netflix","recognised":true}]}"#,
            ),
        ),
        (
            "200 OK",
            completion_envelope(
                r#"{"results":[{"id":0,"name":"Netflix","category":"streaming","confidence":"high"}]}"#,
            ),
        ),
    ]);
    let mut model = model_info("same-name-4b-q4_k_m.gguf", context);
    model.weights_digest =
        Some(runner::eval::resume::file_identity(&root.join("same-name-4b-q4_k_m.gguf")).unwrap());
    let identity = runner::eval::resume::RuntimeIdentity {
        sidecar_digest: "blake3:mock-runtime".into(),
        policy: runner::eval::RuntimePolicy::effective(&runner::sidecar::SidecarRuntime {
            context,
            ..Default::default()
        }),
        threads: 1,
        environment_digest: "blake3:mock-environment".into(),
    };
    FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint().with_runtime_identity(identity)),
        model: Some(model),
        machine: MachineInfo {
            cpu: "mock CPU".into(),
            ram_gb: 16,
            os: "test OS".into(),
        },
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(root.join("fixtures")),
        runs_dir: None,
        resume_dir: Some(root.join("cache")),
        pdfium_dir: None,
    }
}

#[test]
fn changing_upstream_prompt_misses_with_the_same_classify_prompt_and_pack_version() {
    let (root, pack) = setup("prompt");
    let cold = evaluator(&root, 8192).evaluate(&pack).unwrap();
    assert_eq!(cold.reused_fixtures, 0);
    assert_eq!(cold.fixtures[0].step_scores["normalise"].score, 1.0);
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        1
    );
    let path = pack.dir.join("prompts/normalise.md");
    std::fs::write(
        &path,
        format!(
            "{}\nCheck each merchant carefully.\n",
            std::fs::read_to_string(&path).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_model_context_misses_with_the_same_filename() {
    let (root, pack) = setup("context");
    let cold = evaluator(&root, 8192).evaluate(&pack).unwrap();
    let warm = evaluator(&root, 8192).evaluate(&pack).unwrap();
    assert_eq!(warm.reused_fixtures, 1);
    assert_eq!(
        serde_json::to_value(&cold.fixtures).unwrap(),
        serde_json::to_value(&warm.fixtures).unwrap()
    );
    assert_eq!(
        evaluator(&root, 16384)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_weight_bytes_misses_even_when_the_filename_is_unchanged() {
    let (root, pack) = setup("weights");
    evaluator(&root, 8192).evaluate(&pack).unwrap();
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        1
    );
    std::fs::write(
        root.join("same-name-4b-q4_k_m.gguf"),
        b"different synthetic weights",
    )
    .unwrap();
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn effective_overrides_and_referenced_resources_invalidate_the_cached_result() {
    let (root, mut pack) = setup("effective");
    evaluator(&root, 8192).evaluate(&pack).unwrap();
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        1
    );
    // In-memory overrides must not hide behind an unchanged pack.json.
    if let runner::packs::PipelineStep::Model { batch, .. } = &mut pack.manifest.pipeline[1] {
        *batch = Some(1);
    }
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        0
    );
    for relative in [
        "examples/classify.examples.json",
        "schemas/normalise.schema.json",
        "report.html.tera",
    ] {
        assert_eq!(
            evaluator(&root, 8192)
                .evaluate(&pack)
                .unwrap()
                .reused_fixtures,
            1
        );
        let path = pack.dir.join(relative);
        std::fs::write(
            &path,
            format!("{}\n", std::fs::read_to_string(&path).unwrap()),
        )
        .unwrap();
        assert_eq!(
            evaluator(&root, 8192)
                .evaluate(&pack)
                .unwrap()
                .reused_fixtures,
            0,
            "{relative}"
        );
    }
    pack.manifest
        .kinds
        .insert("streaming".into(), "regular_spend".into());
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn runtime_and_model_metadata_are_effective_inputs() {
    use runner::eval::resume::RuntimeIdentity;
    let (root, pack) = setup("runtime");
    evaluator(&root, 8192).evaluate(&pack).unwrap();
    for field in [
        "context",
        "reasoning",
        "parallel",
        "answer_bound",
        "threads",
        "environment",
        "binary",
        "quant",
        "model_context",
        "machine",
    ] {
        let mut changed = evaluator(&root, 8192);
        let mut runtime = RuntimeIdentity {
            sidecar_digest: "blake3:mock-runtime".into(),
            policy: runner::eval::RuntimePolicy::effective(
                &runner::sidecar::SidecarRuntime::default(),
            ),
            threads: 1,
            environment_digest: "blake3:mock-environment".into(),
        };
        match field {
            "context" => runtime.policy.context *= 2,
            "reasoning" => runtime.policy.reasoning = runner::sidecar::Reasoning::On,
            "parallel" => runtime.policy.parallel += 1,
            "answer_bound" => runtime.policy.max_answer_tokens += 1,
            "threads" => runtime.threads += 1,
            "environment" => runtime.environment_digest.push_str("changed"),
            "binary" => runtime.sidecar_digest.push_str("changed"),
            "quant" => changed.model.as_mut().unwrap().quant = "Q8_0".into(),
            "model_context" => changed.model.as_mut().unwrap().context *= 2,
            "machine" => changed.machine.os.push_str(" updated"),
            _ => unreachable!(),
        }
        if let Answers::FromModel(endpoint) = changed.answers {
            changed.answers = Answers::FromModel(endpoint.with_runtime_identity(runtime));
        }
        assert_eq!(
            changed.evaluate(&pack).unwrap().reused_fixtures,
            0,
            "{field}"
        );
    }
    assert_eq!(
        evaluator(&root, 8192)
            .evaluate(&pack)
            .unwrap()
            .reused_fixtures,
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_live_model_with_unknown_weight_identity_never_reuses_a_result() {
    let (root, pack) = setup("unknown-weights");
    for _ in 0..2 {
        let mut run = evaluator(&root, 8192);
        run.model.as_mut().unwrap().weights_digest = None;
        assert_eq!(run.evaluate(&pack).unwrap().reused_fixtures, 0);
    }
    assert!(!root.join("cache").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_a_bundled_library_changes_the_runtime_identity() {
    let (root, _) = setup("library");
    let binary = root.join("llama-server");
    std::fs::write(&binary, b"synthetic launcher").unwrap();
    let library = root.join("libggml.dylib");
    std::fs::write(&library, b"first shared library").unwrap();
    let old = runner::eval::resume::bundle_identity(&binary).unwrap();
    std::fs::write(&library, b"second shared library").unwrap();
    assert_ne!(old, runner::eval::resume::bundle_identity(&binary).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
