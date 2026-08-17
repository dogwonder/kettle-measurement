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
