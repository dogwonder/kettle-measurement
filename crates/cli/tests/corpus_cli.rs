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
    for case in live["cases"].as_array().unwrap() {
        assert_eq!(
            case["score"]["task_slots"]["semantic_action_coverage"],
            "not-assessed"
        );
        for candidate in case["raw"]["asks"].as_array().unwrap() {
            assert!(candidate["text"].as_str().is_some());
        }
        for candidate in case["verified"].as_array().unwrap() {
            assert!(candidate["text"].as_str().is_some());
        }
    }
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

/// Fixed source truth exercises the instrument with both faithful and wrong
/// controlled readings. None of these answers is model-capability evidence.
#[test]
fn product_regressions_keep_wrong_fact_selection_and_heading_duplicates_visible() {
    let dir = workspace("product-regressions");
    let source = include_str!("../../../evals/corpus/product-regressions-01.json");
    let corpus = runner::eval::corpus::Corpus::parse(source).unwrap();
    std::fs::write(dir.join("corpus.json"), source).unwrap();
    for wrong in [false, true] {
        let responses = corpus.cases.iter().map(|case| {
            let segments = runner::document::segments_from_text(&case.passages.join("\n\n"));
            assert_eq!(segments.len(), case.passages.len(), "{}", case.id);
            let results: Vec<_> = segments.iter().enumerate().map(|(id, segment)| {
                let mut asks: Vec<_> = case.asks.iter().filter(|ask| ask.passage == id
                    && ask.status == runner::eval::corpus::AskStatus::Obligation).map(|ask| {
                    let read = |field: &str| {
                        ask.fields.get(field).and_then(|f| f.as_deref()).and_then(|f| case.span(f))
                            .map(|span| json!({"at":span.passage,"value":span.text}))
                            .unwrap_or_else(|| json!({"at":id,"value":""}))
                    };
                    let mut amount = read("amount");
                    if wrong {
                        // Deliberately select the other printed fact: containment
                        // must not turn this into a source-truth success.
                        if let Some(relation) = corpus.relations.iter().find(|r| r.ask == ask.id) {
                            let span = case.span(&relation.distractor).unwrap();
                            amount = json!({"at":span.passage,"value":span.text});
                        }
                    }
                    json!({"kind":ask.kind,"party":read("party"),
                        "ask":format!("Controlled action {}", ask.id),
                        "amount":amount,"deadline":{"at":ask.deadline_at.unwrap_or(id),
                            "value":ask.deadline_words.clone().unwrap_or_default(),
                            "read":ask.deadline_read,"from":read("base")}})
                }).collect();
                if wrong && case.id == "split-list" && id == 1 {
                    asks.push(json!({"kind":"response", "party":{"at":0,"value":"Cedar Account Services"},
                        "ask":"Return the permit application and send the meter photograph",
                        "amount":{"at":1,"value":""},"deadline":{"at":1,"value":"Before 19 November 2026",
                        "read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},
                        "from":{"at":1,"value":""}}}));
                }
                json!({"id":id,"segment":segment.text,"confidence":"high","obligations":asks})
            }).collect();
            ("200 OK", support::completion_envelope(&json!({"results":results}).to_string()))
        }).collect();
        let mock = support::MockModel::respond_sequence(responses);
        let out = if wrong { "wrong" } else { "faithful" };
        assert_ok(&invoke(
            &dir,
            out,
            &["--mock-port", &mock.port().to_string()],
        ));
        let measured = report(&dir, out);
        assert_eq!(measured["unscored_cases"], 0);
        assert_eq!(measured["summary"]["items"], 12);
        for side in ["raw", "verified"] {
            assert_eq!(
                measured["summary"][side]["amount"]["wrong"],
                if wrong { 3 } else { 0 }
            );
            assert_eq!(
                measured["summary"][format!("invented_{side}")],
                if wrong { 1 } else { 0 }
            );
            assert_eq!(measured["summary"][format!("missed_{side}")], 0);
            assert_eq!(
                measured["summary"][format!("whole_item_{side}")],
                if wrong { 9 } else { 12 }
            );
        }
        for case in measured["cases"].as_array().unwrap() {
            assert_eq!(
                case["score"]["task_slots"]["semantic_action_coverage"],
                "not-assessed"
            );
        }
        // Exact replay preserves the wrong answers as well as the correct ones.
        let replay_out = format!("{out}-replay");
        assert_ok(&invoke(
            &dir,
            &replay_out,
            &["--replay", dir.join(out).to_str().unwrap()],
        ));
        let replay = report(&dir, &replay_out);
        assert_eq!(replay["cases"], measured["cases"]);
        assert_eq!(replay["summary"], measured["summary"]);
        assert_eq!(replay["replay"]["exact_requests"], 10);
        assert_eq!(replay["replay"]["legacy_prompt_only_requests"], 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// These declarations exercise the contract only. This is exposed, derived
/// test data, never evidence of independently authored model capability.
fn frozen_challenge(dir: &Path) -> (Value, String, PathBuf) {
    let mut c = corpus(dir, true);
    c["cases"][0]["passages"][0] = json!(format!(
        "{} Synthetic challenge execution contract fixture.",
        c["cases"][0]["passages"][0].as_str().unwrap()
    ));
    let response = answer(&c["cases"][0], false, false, false);
    c["cases"][0]["id"] = json!("contract-only-challenge");
    c["selection"] = json!({"id":"contract-only-challenge", "purpose":"challenge", "exposure":"unexposed", "fields":["amount", "kind", "deadline", "party"]});
    c["provenance"] = json!({"independent":true,"authoring":{"author":"synthetic test declaration", "relationship":"separate-author", "source_families":[{"id":"contract-test-family","locator":"synthetic test only","relationship":"external-source"}]}});
    std::fs::write(dir.join("corpus.json"), c.to_string()).unwrap();
    let record = dir.join("lifecycle.json");
    assert_ok(
        &Command::new("python3")
            .arg("-B")
            .arg(root().join("scripts/challenge-selection.py"))
            .args(["freeze", "--corpus"])
            .arg(dir.join("corpus.json"))
            .arg("--record")
            .arg(&record)
            .output()
            .unwrap(),
    );
    (c, response, record)
}

#[test]
fn challenge_attempt_consumes_identity_and_exact_replay_retains_exposure() {
    let dir = workspace("challenge-replay");
    let (mut c, response, record) = frozen_challenge(&dir);
    assert!(!invoke(&dir, "ordinary", &["--no-model"]).status.success());
    let mock = support::MockModel::respond_once("200 OK", response);
    assert_ok(&invoke(
        &dir,
        "attempt",
        &[
            "--challenge-record",
            record.to_str().unwrap(),
            "--mock-port",
            &mock.port().to_string(),
        ],
    ));
    let live = report(&dir, "attempt");
    assert_eq!(live["selection"]["purpose"], "challenge");
    assert_eq!(live["answer_source"], "controlled-endpoint");
    assert!(live["model"].is_null());
    assert_eq!(live["summary"]["whole_item_raw"], 2);
    let lifecycle: Value =
        serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    assert_eq!(live["challenge"], lifecycle);
    assert_eq!(lifecycle["events"][1]["event"], "exposed");
    let snapshot: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("attempt/challenge-lifecycle.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot, lifecycle);
    let request = mock.request_body();
    assert!(!request.contains("source_families") && !request.contains("contract-only-challenge"));
    assert!(!invoke(
        &dir,
        "second",
        &["--challenge-record", record.to_str().unwrap(), "--no-model"]
    )
    .status
    .success());
    assert!(!dir.join("second").exists());
    let previous = dir.join("attempt");
    assert!(!invoke(
        &dir,
        "implicit-replay",
        &["--replay", previous.to_str().unwrap()]
    )
    .status
    .success());
    assert_ok(&invoke(
        &dir,
        "replayed",
        &[
            "--challenge-record",
            record.to_str().unwrap(),
            "--replay",
            previous.to_str().unwrap(),
        ],
    ));
    let replayed = report(&dir, "replayed");
    assert_eq!(replayed["answer_source"], "replay");
    assert_eq!(live["summary"], replayed["summary"]);
    assert_eq!(live["challenge"], replayed["challenge"]);
    assert_eq!(replayed["replay"]["legacy_prompt_only_requests"], 0);
    let mut altered = lifecycle.clone();
    altered["events"][1]["reason"] = json!("different exposure");
    std::fs::write(&record, altered.to_string()).unwrap();
    assert!(!invoke(
        &dir,
        "altered-ledger",
        &[
            "--challenge-record",
            record.to_str().unwrap(),
            "--replay",
            previous.to_str().unwrap()
        ]
    )
    .status
    .success());
    std::fs::write(&record, lifecycle.to_string()).unwrap();
    c["slice"] = json!("changed after freezing");
    std::fs::write(dir.join("corpus.json"), c.to_string()).unwrap();
    assert!(!invoke(
        &dir,
        "altered-corpus",
        &[
            "--challenge-record",
            record.to_str().unwrap(),
            "--replay",
            previous.to_str().unwrap()
        ]
    )
    .status
    .success());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn challenge_validation_and_pending_writes_refuse_execution_and_failed_launch_consumes() {
    let dir = workspace("challenge-failures");
    let (c, _, record) = frozen_challenge(&dir);
    let original = std::fs::read_to_string(&record).unwrap();
    let mut invalid = c.clone();
    invalid["cases"][0]["spans"][0]["fact"] = json!("unknown fact");
    std::fs::write(dir.join("corpus.json"), invalid.to_string()).unwrap();
    let refusal = invoke(
        &dir,
        "bad-truth",
        &["--challenge-record", record.to_str().unwrap(), "--no-model"],
    );
    assert!(!refusal.status.success());
    assert!(String::from_utf8_lossy(&refusal.stderr).contains("unknown fact"));
    assert_eq!(std::fs::read_to_string(&record).unwrap(), original);
    std::fs::write(dir.join("corpus.json"), c.to_string()).unwrap();
    let mut invalid: Value = serde_json::from_str(&original).unwrap();
    invalid["authoring"]["author"] = json!("someone else");
    std::fs::write(&record, invalid.to_string()).unwrap();
    assert!(!invoke(
        &dir,
        "bad-author",
        &["--challenge-record", record.to_str().unwrap(), "--no-model"]
    )
    .status
    .success());
    std::fs::write(&record, &original).unwrap();
    let pending = dir.join("lifecycle.json.pending");
    std::fs::write(&pending, "interrupted write").unwrap();
    assert!(!invoke(
        &dir,
        "locked",
        &["--challenge-record", record.to_str().unwrap(), "--no-model"]
    )
    .status
    .success());
    assert_eq!(std::fs::read_to_string(&record).unwrap(), original);
    std::fs::remove_file(pending).unwrap();
    let failed = invoke(
        &dir,
        "failed-launch",
        &[
            "--challenge-record",
            record.to_str().unwrap(),
            "--model",
            dir.join("missing.gguf").to_str().unwrap(),
        ],
    );
    assert!(!failed.status.success());
    let consumed: Value = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    assert_eq!(consumed["events"][1]["event"], "exposed");
    assert!(!invoke(
        &dir,
        "retry-failed",
        &["--challenge-record", record.to_str().unwrap(), "--no-model"]
    )
    .status
    .success());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn concurrent_challenge_launches_cannot_both_consume_the_selection() {
    let dir = workspace("challenge-concurrent");
    let (_, _, record) = frozen_challenge(&dir);
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let run = |out| {
            barrier.wait();
            invoke(
                &dir,
                out,
                &["--challenge-record", record.to_str().unwrap(), "--no-model"],
            )
        };
        let first = scope.spawn(move || run("first"));
        let second = scope.spawn(move || run("second"));
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.status.success()).count(), 1);
    let check = Command::new("python3")
        .arg("-B")
        .arg(root().join("scripts/challenge-selection.py"))
        .args(["check", "--corpus"])
        .arg(dir.join("corpus.json"))
        .arg("--record")
        .arg(record)
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&check.stdout).trim(), "regression");
    std::fs::remove_dir_all(dir).unwrap();
}

fn format_bundle(dir: &Path) -> PathBuf {
    let bundle = root().join("evals/corpus/formats-01");
    std::fs::copy(bundle.join("corpus.json"), dir.join("corpus.json")).unwrap();
    bundle
}

#[test]
fn merged_sites_keep_payment_errors_and_cancelled_inventions_separate() {
    let dir = workspace("merged-attribution");
    format_bundle(&dir);
    let mut corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("corpus.json")).unwrap()).unwrap();
    corpus["cases"].as_array_mut().unwrap().truncate(1);
    std::fs::write(dir.join("corpus.json"), corpus.to_string()).unwrap();
    let parts = corpus["cases"][0]["passages"].as_array().unwrap();
    let merged = parts[..5]
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let footer = parts[5].as_str().unwrap();
    std::fs::write(dir.join("merged.txt"), format!("{merged}\n\n{footer}")).unwrap();
    std::fs::write(
        dir.join("bindings.json"),
        json!({"format01-footer":{"inputs":{"letter":"merged.txt"}}}).to_string(),
    )
    .unwrap();
    let payment = |amount: &str| {
        json!({"kind":"payment","party":{"at":0,"value":"Merefield Housing"},
        "ask":"Pay the first instalment","amount":{"at":0,"value":amount},
        "deadline":{"at":0,"value":"by 2 June 2026","read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":0,"value":""}}})
    };
    let cancelled = json!({"kind":"attendance","party":{"at":0,"value":"Merefield Housing"},
        "ask":"Attend the earlier appointment","amount":{"at":0,"value":""},
        "deadline":{"at":0,"value":"21 May 2026","read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":0,"value":""}}});
    let return_form = json!({"kind":"response","party":{"at":0,"value":"Merefield Housing"},
        "ask":"Return the signed form","amount":{"at":1,"value":""},
        "deadline":{"at":1,"value":"by 28 May 2026","read":{"count":0,"unit":"none","qualifier":"none","counts_from":"none"},"from":{"at":1,"value":""}}});
    for (name, first) in [
        ("correct", vec![payment("£120.00")]),
        ("wrong-total", vec![payment("£1,440.00")]),
        ("cancelled-only", vec![cancelled.clone()]),
        ("both", vec![cancelled.clone(), payment("£120.00")]),
        ("both-reversed", vec![payment("£120.00"), cancelled]),
        ("duplicate", vec![payment("£120.00"), payment("£1,440.00")]),
        (
            "unknown-kind",
            vec![{
                let mut p = payment("£120.00");
                p["kind"] = json!("response");
                p
            }],
        ),
    ] {
        let body = json!({"results":[{"id":0,"segment":merged,"confidence":"high","obligations":first},
            {"id":1,"segment":footer,"confidence":"high","obligations":[return_form.clone()]}]});
        let mock = support::MockModel::respond_once(
            "200 OK",
            support::completion_envelope(&body.to_string()),
        );
        let output = invoke(
            &dir,
            name,
            &[
                "--bindings",
                dir.join("bindings.json").to_str().unwrap(),
                "--mock-port",
                &mock.port().to_string(),
            ],
        );
        let r = report(&dir, name);
        assert!(r["cases"][0]["acquisition_errors"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(r["cases"][0]["kind_matched_segments"], json!([0]));
        if matches!(name, "duplicate" | "unknown-kind") {
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(r["unscored_cases"], 1);
            assert!(r["cases"][0]["score"].is_null());
            assert!(!r["cases"][0]["attribution_errors"]
                .as_array()
                .unwrap()
                .is_empty());
            continue;
        }
        assert_ok(&output);
        let inventions = usize::from(matches!(name, "cancelled-only" | "both" | "both-reversed"));
        assert_eq!(r["summary"]["invented_raw"], inventions);
        assert_eq!(r["summary"]["invented_verified"], inventions);
        assert_eq!(
            r["cases"][0]["score"]["negative_sites"][0]["invented_raw"],
            inventions
        );
        assert_eq!(
            r["summary"]["missed_raw"],
            usize::from(name == "cancelled-only")
        );
        assert_eq!(
            r["summary"]["raw"]["amount"]["wrong"],
            usize::from(name == "wrong-total")
        );
        assert_eq!(
            r["summary"]["verified"]["amount"]["wrong"],
            usize::from(name == "wrong-total")
        );
        let correct_items = if matches!(name, "wrong-total" | "cancelled-only") {
            1
        } else {
            2
        };
        assert_eq!(r["summary"]["whole_item_raw"], correct_items);
        assert_eq!(r["summary"]["whole_item_verified"], correct_items);
        assert_ok(&invoke(
            &dir,
            &format!("{name}-replay"),
            &[
                "--bindings",
                dir.join("bindings.json").to_str().unwrap(),
                "--replay",
                dir.join(name).to_str().unwrap(),
            ],
        ));
        assert_eq!(
            r["summary"],
            report(&dir, &format!("{name}-replay"))["summary"]
        );
    }
    // Several positive kinds can share the acquired segment as well.
    let all = format!("{merged} {footer}");
    std::fs::write(dir.join("merged.txt"), &all).unwrap();
    let mut response = return_form.clone();
    response["deadline"]["at"] = json!(0);
    response["deadline"]["from"]["at"] = json!(0);
    response["amount"]["at"] = json!(0);
    let body = json!({"results":[{"id":0,"segment":all,"confidence":"high","obligations":[response,payment("£120.00")]}]});
    let mock =
        support::MockModel::respond_once("200 OK", support::completion_envelope(&body.to_string()));
    assert_ok(&invoke(
        &dir,
        "all-merged",
        &[
            "--bindings",
            dir.join("bindings.json").to_str().unwrap(),
            "--mock-port",
            &mock.port().to_string(),
        ],
    ));
    let all_report = report(&dir, "all-merged");
    assert_eq!(all_report["summary"]["whole_item_raw"], 2);
    assert_eq!(all_report["summary"]["whole_item_verified"], 2);
    assert_eq!(all_report["summary"]["invented_raw"], 0);
    assert_eq!(
        all_report["cases"][0]["score"]["negative_sites"][0]["invented_raw"],
        0
    );
    // Source sites of the same kind are not distinguishable by this wire
    // contract. Do not pair them by whichever dates/amounts score best.
    corpus["cases"][0]["asks"][2]["kind"] = json!("payment");
    std::fs::write(dir.join("corpus.json"), corpus.to_string()).unwrap();
    let outcome = invoke(
        &dir,
        "overlapping",
        &[
            "--bindings",
            dir.join("bindings.json").to_str().unwrap(),
            "--no-model",
        ],
    );
    assert_eq!(outcome.status.code(), Some(2));
    let overlap = report(&dir, "overlapping");
    assert!(overlap["cases"][0]["score"].is_null());
    assert!(overlap["cases"][0]["attribution_errors"]
        .to_string()
        .contains("overlapping"));
    std::fs::remove_dir_all(dir).unwrap();
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
        assert!(!case["score"].is_null(), "{case}");
        assert!(case["acquisition_errors"].as_array().unwrap().is_empty());
        assert!(case["attribution_errors"].as_array().unwrap().is_empty());
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
fn real_pdf_formats_resolve_distinct_kinds_in_merged_passages() {
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
fn real_photo_formats_resolve_distinct_kinds_in_merged_passages() {
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
    let prompt_path = dir.join("pack/prompts/obligations.md");
    let original_prompt = std::fs::read_to_string(&prompt_path).unwrap();
    std::fs::write(
        &prompt_path,
        format!("{original_prompt}\nA revised asking policy.\n"),
    )
    .unwrap();
    assert_eq!(
        invoke(
            &dir,
            "changed-prompt",
            &["--replay", dir.join("recorded").to_str().unwrap()]
        )
        .status
        .code(),
        Some(2)
    );
    let refused_prompt = report(&dir, "changed-prompt");
    assert_ne!(read["pipeline_digest"], refused_prompt["pipeline_digest"]);
    assert!(refused_prompt["cases"][0]["execution_error"]
        .as_str()
        .unwrap()
        .contains("no recorded answer"));
    std::fs::write(prompt_path, original_prompt).unwrap();
    assert_ok(&invoke(
        &dir,
        "original-prompt",
        &["--replay", dir.join("recorded").to_str().unwrap()],
    ));
    assert_eq!(read["cases"], report(&dir, "original-prompt")["cases"]);
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
