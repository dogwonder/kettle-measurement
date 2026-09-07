//! #288: a scorer change is verified against recorded answers, not by
//! asking the model questions it has already answered.
//!
//! The safety property is the interesting half. A replay that served
//! the old prompt's answers to a new prompt's questions would report a
//! clean run for a measurement nobody made — so most of what is
//! asserted here is what must *refuse*.

mod support;

use runner::eval::replay::Recording;
use runner::exec::{call_constrained, Endpoint};
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": { "results": { "type": "array" } },
        "required": ["results"]
    })
}

#[test]
fn disk_replay_refuses_an_expanded_enum_even_when_the_old_answer_is_valid() {
    use runner::exec::{render_prompt, run_batch, BatchContext, BatchItem};
    use runner::run_dir::RunDir;
    let dir = std::env::temp_dir().join(format!("kettle-disk-schema-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log = RunDir::create(&dir, "recording").unwrap();
    let schema = serde_json::json!({
        "type": "object", "required": ["results"],
        "properties": {"results": {"type": "array", "items": {
            "type": "object", "properties": {"choice": {"enum": ["yes"]}}
        }}}
    });
    let body = r#"{"results":[{"id":0,"raw":"one","choice":"yes"}]}"#;
    let model = MockModel::respond_once("200 OK", completion_envelope(body));
    let batch = [BatchItem::new(0, "one")];
    let template = "Read {{ batch_json }}";
    let context = BatchContext {
        log: &log,
        step: "Read",
        batch: 1,
        cancel: &AtomicBool::new(false),
    };
    run_batch(
        &model.endpoint(),
        template,
        None,
        &schema,
        &batch,
        "raw",
        &context,
    )
    .unwrap();
    let recording = Recording::from_run_dirs(&dir).unwrap();
    let endpoint = Endpoint::replaying(recording);
    let prompt = render_prompt(template, &batch, None).unwrap();
    assert_eq!(
        call_constrained(&endpoint, &prompt, &schema, context.cancel).unwrap()["results"][0]
            ["choice"],
        "yes"
    );
    let mut widened = schema.clone();
    widened["properties"]["results"]["items"]["properties"]["choice"]["enum"] =
        serde_json::json!(["yes", "no"]);
    assert!(jsonschema::is_valid(
        &widened,
        &serde_json::from_str::<serde_json::Value>(body).unwrap()
    ));
    let error = call_constrained(&endpoint, &prompt, &widened, context.cancel)
        .expect_err("different generation request");
    assert!(error.to_string().contains("no recorded answer"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_replayed_answer_is_served_without_asking_any_model() {
    // No endpoint, no port, no server — if this reaches the network it
    // fails, which is the assertion.
    let mut recording = Recording::default();
    recording.insert(
        "Sort these merchants",
        &schema(),
        String::from(r#"{"results": [{"id": 0, "name": "Netflix"}]}"#),
    );

    let endpoint = Endpoint::replaying(recording);
    let answer = call_constrained(
        &endpoint,
        "Sort these merchants",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect("the recorded answer is served");

    assert_eq!(answer["results"][0]["name"], "Netflix");
}

#[test]
fn a_replay_refuses_a_prompt_the_recording_never_heard() {
    // The whole safety property. A prompt edit changes what the model
    // would say, so the old answers are not evidence about the new
    // prompt — and this must fail loudly rather than score one against
    // the other.
    let mut recording = Recording::default();
    recording.insert(
        "Sort these merchants",
        &schema(),
        String::from(r#"{"results": []}"#),
    );

    let endpoint = Endpoint::replaying(recording);
    let error = call_constrained(
        &endpoint,
        "Sort these merchants, and mind the pennies",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect_err("a changed prompt must refuse");

    let message = error.to_string();
    assert!(
        message.contains("no recorded answer"),
        "the refusal should say what happened: {message}"
    );
    assert!(
        message.contains("record again"),
        "and what to do about it: {message}"
    );
}

#[test]
fn a_changed_schema_also_refuses_even_when_the_prompt_is_identical() {
    // The schema is part of what constrains generation, so it is part
    // of the question. Keying on the request alone catches this
    // without anyone having to remember it.
    let mut recording = Recording::default();
    recording.insert(
        "Sort these merchants",
        &schema(),
        String::from(r#"{"results": []}"#),
    );

    let widened = serde_json::json!({
        "type": "object",
        "properties": { "results": { "type": "array" }, "note": { "type": "string" } },
        "required": ["results"]
    });
    let endpoint = Endpoint::replaying(recording);
    assert!(
        call_constrained(
            &endpoint,
            "Sort these merchants",
            &widened,
            &AtomicBool::new(false)
        )
        .is_err(),
        "a different schema is a different question"
    );
}

/// #328: the rendered prompt is only the content of one message, not
/// the request Kettle sent. A chat template may answer the same bytes
/// differently in a system turn and a user turn, so a recording made
/// under one role is not evidence about the other.
#[test]
fn a_recording_refuses_the_same_prompt_sent_under_a_different_role() {
    let dir =
        std::env::temp_dir().join(format!("kettle-replay-request-role-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = dir.join("app.kttl.test-somepack-model-fixture-csv");
    let raw = run.join("raw");
    std::fs::create_dir_all(&raw).expect("run dir");

    std::fs::write(
        raw.join("0001-sorting-merchants.request.txt"),
        "Sort these merchants",
    )
    .expect("write request");
    std::fs::write(
        raw.join("0001-sorting-merchants.response.json"),
        String::from(r#"{"results": [{"id": 0, "name": "System answer"}]}"#),
    )
    .expect("write response");
    std::fs::write(
        run.join("run.json"),
        serde_json::json!({
            "inputs": [],
            "request": {
                "model": "local",
                "temperature": 0,
                "message_role": "system",
                "max_tokens": runner::exec::MAX_ANSWER_TOKENS,
                "response_format": "json_schema"
            }
        })
        .to_string(),
    )
    .expect("write manifest");

    let endpoint = Endpoint::replaying(
        Recording::from_run_dirs(&dir).expect("the system-role recording loads"),
    );
    let error = call_constrained(
        &endpoint,
        "Sort these merchants",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect_err("a user-turn request must not receive a system-turn answer");

    assert!(error.to_string().contains("no recorded answer"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Runs archived before #328 cannot say which request policy produced
/// an answer. They remain usable as the current policy, but two
/// different answers to one inferred request are ambiguous and must
/// refuse rather than letting directory order choose the evidence.
#[test]
fn legacy_runs_with_conflicting_answers_to_one_request_are_refused() {
    let dir = std::env::temp_dir().join(format!(
        "kettle-replay-conflicting-legacy-runs-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    for (run_name, answer) in [("old-system", "System answer"), ("new-user", "User answer")] {
        let raw = dir.join(run_name).join("raw");
        std::fs::create_dir_all(&raw).expect("run dir");
        std::fs::write(
            raw.join("0001-sorting-merchants.request.json"),
            "Sort these merchants",
        )
        .expect("write request");
        std::fs::write(
            raw.join("0001-sorting-merchants.response.json"),
            format!(r#"{{"results": [{{"id": 0, "name": "{answer}"}}]}}"#),
        )
        .expect("write response");
    }

    let error = Recording::from_run_dirs(&dir)
        .expect_err("ambiguous legacy evidence must not choose whichever answer loads last");

    assert!(
        error.contains("different answers for the same request"),
        "{error}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_replayed_answer_is_validated_by_the_same_rules_as_a_live_one() {
    // A replay must be scored identically or it is not the same
    // measurement. An answer that no longer satisfies the schema is
    // still invalid on the way back out of the recording.
    let mut recording = Recording::default();
    recording.insert(
        "Sort these merchants",
        &schema(),
        String::from(r#"{"wrong_field": []}"#),
    );

    let endpoint = Endpoint::replaying(recording);
    let error = call_constrained(
        &endpoint,
        "Sort these merchants",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect_err("schema validation still applies");
    assert!(
        matches!(error, runner::exec::ModelCallError::Invalid { .. }),
        "{error:?}"
    );
}

#[test]
fn a_recording_is_loaded_from_the_run_directories_an_eval_already_writes() {
    // The input for a replay is a by-product Kettle already produces.
    let dir = std::env::temp_dir().join(format!("kettle-replay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let raw = dir
        .join("app.kttl.test-somepack-model-fixture-csv")
        .join("raw");
    std::fs::create_dir_all(&raw).expect("run dir");

    std::fs::write(
        raw.join("0001-sorting-merchants.request.json"),
        "Sort these merchants",
    )
    .expect("write request");
    std::fs::write(
        raw.join("0001-sorting-merchants.response.json"),
        String::from(r#"{"results": [{"id": 0, "name": "Spotify"}]}"#),
    )
    .expect("write response");
    // A request whose answer never landed — the interruption that
    // stopped a run mid-write. Skipped, not half-loaded.
    std::fs::write(
        raw.join("0002-sorting-merchants.request.json"),
        "Another question",
    )
    .expect("write orphan");

    let recording = Recording::from_run_dirs(&dir).expect("the recording loads");
    assert_eq!(recording.len(), 1, "the orphaned request is not an answer");

    let endpoint = Endpoint::replaying(recording);
    let answer = call_constrained(
        &endpoint,
        "Sort these merchants",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect("served from disk");
    assert_eq!(answer["results"][0]["name"], "Spotify");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_recording_says_so_rather_than_replaying_nothing() {
    let dir = std::env::temp_dir().join(format!("kettle-replay-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let error = Recording::from_run_dirs(&dir).expect_err("nothing to replay");
    assert!(error.contains("no recorded answers"), "{error}");
}

#[test]
fn a_live_endpoint_is_untouched_by_any_of_this() {
    // The existing path must behave exactly as before: a replay is an
    // addition, not a change to how a real measurement is taken.
    let mock = MockModel::respond_sequence(vec![(
        "200 OK",
        completion_envelope(r#"{"results": [{"id": 0, "name": "Disney+"}]}"#),
    )]);
    let answer = call_constrained(
        &mock.endpoint(),
        "Sort these merchants",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect("the live path still works");
    assert_eq!(answer["results"][0]["name"], "Disney+");
}

/// #303: a replay must be able to say whose answers it is serving.
///
/// `baseline::compare` joins reports on `model_name()`, and a replayed
/// report used to be labelled `without a model` — so a baseline
/// re-derived by replay could never be compared against a live
/// measurement. The join missed and the comparison reported "the
/// baseline measured without a model and this eval didn't" as a
/// regression, rather than comparing anything.
///
/// That defeated the case replay was built on: a scoring change is
/// exactly when SCORING_VERSION bumps and every baseline must be
/// re-recorded, and a re-recorded baseline is precisely what replay
/// could not produce.
///
/// The information was never lost, only dropped. A run directory is
/// written by a run that knew which model answered.
#[test]
fn a_recording_knows_which_model_answered() {
    let dir = std::env::temp_dir().join(format!("kettle-replay-model-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = dir.join("app.kttl.test-somepack-qwen2.5-7b-instruct-q4_k_m-fixture-csv");
    std::fs::create_dir_all(run.join("raw")).expect("run dir");

    std::fs::write(
        run.join("raw").join("0001-sorting-merchants.request.json"),
        "Sort these merchants",
    )
    .expect("write request");
    std::fs::write(
        run.join("raw").join("0001-sorting-merchants.response.json"),
        String::from(r#"{"results": [{"id": 0, "name": "Spotify"}]}"#),
    )
    .expect("write response");
    std::fs::write(
        run.join("run.json"),
        serde_json::json!({
            "inputs": [],
            "model": {
                "file": "qwen2.5-7b-instruct-q4_k_m.gguf",
                "params": "7B",
                "quant": "Q4_K_M",
                "context": 8192
            }
        })
        .to_string(),
    )
    .expect("write manifest");

    let recording = Recording::from_run_dirs(&dir).expect("the recording loads");
    let model = recording
        .model()
        .expect("a recording of a run that used a model names it");
    assert_eq!(model.file, "qwen2.5-7b-instruct-q4_k_m.gguf");
    assert_eq!(model.params, "7B");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A recording spanning two models cannot name one, and must not
/// silently pick either: the answers it serves came from both, so any
/// single label on the resulting report would be a false claim about
/// evidence — the exact defect #303 is about, one layer down.
#[test]
fn a_recording_spanning_two_models_refuses_rather_than_choosing() {
    let dir = std::env::temp_dir().join(format!("kettle-replay-two-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (slug, file) in [
        ("qwen2.5-7b", "qwen2.5-7b-instruct-q4_k_m.gguf"),
        ("qwen3.5-4b", "qwen3.5-4b-instruct-q4_k_m.gguf"),
    ] {
        let run = dir.join(format!("app.kttl.test-somepack-{slug}-fixture-csv"));
        std::fs::create_dir_all(run.join("raw")).expect("run dir");
        std::fs::write(
            run.join("raw").join(format!("0001-{slug}.request.json")),
            format!("Sort these merchants for {slug}"),
        )
        .expect("write request");
        std::fs::write(
            run.join("raw").join(format!("0001-{slug}.response.json")),
            String::from(r#"{"results": []}"#),
        )
        .expect("write response");
        std::fs::write(
            run.join("run.json"),
            serde_json::json!({
                "inputs": [],
                "model": {"file": file, "params": "7B", "quant": "Q4_K_M", "context": 8192}
            })
            .to_string(),
        )
        .expect("write manifest");
    }

    let problem =
        Recording::from_run_dirs(&dir).expect_err("two models in one recording is refused");
    assert!(
        problem.contains("qwen2.5-7b-instruct-q4_k_m.gguf")
            && problem.contains("qwen3.5-4b-instruct-q4_k_m.gguf"),
        "the refusal names both models so it can be acted on: {problem}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #478: the recording's request file holds the rendered prompt as
/// plain text, and has always been named `.request.json`. Publishing an
/// archive whose filenames misdescribe their contents is a small
/// credibility tax on a repository whose whole argument is that claims
/// match evidence, so the writer now emits `.request.txt`.
///
/// The reader must accept both, and this is the test that says why: the
/// 17,145 files already archived in `kettle-runs` are the evidence that
/// lets a score be re-asked under new scoring without re-running the
/// GPU. A rename that stranded them would cost far more than the
/// honesty it bought. The response file is untouched — it really is
/// JSON.
#[test]
fn a_recording_replays_whether_its_request_was_named_txt_or_json() {
    let dir = std::env::temp_dir().join(format!("kettle-replay-suffix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // What the writer produces today.
    let fresh = dir.join("app.kttl.test-pack-model-fresh-csv").join("raw");
    std::fs::create_dir_all(&fresh).expect("fresh run dir");
    std::fs::write(
        fresh.join("0001-sorting-merchants.request.txt"),
        "Sort the new way",
    )
    .expect("write request");
    std::fs::write(
        fresh.join("0001-sorting-merchants.response.json"),
        String::from(r#"{"results": [{"id": 0, "name": "Spotify"}]}"#),
    )
    .expect("write response");

    // What every archived run already on disk looks like.
    let archived = dir
        .join("app.kttl.test-pack-model-archived-csv")
        .join("raw");
    std::fs::create_dir_all(&archived).expect("archived run dir");
    std::fs::write(
        archived.join("0001-sorting-merchants.request.json"),
        "Sort the old way",
    )
    .expect("write archived request");
    std::fs::write(
        archived.join("0001-sorting-merchants.response.json"),
        String::from(r#"{"results": [{"id": 0, "name": "Bandcamp"}]}"#),
    )
    .expect("write archived response");

    let recording = Recording::from_run_dirs(&dir).expect("the recording loads");
    assert_eq!(recording.len(), 2, "both namings are answers");
    assert_eq!(recording.compatibility().legacy_prompt_only_requests, 2);
    assert_eq!(recording.compatibility().exact_requests, 0);

    let endpoint = Endpoint::replaying(recording);
    let fresh_answer = call_constrained(
        &endpoint,
        "Sort the new way",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect("the .txt recording serves");
    assert_eq!(fresh_answer["results"][0]["name"], "Spotify");

    let endpoint = Endpoint::replaying(Recording::from_run_dirs(&dir).expect("loads"));
    let archived_answer = call_constrained(
        &endpoint,
        "Sort the old way",
        &schema(),
        &AtomicBool::new(false),
    )
    .expect("the archived .json recording still serves");
    assert_eq!(archived_answer["results"][0]["name"], "Bandcamp");
}

#[test]
fn new_recordings_refuse_missing_corrupt_or_unknown_generation_identity() {
    use runner::exec::GenerationRequest;
    use runner::run_dir::{RunDir, RunLog};
    let root =
        std::env::temp_dir().join(format!("kettle-recording-identity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = RunDir::create(&root, "new").unwrap();
    let request = GenerationRequest::current("Read", &schema());
    dir.generation("Read", 1, &[], &request, r#"{"results":[]}"#);
    let identity = dir.path.join("raw/0001-read.generation.json");
    let original = std::fs::read(&identity).unwrap();
    assert_eq!(
        Recording::from_run_dirs(&root)
            .unwrap()
            .compatibility()
            .exact_requests,
        1
    );
    for invalid in [
        "{".to_owned(),
        serde_json::json!({"version":99,"payload":request.payload}).to_string(),
    ] {
        std::fs::write(&identity, invalid).unwrap();
        assert!(Recording::from_run_dirs(&root).is_err());
    }
    std::fs::remove_file(&identity).unwrap();
    assert!(Recording::from_run_dirs(&root)
        .unwrap_err()
        .contains("cannot use prompt-only"));
    std::fs::write(&identity, original).unwrap();
    std::fs::write(dir.path.join("raw/0001-read.request.txt"), "Changed").unwrap();
    assert!(Recording::from_run_dirs(&root)
        .unwrap_err()
        .contains("inconsistent"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_exact_recording_cannot_fall_back_to_a_legacy_answer_for_a_changed_schema() {
    use runner::exec::GenerationRequest;
    use runner::run_dir::{RunDir, RunLog};
    let root = std::env::temp_dir().join(format!("kettle-recording-mixed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let exact = RunDir::create(&root, "exact").unwrap();
    exact.generation(
        "Read",
        1,
        &[],
        &GenerationRequest::current("Read", &schema()),
        r#"{"results":[]}"#,
    );
    let legacy = RunDir::create(&root, "legacy").unwrap();
    legacy.exchange("Read", 1, &[], "Read", r#"{"results":[]}"#);
    let recording = Recording::from_run_dirs(&root).unwrap();
    assert!(recording.answer_for("Read", &schema()).is_some());
    assert_eq!(recording.compatibility().legacy_prompt_only_requests, 1);
    let mut changed = schema();
    changed["description"] = serde_json::json!("A changed generation schema");
    assert!(recording.answer_for("Read", &changed).is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn disk_identity_covers_every_generation_policy_field() {
    use runner::exec::GenerationRequest;
    use runner::run_dir::{RunDir, RunLog};
    let root = std::env::temp_dir().join(format!("kettle-recording-policy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let base = GenerationRequest::current("Read", &schema());
    for (index, (pointer, value)) in [
        ("/model", serde_json::json!("different")),
        ("/temperature", serde_json::json!(1)),
        ("/max_tokens", serde_json::json!(2048)),
        ("/messages/0/role", serde_json::json!("system")),
        ("/response_format/type", serde_json::json!("json_object")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut request = base.clone();
        *request.payload.pointer_mut(pointer).unwrap() = value;
        let dir = RunDir::create(&root, &format!("case-{index}")).unwrap();
        dir.generation("Read", 1, &[], &request, r#"{"results":[]}"#);
        assert!(
            Recording::from_run_dirs(&dir.path)
                .unwrap()
                .answer_for("Read", &schema())
                .is_none(),
            "{pointer}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
