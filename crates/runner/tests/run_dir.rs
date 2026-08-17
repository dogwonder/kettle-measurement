//! The per-run directory (#24).
//!
//! RED BY DESIGN: every `RunDir` method is `todo!()`. The `RunLog` seam
//! it plugs into is already threaded through `run_pack` and
//! `exec::run_batch`, so making these pass is a matter of writing files.

mod support;

use runner::packs::load_pack;
use runner::run::{run_pack, Answers, Progress};
use runner::run_dir::{RunDir, RunDirError};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
}

fn statement() -> PathBuf {
    pack_dir().join("fixtures/statement-02-messy.csv")
}

/// A directory that cleans itself up, so a failing test doesn't leave
/// litter in the repo.
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

fn normalise_answer() -> String {
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

fn classify_answer() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "name": "Disney+", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 1, "name": "Amazon Prime", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 2, "name": "Kaffa Coffee", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
        {"id": 3, "name": "Amazon Marketplace", "kind": "one_off", "category": "retail", "confidence": "medium"},
        {"id": 4, "name": "Coffee Cart", "kind": "regular_spend", "category": "food_drink", "confidence": "low"}
    ]}"#,
    )
}

#[test]
fn contains_raw_io_after_step() {
    let scratch = Scratch::new("run-dir-raw-io");
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", normalise_answer()),
        ("200 OK", classify_answer()),
    ]);

    let run_dir = RunDir::create(&scratch.0, "run-01").expect("the run directory is created");
    let outcome = run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_: Progress| {},
        &run_dir,
    )
    .expect("the run completes");
    run_dir
        .record_claims(&outcome.claim_traces)
        .expect("claim evidence recorded");

    let raw = run_dir.path.join("raw");
    let mut files: Vec<String> = std::fs::read_dir(&raw)
        .expect("raw/ exists")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();

    assert_eq!(
        files.len(),
        4,
        "two model steps, a request and a response each: {files:?}"
    );
    assert!(
        files[0].starts_with("0001-grouping-payments-by-merchant"),
        "raw files are numbered in the order they happened and named after \
         what a person saw: {files:?}"
    );

    let request = std::fs::read_to_string(raw.join(&files[0])).expect("the request was written");
    assert!(
        request.contains("DISNEYPLUS"),
        "the request holds exactly what was asked"
    );

    let response: String = files
        .iter()
        .filter(|f| f.contains("response"))
        .map(|f| std::fs::read_to_string(raw.join(f)).expect("readable"))
        .collect();
    assert!(
        response.contains("Disney+"),
        "and the response holds exactly what came back"
    );
    let claims = std::fs::read_to_string(run_dir.path.join("claims.json"))
        .expect("the lifecycle is beside its raw exchanges");
    assert!(claims.contains("kettle/claim-traces@0"));
    assert!(claims.contains("classification"));
}

#[test]
fn records_what_went_in_so_a_run_can_be_explained() {
    let scratch = Scratch::new("run-dir-inputs");
    let run_dir = RunDir::create(&scratch.0, "run-02").expect("created");
    run_dir
        .record_inputs(&[statement()])
        .expect("inputs recorded");

    let recorded = std::fs::read_to_string(run_dir.path.join("run.json")).expect("run.json");
    assert!(recorded.contains("statement-02-messy.csv"));
    assert!(
        recorded.contains("blake3:"),
        "the same hash the results cache keys on: {recorded}"
    );
}

#[test]
fn outputs_land_beside_the_evidence_that_produced_them() {
    let scratch = Scratch::new("run-dir-outputs");
    let run_dir = RunDir::create(&scratch.0, "run-03").expect("created");
    run_dir
        .write_output("results.json", "{\"schema\":\"kettle/run-report@0\"}")
        .expect("written");

    let written = std::fs::read_to_string(run_dir.path.join("results.json")).expect("results.json");
    assert!(written.contains("kettle/run-report@0"));
}

#[test]
fn delete_everything_is_one_call() {
    let scratch = Scratch::new("run-dir-delete");
    let run_dir = RunDir::create(&scratch.0, "run-04").expect("created");
    let path = run_dir.path.clone();
    run_dir.write_output("results.json", "{}").expect("written");
    run_dir.record_claims(&[]).expect("claim evidence written");

    run_dir.delete().expect("deleted");
    assert!(!path.exists(), "nothing of the run survives");
}

/// #118: a completed or errored run keeps its directory, so a later run
/// handed the same id must be refused rather than write over the
/// evidence that explains an earlier answer. Creation is atomic —
/// `create_dir`, not check-then-create — so two racing callers cannot
/// both believe they own the directory.
#[test]
fn create_refuses_a_directory_that_already_exists() {
    let scratch = Scratch::new("run-dir-existing");
    let first = RunDir::create(&scratch.0, "run-05").expect("created");
    first
        .write_output("results.json", "{\"from\":\"the earlier run\"}")
        .expect("written");

    assert!(
        matches!(
            RunDir::create(&scratch.0, "run-05"),
            Err(RunDirError::AlreadyExists(_))
        ),
        "an existing run directory is never reused"
    );

    let kept = std::fs::read_to_string(scratch.0.join("run-05/results.json")).expect("still there");
    assert_eq!(
        kept, "{\"from\":\"the earlier run\"}",
        "the earlier run's evidence is byte-for-byte unchanged"
    );
}

#[test]
fn a_run_id_can_never_escape_its_root() {
    let scratch = Scratch::new("run-dir-escape");
    for hostile in ["../elsewhere", "/etc", "a/b", ""] {
        assert!(
            matches!(
                RunDir::create(&scratch.0, hostile),
                Err(RunDirError::UnsafeRunId(_))
            ),
            "{hostile:?} should be refused before anything is created"
        );
    }
}
