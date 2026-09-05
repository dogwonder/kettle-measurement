//! #332: a pack's declared inputs stop being decorative.
//!
//! `InputSpec { role, accept, multiple }` was parsed and never used to
//! validate anything. That is harmless while every pack declares one
//! role — the binding of file to role is then unambiguous — and it is
//! the whole problem the moment a pack declares two, which #66 does.

mod support;

use runner::packs::load_pack;
use runner::run::{run_pack, run_pack_bound, Answers, InputBindingError, RunError};
use runner::run_dir::NoLog;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, per_batch, MockModel};

const LAST_YEAR: &str = "1 August 2025\n\nAnnual premium £412.00.";
const THIS_YEAR: &str = "1 August 2026\n\nAnnual premium £459.00.";

/// A pack declaring two distinct roles, one document each — the shape
/// #66 needs and the manifest could not previously express.
fn renewal_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-roles-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write pack file");
    };
    write(
        "pack.json",
        r#"{
          "id": "app.kttl.test-renewal",
          "name": "Renewal test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [
            { "role": "previous", "label": "Last year's policy", "accept": ["text/plain"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["text/plain"], "multiple": false }
          ],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 },
            { "step": "render", "template": "report.html.tera" }
          ],
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do, and by when?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "enum": ["payment", "response", "attendance", "other"] },
                    "party": { "type": "string" },
                    "ask": { "type": "string" },
                    "deadline": { "type": "string" },
                    "anchor": { "type": "string" }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/last-year.txt", LAST_YEAR);
    write("fixtures/this-year.txt", THIS_YEAR);
    dir
}

/// No obligations — this pack is being run for its input binding, not
/// its extraction. The segments are still echoed verbatim, in the order
/// the documents were bound, because the rejoin pairs on that echo
/// (#312) and an unpairable answer would retry into a spent mock.
fn answer_for(documents: [&str; 2]) -> String {
    let segments: Vec<&str> = documents
        .iter()
        .flat_map(|document| document.split("\n\n"))
        .collect();
    completion_envelope(
        &serde_json::json!({
            "results": segments.iter().enumerate().map(|(id, segment)| serde_json::json!({
                "id": id, "segment": segment, "confidence": "high", "obligations": []
            })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// A role carries the words a person is shown (#334 item 3).
///
/// `role` is a machine name the pipeline binds on — `previous` is a key,
/// not copy. Two labelled drop zones is how someone says which document
/// is last year's, and nothing in the manifest could say what to write
/// above them. Asking the app to prettify `previous` would put product
/// copy in the shell, where no pack author can see or change it.
#[test]
fn a_role_says_what_to_call_it() {
    let dir = renewal_pack("labels");
    let pack = load_pack(&dir).expect("a two-role pack loads");

    let labels: Vec<&str> = pack
        .manifest
        .inputs
        .iter()
        .map(|input| input.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Last year's policy", "This year's renewal"]);
}

/// The flat API cannot serve a two-role pack, and must say so rather
/// than bind positionally.
///
/// Order is invisible at the call site and unverifiable afterwards, and
/// getting it wrong does not fail — it produces a *reversed* diff. On a
/// renewal that reports a price cut where there was a rise, which is
/// worse than any error message.
#[test]
fn a_two_role_pack_cannot_be_run_through_the_single_role_api() {
    let dir = renewal_pack("flat-refused");
    let pack = load_pack(&dir).expect("a two-role pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer_for([LAST_YEAR, THIS_YEAR]))]);

    let error = run_pack(
        &pack,
        &[
            dir.join("fixtures/last-year.txt"),
            dir.join("fixtures/this-year.txt"),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect_err("a two-role pack has no unambiguous flat binding");

    let RunError::InputBinding(InputBindingError::RoleUnstated { declared }) = error else {
        panic!("expected RoleUnstated, got: {error}");
    };
    assert_eq!(declared, vec!["previous".to_owned(), "renewal".to_owned()]);
}

/// Each role binds to its own document, and the run says which is which.
///
/// #330 gave every segment a `document` index; this is what gives that
/// index a meaning a later step can act on.
#[test]
fn each_role_binds_to_its_own_document() {
    let dir = renewal_pack("binds");
    let pack = load_pack(&dir).expect("a two-role pack loads");
    // Bound renewal-first below, so that is the order the segments arrive in.
    let mock = MockModel::respond_sequence(vec![("200 OK", answer_for([THIS_YEAR, LAST_YEAR]))]);

    let outcome = run_pack_bound(
        &pack,
        &[
            ("renewal", dir.join("fixtures/this-year.txt")),
            ("previous", dir.join("fixtures/last-year.txt")),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("both roles are satisfied");

    // Bound by name, never by position: the call above supplies them in
    // the opposite order to the declaration on purpose.
    let roles: Vec<&str> = outcome.inputs.iter().map(|i| i.role.as_str()).collect();
    assert_eq!(roles, vec!["renewal", "previous"]);
}

/// A role the pack declares and the run never supplies is refused
/// before any model call.
#[test]
fn a_missing_role_is_refused() {
    let dir = renewal_pack("missing");
    let pack = load_pack(&dir).expect("a two-role pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer_for([LAST_YEAR, THIS_YEAR]))]);

    let error = run_pack_bound(
        &pack,
        &[("previous", dir.join("fixtures/last-year.txt"))],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect_err("a renewal diff with nothing to diff against is not a run");

    let RunError::InputBinding(InputBindingError::MissingRole { role }) = error else {
        panic!("expected MissingRole, got: {error}");
    };
    assert_eq!(role, "renewal");
}

/// `"multiple": false` means one, and a second file for that role is a
/// mistake worth stopping for rather than quietly ignoring.
#[test]
fn a_second_file_for_a_single_role_is_refused() {
    let dir = renewal_pack("too-many");
    let pack = load_pack(&dir).expect("a two-role pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer_for([LAST_YEAR, THIS_YEAR]))]);

    let error = run_pack_bound(
        &pack,
        &[
            ("previous", dir.join("fixtures/last-year.txt")),
            ("previous", dir.join("fixtures/this-year.txt")),
            ("renewal", dir.join("fixtures/this-year.txt")),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect_err("two documents cannot both be last year's");

    let RunError::InputBinding(InputBindingError::TooMany { role, given, .. }) = error else {
        panic!("expected TooMany, got: {error}");
    };
    assert_eq!((role.as_str(), given), ("previous", 2));
}

/// A role the pack never declared is refused, rather than read as if it
/// were one of the declared ones.
#[test]
fn an_undeclared_role_is_refused() {
    let dir = renewal_pack("undeclared");
    let pack = load_pack(&dir).expect("a two-role pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer_for([LAST_YEAR, THIS_YEAR]))]);

    let error = run_pack_bound(
        &pack,
        &[
            ("previous", dir.join("fixtures/last-year.txt")),
            ("renewal", dir.join("fixtures/this-year.txt")),
            ("schedule", dir.join("fixtures/this-year.txt")),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect_err("a role the pack never declared means the caller is confused");

    let RunError::InputBinding(InputBindingError::UndeclaredRole { role }) = error else {
        panic!("expected UndeclaredRole, got: {error}");
    };
    assert_eq!(role, "schedule");
}

/// A single-role pack keeps the flat API, and every file binds to the
/// one role it declares. This is the path all twenty existing call
/// sites take, and it must not have moved.
#[test]
fn a_single_role_pack_still_takes_a_flat_list() {
    let dir = renewal_pack("single");
    // Rewrite the manifest to one role, leaving everything else alone.
    let manifest = std::fs::read_to_string(dir.join("pack.json")).expect("read manifest");
    let single = manifest.replace(
        r#"{ "role": "previous", "label": "Last year's policy", "accept": ["text/plain"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["text/plain"], "multiple": false }"#,
        r#"{ "role": "policy", "label": "Your policy", "accept": ["text/plain"], "multiple": true }"#,
    );
    assert_ne!(manifest, single, "the manifest rewrite must actually apply");
    std::fs::write(dir.join("pack.json"), single).expect("write manifest");

    let pack = load_pack(&dir).expect("a one-role pack loads");
    // Two documents under one role are two batches (#624), unlike the
    // role-bound pair every other test here runs.
    let mock = MockModel::respond_sequence(per_batch(
        &answer_for([LAST_YEAR, THIS_YEAR]),
        &[LAST_YEAR.split("\n\n").count()],
    ));

    let outcome = run_pack(
        &pack,
        &[
            dir.join("fixtures/last-year.txt"),
            dir.join("fixtures/this-year.txt"),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("one declared role binds every file unambiguously");

    let roles: Vec<&str> = outcome.inputs.iter().map(|i| i.role.as_str()).collect();
    assert_eq!(roles, vec!["policy", "policy"]);
    // The index into `inputs` is the `Segment::document` index from #330,
    // so a later step can ask which document a value came out of.
    assert_eq!(outcome.inputs[0].file, "last-year.txt");
}
