//! Command-level corpus execution and exact disk replay, without weights.
#[path = "../../runner/tests/support/mod.rs"]
mod support;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-corpus-cli-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn corpus(dir: &Path, single: bool) -> Value {
    let mut value: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("evals/corpus/slice-01.json")).unwrap(),
    )
    .unwrap();
    if single {
        value["cases"].as_array_mut().unwrap().truncate(1);
    }
    std::fs::write(dir.join("corpus.json"), value.to_string()).unwrap();
    value
}
fn invoke(dir: &Path, out: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kettle"))
        .args(["corpus", "--corpus"])
        .arg(dir.join("corpus.json"))
        .arg("--inventory-dir")
        .arg(root().join("evals/capabilities"))
        .arg("--pack-dir")
        .arg(if dir.join("pack").exists() {
            dir.join("pack")
        } else {
            root().join("packs/app.kttl.letter-to-actions")
        })
        .arg("--out")
        .arg(dir.join(out))
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn linked_diagnostic_runs_every_case_and_replays_without_inventing_model_coverage() {
    let dir = workspace("diagnostic");
    let corpus: runner::eval::corpus::Corpus = runner::eval::corpus::Corpus::parse(include_str!(
        "../../../evals/corpus/diagnostic-01.json"
    ))
    .unwrap();
    std::fs::copy(
        root().join("evals/corpus/diagnostic-01.json"),
        dir.join("corpus.json"),
    )
    .unwrap();
    let responses = corpus.cases.iter().map(|case| {
        let segments = runner::document::segments_from_text(&case.passages.join("\n\n"));
        assert_eq!(segments.len(), case.passages.len(), "{}", case.id);
        let results: Vec<_> = segments.iter().enumerate().map(|(id, segment)| {
            let asks: Vec<_> = case.asks.iter().filter(|ask| ask.passage == id
                && ask.status == runner::eval::corpus::AskStatus::Obligation
                && case.coverage[0].inventory_id != "date-form-001").map(|ask| {
                let read = |field: &str| {
                    ask.fields.get(field).and_then(|f| f.as_deref()).and_then(|f| case.span(f))
                        .map(|span| json!({"at":span.passage,"value":span.text}))
                        .unwrap_or_else(|| json!({"at":id,"value":""}))
                };
                let amount = if case.coverage[0].inventory_id == "money-form-013" {
                    json!({"at":2,"value":"£3,052.90"})
                } else { read("amount") };
                json!({"kind":ask.kind,"party":read("party"),"ask":"Act as requested",
                    "amount":amount,"deadline":{"at":ask.deadline_at.unwrap_or(id),
                        "value":ask.deadline_words.clone().unwrap_or_default(),
                        "read":ask.deadline_read,"from":read("base")}})
            }).collect();
            let asks = if id == 2 && case.coverage[0].inventory_id == "obligation-form-006" {
                vec![json!({"kind":"response","party":{"at":1,"value":"Example Services"},
                    "ask":"Tell your tenants","amount":{"at":2,"value":""},
                    "deadline":{"at":2,"value":"","read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":2,"value":""}}})]
            } else { asks };
            json!({"id":id,"segment":segment.text,"confidence":"high","obligations":asks})
        }).collect();
        ("200 OK", support::completion_envelope(&json!({"results":results}).to_string()))
    }).collect();
    let mock = support::MockModel::respond_sequence(responses);
    assert_ok(&invoke(
        &dir,
        "live",
        &["--mock-port", &mock.port().to_string()],
    ));
    let live = report(&dir, "live");
    assert_eq!(live["cases"].as_array().unwrap().len(), 33);
    assert_eq!(live["unscored_cases"], 0);
    assert_eq!(live["summary"]["items"], 28);
    assert_eq!(live["summary"]["no_obligation_sites"], 7);
    assert_eq!(live["summary"]["missed_raw"], 1);
    assert_eq!(live["summary"]["invented_raw"], 1);
    assert_eq!(live["summary"]["raw"]["amount"]["wrong"], 1);
    assert_eq!(live["selection"]["purpose"], "diagnostic");
    assert!(live["model"].is_null());
    for _ in &corpus.cases {
        let request = mock.request_body();
        assert!(
            !request.contains("inventory_digest")
                && !request.contains("-ask-1")
                && !request.contains("-dateline")
        );
    }
    assert_ok(&invoke(
        &dir,
        "replayed",
        &["--replay", dir.join("live").to_str().unwrap()],
    ));
    let replayed = report(&dir, "replayed");
    assert_eq!(live["summary"], replayed["summary"]);
    assert_eq!(replayed["replay"]["legacy_prompt_only_requests"], 0);
    let coverage = Command::new("python3")
        .arg("-B")
        .arg(root().join("scripts/capability-coverage.py"))
        .arg("--recording")
        .arg(dir.join("replayed"))
        .arg("--kettle")
        .arg(env!("CARGO_BIN_EXE_kettle"))
        .output()
        .unwrap();
    assert!(!coverage.status.success());
    assert!(String::from_utf8_lossy(&coverage.stderr).contains("cannot establish model evidence"));
    let mut stale: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("corpus.json")).unwrap()).unwrap();
    stale["cases"][0]["coverage"][0]["inventory_digest"] = json!("sha256:stale");
    std::fs::write(dir.join("corpus.json"), stale.to_string()).unwrap();
    let refused = invoke(&dir, "stale", &["--no-model"]);
    assert!(!refused.status.success());
    assert!(
        !dir.join("stale").exists(),
        "stale registry rejected before execution"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
fn report(dir: &Path, out: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(out).join("report.json")).unwrap())
        .unwrap()
}
fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn format_bundle(dir: &Path) -> PathBuf {
    let bundle = root().join("evals/corpus/formats-01");
    std::fs::copy(bundle.join("corpus.json"), dir.join("corpus.json")).unwrap();
    bundle
}

fn assert_reader_attribution(dir: &Path, out: &str) {
    let report = report(dir, out);
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("corpus.json")).unwrap()).unwrap();
    for (case, truth) in report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(corpus["cases"].as_array().unwrap())
    {
        assert!(case["execution_error"].is_null(), "{case}");
        let words = |parts: &[Value], key: Option<&str>| {
            parts
                .iter()
                .map(|p| key.map(|k| &p[k]).unwrap_or(p).as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            words(case["segments"].as_array().unwrap(), Some("text")),
            words(truth["passages"].as_array().unwrap(), None)
        );
        if case["score"].is_null() {
            assert!(
                case["acquisition_errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e
                        .as_str()
                        .unwrap()
                        .contains("merged authored passage boundaries")),
                "{case}"
            );
        } else {
            assert!(case["acquisition_errors"].as_array().unwrap().is_empty());
        }
        let page = if case["case"] == "format01-three-pages" {
            3
        } else {
            1
        };
        assert_eq!(
            case["segments"].as_array().unwrap().last().unwrap()["page"],
            page
        );
        assert!(
            case["exchanges"].as_array().unwrap().is_empty(),
            "reader check uses no model"
        );
    }
}

#[test]
fn format_pages_preserve_late_asks_and_missing_reordered_or_extra_prose_is_unscored() {
    let dir = workspace("format-pages");
    let bundle = format_bundle(&dir);
    assert_ok(&invoke(
        &dir,
        "text",
        &[
            "--no-model",
            "--bindings",
            bundle.join("text.bindings.json").to_str().unwrap(),
        ],
    ));
    let text = report(&dir, "text");
    assert_eq!(text["summary"]["items"], 4);
    assert_eq!(text["summary"]["no_obligation_sites"], 2);
    assert_reader_attribution(&dir, "text");
    let original: Value =
        serde_json::from_str(&std::fs::read_to_string(bundle.join("text.bindings.json")).unwrap())
            .unwrap();
    for mutation in ["missing", "reordered", "extra"] {
        let mut bindings = original.clone();
        for case in bindings.as_object_mut().unwrap().values_mut() {
            for file in case["inputs"]["letter"].as_array_mut().unwrap() {
                *file = json!(bundle.join(file.as_str().unwrap()));
            }
        }
        let pages = bindings["format01-three-pages"]["inputs"]["letter"]
            .as_array_mut()
            .unwrap();
        if mutation == "missing" {
            pages.remove(1);
        } else if mutation == "reordered" {
            pages.reverse();
        } else {
            let extra = dir.join("extra-prose.txt");
            let mut contents = std::fs::read_to_string(pages[2].as_str().unwrap()).unwrap();
            contents.push_str("\n\nPlease send another payment of £80.00.\n");
            std::fs::write(&extra, contents).unwrap();
            pages[2] = json!(extra);
        }
        let path = dir.join("bindings.json");
        std::fs::write(&path, bindings.to_string()).unwrap();
        let outcome = invoke(
            &dir,
            mutation,
            &["--no-model", "--bindings", path.to_str().unwrap()],
        );
        assert_eq!(outcome.status.code(), Some(2));
        let result = report(&dir, mutation);
        assert_eq!(result["unscored_cases"], 1);
        assert_eq!(result["summary"]["items"], 2);
        let broken = &result["cases"][1];
        assert!(broken["score"].is_null());
        if mutation == "extra" {
            assert!(broken["acquisition_errors"]
                .to_string()
                .contains("outside the authored passages"));
        }
    }
    // Ordinary iteration must refuse a challenge before creating output.
    let mut challenge: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("corpus.json")).unwrap()).unwrap();
    challenge["selection"]["purpose"] = json!("challenge");
    challenge["selection"]["exposure"] = json!("unexposed");
    std::fs::write(dir.join("corpus.json"), challenge.to_string()).unwrap();
    assert!(!invoke(&dir, "challenge", &["--no-model"]).status.success());
    assert!(!dir.join("challenge").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(feature = "pdf")]
fn real_pdf_formats_preserve_words_and_report_unavailable_ask_attribution() {
    if !runner::pdf::library_present(&root().join("sidecars")) {
        eprintln!("skipping: no libpdfium in sidecars/ — see sidecars/README.md");
        return;
    }
    let dir = workspace("format-pdf");
    let bundle = format_bundle(&dir);
    let output = invoke(
        &dir,
        "pdf",
        &[
            "--no-model",
            "--bindings",
            bundle.join("pdf.bindings.json").to_str().unwrap(),
            "--sidecars-dir",
            root().join("sidecars").to_str().unwrap(),
        ],
    );
    assert!(matches!(output.status.code(), Some(0 | 2)));
    assert_reader_attribution(&dir, "pdf");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_os = "macos", feature = "vision"))]
fn real_photo_formats_preserve_words_and_report_unavailable_ask_attribution() {
    let dir = workspace("format-photo");
    let bundle = format_bundle(&dir);
    let output = invoke(
        &dir,
        "photos",
        &[
            "--no-model",
            "--bindings",
            bundle.join("photos.bindings.json").to_str().unwrap(),
        ],
    );
    assert!(matches!(output.status.code(), Some(0 | 2)));
    assert_reader_attribution(&dir, "photos");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The test endpoint's answer; truth is never fed to the production adapter.
fn answer(case: &Value, wrong_sum: bool, omit_attendance: bool, invented: bool) -> String {
    let letter = case["id"] == "slice01-letter";
    let pay = if letter { 3 } else { 5 };
    let attend = if letter { 5 } else { 6 };
    let conditional = if letter { 4 } else { 7 };
    let text = case["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n\n");
    let segments = runner::document::segments_from_text(&text);
    let results: Vec<_> = segments.iter().enumerate().map(|(id, segment)| {
        let obligations = if id == pay {
            vec![json!({"kind":"payment", "party":{"at":1,"value":"Redhill District Council"},
                "ask":"Pay the first instalment", "amount":{"at":if wrong_sum {if letter {2} else {4}} else {pay},"value":if wrong_sum {"£3,052.90"} else {"£305.29"}},
                "deadline":{"at":pay,"value":if letter {"within 14 days of the date of this letter"} else {"due 24/03/2026"},
                    "read":{"count":if letter {14} else {0},"unit":if letter {"days"} else {"none"},"qualifier":"none","counts_from":if letter {"letter_date"} else {"none"}},
                    "from":{"at":0,"value":if letter {"10 March 2026"} else {""}}}})]
        } else if id == attend && !omit_attendance {
            vec![json!({"kind":"attendance", "party":{"at":1,"value":"Redhill District Council"},"ask":"Attend the appointment",
                "amount":{"at":id,"value":""},"deadline":{"at":id,"value":if letter {"2 April 2026"} else {"02/04/2026"},
                "read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":id,"value":""}}})]
        } else if id == conditional && invented {
            vec![json!({"kind":"response", "party":{"at":1,"value":"Redhill District Council"},"ask":"Tell the tenants",
                "amount":{"at":id,"value":""},"deadline":{"at":id,"value":"", "read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":id,"value":""}}})]
        } else { vec![] };
        json!({"id":id,"segment":segment.text,"confidence":"high","obligations":obligations})
    }).collect();
    support::completion_envelope(&json!({"results":results}).to_string())
}

#[test]
fn corpus_command_keeps_wrong_missing_and_invented_answers_and_replays_them() {
    let dir = workspace("roundtrip");
    let c = corpus(&dir, false);
    let mock = support::MockModel::respond_sequence(vec![
        ("200 OK", answer(&c["cases"][0], true, true, true)),
        ("200 OK", answer(&c["cases"][1], false, false, false)),
    ]);
    assert_ok(&invoke(
        &dir,
        "live",
        &["--mock-port", &mock.port().to_string()],
    ));
    let live = report(&dir, "live");
    assert_eq!(live["answer_source"], "controlled-endpoint");
    assert_eq!(live["summary"]["items"], 4);
    assert_eq!(live["summary"]["raw"]["amount"]["wrong"], 1);
    assert_eq!(live["summary"]["verified"]["amount"]["wrong"], 1);
    assert_eq!(live["summary"]["missed_raw"], 1);
    assert_eq!(live["summary"]["invented_raw"], 1);
    assert_eq!(live["summary"]["raw"]["time"]["unsupported"], 4);
    for _ in 0..2 {
        let request = mock.request_body();
        assert!(!request.contains("fact-instalment") && !request.contains("ask-pay"));
    }
    assert_ok(&invoke(
        &dir,
        "replay",
        &["--replay", dir.join("live").to_str().unwrap()],
    ));
    let replay = report(&dir, "replay");
    assert_eq!(live["summary"], replay["summary"]);
    assert_eq!(replay["replay"]["legacy_prompt_only_requests"], 0);
    assert_eq!(replay["replay"]["exact_requests"], 2);
    assert!(
        replay["model"].is_null() && replay["generation_machine"].is_null(),
        "a mock recording is not model evidence"
    );
    assert!(
        !invoke(&dir, "live", &["--no-model"]).status.success(),
        "existing evidence is never overwritten"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordered_pages_keep_real_coordinates_and_truth_changes_rescore_recorded_answers() {
    let dir = workspace("pages");
    let mut c = corpus(&dir, true);
    let mock =
        support::MockModel::respond_once("200 OK", answer(&c["cases"][0], false, false, false));
    assert_ok(&invoke(
        &dir,
        "one",
        &["--mock-port", &mock.port().to_string()],
    ));
    let parts = c["cases"][0]["passages"].as_array().unwrap();
    for (name, passages) in [("front.txt", &parts[..3]), ("back.txt", &parts[3..])] {
        std::fs::write(
            dir.join(name),
            passages
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
        .unwrap();
    }
    let binding = dir.join("bindings.json");
    std::fs::write(
        &binding,
        json!({"slice01-letter":{"inputs":{"letter":["front.txt","back.txt"]}}}).to_string(),
    )
    .unwrap();
    assert_ok(&invoke(
        &dir,
        "pages",
        &[
            "--replay",
            dir.join("one").to_str().unwrap(),
            "--bindings",
            binding.to_str().unwrap(),
        ],
    ));
    let one = report(&dir, "one");
    let pages = report(&dir, "pages");
    assert_eq!(one["summary"], pages["summary"]);
    assert_eq!(one["cases"][0]["segments"][3]["page"], 1);
    assert_eq!(pages["cases"][0]["segments"][3]["page"], 2);
    assert_ne!(one["cases"][0]["inputs"], pages["cases"][0]["inputs"]);
    c["facts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["id"] == "fact-instalment")
        .unwrap()["value"]["currency"] = json!("EUR");
    std::fs::write(dir.join("corpus.json"), c.to_string()).unwrap();
    assert_ok(&invoke(
        &dir,
        "rescored",
        &["--replay", dir.join("one").to_str().unwrap()],
    ));
    let rescored = report(&dir, "rescored");
    assert_ne!(one["corpus_digest"], rescored["corpus_digest"]);
    assert_eq!(rescored["summary"]["verified"]["amount"]["wrong"], 1);
    assert_eq!(rescored["summary"]["raw"]["amount"]["correct"], 2);
    // Changing acquired bytes changes the request; no old score or answer fits.
    std::fs::write(dir.join("back.txt"), "A changed page.").unwrap();
    let changed = invoke(
        &dir,
        "changed",
        &[
            "--replay",
            dir.join("one").to_str().unwrap(),
            "--bindings",
            binding.to_str().unwrap(),
        ],
    );
    assert_eq!(changed.status.code(), Some(2));
    let failed = report(&dir, "changed");
    assert!(failed["cases"][0]["execution_error"]
        .as_str()
        .unwrap()
        .contains("no recorded answer"));
    assert_eq!(failed["unscored_cases"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_acquisition_is_unscored_and_malformed_answers_remain_visible() {
    let dir = workspace("failures");
    let c = corpus(&dir, true);
    let invalid = support::completion_envelope("{\"results\":\"broken\"}");
    let mock = support::MockModel::respond_sequence(vec![
        ("200 OK", invalid.clone()),
        ("200 OK", invalid),
    ]);
    assert_ok(&invoke(
        &dir,
        "malformed",
        &["--mock-port", &mock.port().to_string()],
    ));
    let malformed = report(&dir, "malformed");
    assert_eq!(malformed["summary"]["items"], 2);
    assert_eq!(malformed["summary"]["missed_raw"], 2);
    assert!(!malformed["cases"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        malformed["cases"][0]["exchanges"].as_array().unwrap().len(),
        2
    );
    let mock = support::MockModel::respond_sequence(vec![
        (
            "200 OK",
            support::completion_envelope("{\"results\":\"broken\"}"),
        ),
        ("200 OK", answer(&c["cases"][0], false, false, false)),
    ]);
    assert_ok(&invoke(
        &dir,
        "retry",
        &["--mock-port", &mock.port().to_string()],
    ));
    assert_eq!(report(&dir, "retry")["summary"]["whole_item_raw"], 2);
    std::fs::write(dir.join("missing.txt"), "Only an unrelated page.").unwrap();
    let bindings = dir.join("bindings.json");
    std::fs::write(
        &bindings,
        json!({"slice01-letter":{"inputs":{"letter":"missing.txt"}}}).to_string(),
    )
    .unwrap();
    assert_eq!(
        invoke(
            &dir,
            "acquisition",
            &["--no-model", "--bindings", bindings.to_str().unwrap()]
        )
        .status
        .code(),
        Some(2)
    );
    let acquired = report(&dir, "acquisition");
    assert_eq!(acquired["unscored_cases"], 1);
    assert_eq!(acquired["summary"]["items"], 0);
    assert!(acquired["cases"][0]["score"].is_null());
    std::fs::remove_dir_all(dir).unwrap();
}

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
fn unpaired_claims_stay_visible_and_changed_schema_cannot_replay() {
    let dir = workspace("identity");
    let c = corpus(&dir, true);
    copy_pack(
        &root().join("packs/app.kttl.letter-to-actions"),
        &dir.join("pack"),
    );
    let mut envelope: Value =
        serde_json::from_str(&answer(&c["cases"][0], false, false, false)).unwrap();
    let mut body: Value = serde_json::from_str(
        envelope["choices"][0]["message"]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let mut extra = body["results"][3].clone();
    extra["id"] = json!(999);
    body["results"].as_array_mut().unwrap().push(extra);
    envelope["choices"][0]["message"]["content"] = json!(body.to_string());
    let mock = support::MockModel::respond_once("200 OK", envelope.to_string());
    assert_ok(&invoke(
        &dir,
        "recorded",
        &["--mock-port", &mock.port().to_string()],
    ));
    let read = report(&dir, "recorded");
    assert_eq!(read["summary"]["invented_raw"], 1);
    assert_eq!(read["summary"]["invented_verified"], 0);
    assert!(!read["cases"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    let schema_path = dir.join("pack/schemas/obligations.schema.json");
    let mut schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_path).unwrap()).unwrap();
    schema["properties"]["results"]["items"]["properties"]["confidence"]["enum"]
        .as_array_mut()
        .unwrap()
        .push(json!("unknown"));
    std::fs::write(schema_path, schema.to_string()).unwrap();
    assert_eq!(
        invoke(
            &dir,
            "incompatible",
            &["--replay", dir.join("recorded").to_str().unwrap()]
        )
        .status
        .code(),
        Some(2)
    );
    let refused = report(&dir, "incompatible");
    assert_ne!(read["pipeline_digest"], refused["pipeline_digest"]);
    assert!(refused["cases"][0]["execution_error"]
        .as_str()
        .unwrap()
        .contains("no recorded answer"));
    // Invalid corpus identities must be refused before any case is executed.
    let mut duplicate = c.clone();
    duplicate["cases"]
        .as_array_mut()
        .unwrap()
        .push(c["cases"][0].clone());
    std::fs::write(dir.join("corpus.json"), duplicate.to_string()).unwrap();
    let invalid = invoke(&dir, "duplicate", &["--no-model"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("duplicate case"));
    assert!(!dir.join("duplicate").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
