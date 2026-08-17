//! End-to-end pipeline tests (#78): a pack, a statement, a mock model —
//! a completed run. This is M1's finish line in test form.

mod support;

#[cfg(feature = "pdf")]
use runner::aggregate::build_report;
use runner::exec::Endpoint;
use runner::packs::load_pack;
use runner::recurrence::Period;
#[cfg(feature = "pdf")]
use runner::results::{DateRange, InputInfo, ModelInfo, PackInfo, RunInfo};
use runner::run::{run_pack, Answers, Payload, Progress, RunError};
#[cfg(feature = "pdf")]
use runner::run::{run_pack_with_resources, RunResources};
use runner::run_dir::NoLog;
use rust_decimal::Decimal;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use support::{completion_envelope, MockModel};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
}

fn statement() -> PathBuf {
    pack_dir().join("fixtures/statement-02-messy.csv")
}

#[cfg(feature = "pdf")]
#[test]
fn text_layer_pdf_reaches_the_ordinary_pipeline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sidecars = root.join("sidecars");
    if !runner::pdf::library_present(&sidecars) {
        eprintln!("skipping: no libpdfium in sidecars/ — see sidecars/README.md");
        return;
    }
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let pdf = pack_dir().join("fixtures/statement-04.pdf");

    let outcome = run_pack_with_resources(
        &pack,
        &[pdf],
        &Answers::WithoutModel,
        RunResources {
            pdfium_dir: Some(&sidecars),
        },
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("a positioned PDF reaches the same pipeline as CSV");

    assert_eq!(outcome.input.rows, 30);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
}

#[cfg(feature = "pdf")]
fn statement_04_normalise_answer() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "raw": "PUREGYM LTD", "name": "PureGym", "recognised": true},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "recognised": true},
        {"id": 2, "raw": "SPOTIFY LTD", "name": "Spotify", "recognised": true},
        {"id": 3, "raw": "KAFFA COFFEE", "name": "Kaffa Coffee", "recognised": true},
        {"id": 4, "raw": "ACME PAYROLL", "name": "Acme Payroll", "recognised": true}
    ]}"#,
    )
}

#[cfg(feature = "pdf")]
fn statement_04_classify_answer() -> String {
    completion_envelope(
        r#"{"results": [
        {"id": 0, "name": "PureGym", "kind": "subscription", "category": "fitness", "confidence": "high"},
        {"id": 1, "name": "Netflix", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 2, "name": "Spotify", "kind": "subscription", "category": "streaming", "confidence": "high"},
        {"id": 3, "name": "Kaffa Coffee", "kind": "regular_spend", "category": "food_drink", "confidence": "high"},
        {"id": 4, "name": "Acme Payroll", "kind": "one_off", "category": "other", "confidence": "high"}
    ]}"#,
    )
}

#[cfg(feature = "pdf")]
#[test]
fn text_layer_pdf_reaches_recurring_findings_and_report_totals() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sidecars = root.join("sidecars");
    if !runner::pdf::library_present(&sidecars) {
        eprintln!("skipping: no libpdfium in sidecars/ — see sidecars/README.md");
        return;
    }
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let pdf = pack_dir().join("fixtures/statement-04.pdf");
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(pack_dir().join("fixtures/statement-04.pdf.expected.json"))
            .expect("PDF expectations"),
    )
    .expect("expectations are JSON");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", statement_04_normalise_answer()),
        ("200 OK", statement_04_classify_answer()),
    ]);

    let outcome = run_pack_with_resources(
        &pack,
        std::slice::from_ref(&pdf),
        &Answers::FromModel(mock.endpoint()),
        RunResources {
            pdfium_dir: Some(&sidecars),
        },
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the PDF reaches the complete pipeline");
    let (from, to) = outcome.input.period.expect("the statement has dates");
    let report = build_report(
        &outcome,
        RunInfo {
            id: "statement-04-pdf".to_owned(),
            pack: PackInfo {
                id: pack.manifest.id.clone(),
                version: pack.manifest.version.clone(),
                title: pack.manifest.name.clone(),
            },
            input: InputInfo {
                file: "statement-04.pdf".to_owned(),
                rows: outcome.input.rows,
                period: DateRange { from, to },
                hash: "blake3:fixture".to_owned(),
            },
            model: ModelInfo {
                tier: "Test".to_owned(),
                id: "mock".to_owned(),
            },
            started: "2025-07-01T00:00:00Z".to_owned(),
            finished: "2025-07-01T00:00:01Z".to_owned(),
            currency: "GBP".to_owned(),
        },
    )
    .expect("an audit run reports");

    assert_eq!(
        report.run.input.rows,
        expected["rows"].as_u64().expect("row expectation") as usize
    );
    for wanted in expected["recurring"]
        .as_array()
        .expect("recurring expectations")
    {
        let merchant = wanted["merchant"].as_str().expect("merchant");
        let found = report
            .recurring
            .iter()
            .find(|finding| finding.merchant == merchant)
            .unwrap_or_else(|| panic!("{merchant} was not found: {:#?}", report.recurring));
        assert_eq!(found.period.as_wire(), wanted["period"].as_str().unwrap());
        assert_eq!(
            found.annualised.to_string(),
            wanted["annualised"].as_str().unwrap()
        );
        assert_eq!(
            found.price_rise.is_some(),
            wanted["price_rise"].as_bool().unwrap()
        );
    }
    assert_eq!(
        report.summary.recurring_count,
        expected["summary"]["recurring_count"].as_u64().unwrap() as usize
    );
    assert_eq!(
        report.summary.annualised_total.to_string(),
        expected["summary"]["annualised_total"].as_str().unwrap()
    );
    assert_eq!(
        report.summary.monthly_equivalent.to_string(),
        expected["summary"]["monthly_equivalent"].as_str().unwrap()
    );
    assert_eq!(
        report.summary.price_rises,
        expected["summary"]["price_rises"].as_u64().unwrap() as usize
    );
    assert!(
        report
            .regular_spend
            .iter()
            .any(|spend| spend.merchant == "Kaffa Coffee"),
        "non-recurring spending survives the same PDF"
    );
    assert!(
        report
            .income
            .iter()
            .any(|income| income.merchant == "Acme Payroll"),
        "credits remain income rather than entering the spending total"
    );
}

/// The canned answers a real model would give for statement-02-messy's
/// five merchant groups, in first-seen order.
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
fn subscription_audit_end_to_end_with_mock_model() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", normalise_answer()),
        ("200 OK", classify_answer()),
    ]);

    let mut labels: Vec<String> = Vec::new();
    let outcome = run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |progress: Progress| labels.push(progress.step.to_owned()),
        &NoLog,
    )
    .expect("the run completes");

    // The recurring findings, classified. Coffee shops and marketplace
    // orders recur nowhere and must not appear.
    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    assert_eq!(audit.findings.len(), 2, "{:#?}", audit.findings);

    let disney = &audit.findings[0];
    assert_eq!(disney.merchant, "Disney+");
    assert_eq!(disney.raw_merchant, "PAYPAL *DISNEYPLUS");
    assert_eq!(disney.kind, "subscription");
    assert_eq!(disney.category, "streaming");
    assert_eq!(disney.confidence, "high");
    assert_eq!(disney.period, Period::Monthly);
    assert_eq!(disney.current_amount, Decimal::from_str("10.99").unwrap());
    assert!(
        disney.price_rise.is_some(),
        "the fixture's rise survives the pipeline"
    );
    assert_eq!(
        disney.evidence.len(),
        34,
        "every payment travels as evidence"
    );

    let prime = &audit.findings[1];
    assert_eq!(prime.merchant, "Amazon Prime");
    assert_eq!(prime.period, Period::Annual);

    // Clean answers, clean run: nothing needed a human.
    assert!(
        outcome.needs_review.is_empty(),
        "{:#?}",
        outcome.needs_review
    );
    assert!(outcome.warnings.is_empty());

    // Progress spoke plain language throughout — no pipeline jargon
    // reaches a person (CLAUDE.md).
    assert!(!labels.is_empty());
    assert!(labels[0].contains("Reading"), "{labels:?}");
    for label in &labels {
        for jargon in ["batch", "schema", "model", "prompt", "step"] {
            assert!(
                !label.to_lowercase().contains(jargon),
                "jargon {jargon:?} in progress label {label:?}"
            );
        }
    }
}

#[test]
fn progress_labels_are_the_sequence_the_progress_screen_expects() {
    // The shell's step list is pre-seeded with the approved sequence
    // from mock 03 (app/src/screens/copy.ts, `stepSequence`). A label
    // the sequence didn't predict is appended rather than dropped, so a
    // mismatch here doesn't break the screen — it just shows steps
    // stuck on Waiting forever. The two lists have to stay in step.
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", normalise_answer()),
        ("200 OK", classify_answer()),
    ]);

    let mut labels: Vec<String> = Vec::new();
    run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |progress: Progress| labels.push(progress.step.to_owned()),
        &NoLog,
    )
    .expect("the run completes");

    // Steps heartbeat once per file or batch, so compare the sequence
    // of distinct labels, in order.
    labels.dedup();
    assert_eq!(
        labels,
        [
            "Reading your statement",
            "Grouping payments by merchant",
            "Sorting merchants",
            "Checking for price rises",
            "Writing your report",
        ]
    );
}

#[test]
fn twice_failed_step_reviews_everything_and_still_completes() {
    // The normalise step fails twice: every merchant lands in
    // needs-review with a plain-language reason, classify is never
    // asked (no items), and the run still completes. Never crash a run
    // on one bad batch (#23) — and never crash it on a bad step either.
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(r#"{"nope": 1}"#)),
        ("200 OK", completion_envelope(r#"{"nope": 2}"#)),
    ]);

    let outcome = run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("a failed step never crashes the run");

    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    assert!(audit.findings.is_empty());
    assert_eq!(outcome.needs_review.len(), 5, "{:#?}", outcome.needs_review);
    for review in &outcome.needs_review {
        assert!(
            !review.transactions.is_empty(),
            "the payments travel with the review item"
        );
        for jargon in ["batch", "schema", "valid", "json"] {
            assert!(
                !review.reason.to_lowercase().contains(jargon),
                "jargon {jargon:?} in reason {:?}",
                review.reason
            );
        }
    }
}

#[test]
fn cancelled_run_stops_before_asking_the_model() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    // No mock at all: a cancelled run must never reach the network.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let error = run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(Endpoint::local(port)),
        &AtomicBool::new(true),
        &mut |_| {},
        &NoLog,
    )
    .expect_err("cancelled");
    assert!(matches!(error, RunError::Cancelled));
}

/// The other half of cancellation, and the load-bearing one for #46's
/// Cancel button: a cancel that arrives *after* a batch has been sent,
/// while the model call is still in flight. Today `run_pack` checks the
/// flag only before each batch (`run.rs`), and `call_constrained` blocks
/// in `ureq` with no timeout — so against a stalled sidecar the run
/// hangs forever and the person's Cancel does nothing.
///
/// The contract this pins: cancelling always stops the run within a
/// bounded time. It deliberately does *not* dictate the mechanism (a
/// per-call timeout, a cancel-aware transport, aborting the request) —
/// that is #46's decision. It only forbids "hangs forever".
///
/// RED until #46: the run thread never returns and this times out.
#[test]
fn a_cancel_while_the_model_call_is_in_flight_stops_the_run_it_does_not_hang() {
    let mock = MockModel::hang();
    let endpoint = mock.endpoint();
    let cancel = Arc::new(AtomicBool::new(false));

    // Drive the run on its own thread and report back only the verdict,
    // so nothing about the run's borrows or errors has to cross threads.
    let (verdict_tx, verdict_rx) = mpsc::channel();
    let cancel_in_thread = Arc::clone(&cancel);
    std::thread::spawn(move || {
        let pack = load_pack(&pack_dir()).expect("pack loads");
        let verdict = match run_pack(
            &pack,
            &[statement()],
            &Answers::FromModel(endpoint),
            &cancel_in_thread,
            &mut |_: Progress| {},
            &NoLog,
        ) {
            Ok(_) => "completed",
            Err(RunError::Cancelled) => "cancelled",
            Err(_) => "errored",
        };
        verdict_tx.send(verdict).ok();
    });

    // Wait until the model call is genuinely in flight — the wedged mock
    // announces the request it will never answer — then cancel.
    mock.request_body();
    cancel.store(true, Ordering::Relaxed);

    match verdict_rx.recv_timeout(Duration::from_secs(3)) {
        Ok("cancelled") => {}
        Ok(other) => panic!("a cancelled run should stop, not {other}"),
        Err(_) => panic!(
            "run_pack hung on a stalled model call after cancellation — a cancel \
             arriving mid-batch is never seen (call_constrained blocks in ureq with \
             no timeout). #46 must make Cancel mean something here."
        ),
    }
}

#[test]
fn an_audit_run_carries_its_findings_in_a_typed_payload() {
    // #238. A run has two halves, and only one of them is the
    // subscription audit's. What was read, what nobody could answer and
    // what went wrong belong to *every* run whatever the pack asked;
    // findings, income and regular spending belong to the Audit
    // typology and mean nothing to a letter.
    //
    // Reading the envelope must therefore need no knowledge of which
    // typology ran, and reading the findings must require saying so.
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", normalise_answer()),
        ("200 OK", classify_answer()),
    ]);

    let outcome = run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes");

    // The envelope, reached without knowing the typology.
    assert_eq!(outcome.input.rows, 53);
    assert!(
        outcome.needs_review.is_empty(),
        "{:#?}",
        outcome.needs_review
    );
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);

    // The payload, which has to be named to be read.
    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    assert_eq!(audit.findings.len(), 2, "{:#?}", audit.findings);
    assert_eq!(audit.findings[0].merchant, "Disney+");
    assert_eq!(audit.other.len(), 3, "{:#?}", audit.other);
    assert!(audit.income.is_empty(), "{:#?}", audit.income);
}

#[test]
fn a_model_step_plays_the_role_it_declares_not_the_one_its_position_implies() {
    // #120's other half. `run_pack` used to decide what a model step
    // meant by counting schema-bearing steps: index 0 was always
    // "normalise merchants", anything after it "classify".
    //
    // The discriminator is a third schema-bearing step declaring
    // `normalise`. By position it is the *last* of three and the old code
    // reads it as classify; by declaration it is normalise. It asks about
    // the cleaned representatives, exactly as the first one does, so the
    // step has real work either way — only the reading differs.
    let dir = std::env::temp_dir().join(format!("kettle-role-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&pack_dir(), &dir);

    let manifest_path = dir.join("pack.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let normalise_step = manifest
        .lines()
        .find(|line| line.contains(r#""role": "normalise""#))
        .expect("the normalise step is in the manifest")
        .to_owned();
    // A second normalise, immediately before the aggregate step.
    let with_third = manifest.replace(
        r#"    { "step": "aggregate""#,
        &format!("{normalise_step}\n    {{ \"step\": \"aggregate\""),
    );
    assert_ne!(
        manifest, with_third,
        "the aggregate step should exist to insert before"
    );
    std::fs::write(&manifest_path, with_third).expect("write manifest");

    let pack = load_pack(&dir).expect("three declared roles all in the supported set");

    let mock = MockModel::respond_sequence(vec![
        ("200 OK", normalise_answer()),
        ("200 OK", classify_answer()),
        ("200 OK", normalise_answer()),
    ]);

    let mut labels: Vec<String> = Vec::new();
    run_pack(
        &pack,
        &[statement()],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |progress: Progress| labels.push(progress.step.to_owned()),
        &NoLog,
    )
    .expect("the run completes");
    labels.dedup();

    // Position would make the third model step "Sorting merchants".
    // Its declaration makes it merchant cleanup, so that label returns.
    let merchants = labels
        .iter()
        .filter(|label| *label == "Grouping payments by merchant")
        .count();
    assert_eq!(
        merchants, 2,
        "both steps declaring normalise should play normalise: {labels:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Recursive copy, so a test can start from the real pack and change one
/// thing about it.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

#[test]
fn kind_is_decided_by_cadence_not_by_the_model() {
    // #253's named test. Twelve stable monthly payments are a
    // subscription even when the model cannot name the merchant; three
    // irregular payments are not, however confidently the model calls
    // the name "streaming". The model answers what a name can carry —
    // category — and cadence, which it was never shown, decides kind.
    let dir = std::env::temp_dir().join(format!("kettle-cadence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&pack_dir(), &dir);
    let months = (1..=12)
        .map(|month| format!("2025-{month:02}-07,NIGHTJAR VIDEO,-9.99"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        dir.join("fixtures/cadence.csv"),
        format!(
            "Date,Description,Amount\n{months}\n\
             2025-02-11,HARBOUR MARKET,-31.40\n\
             2025-06-03,HARBOUR MARKET,-8.15\n\
             2025-09-27,HARBOUR MARKET,-44.02\n"
        ),
    )
    .expect("write statement");

    let pack = load_pack(&dir).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "raw": "NIGHTJAR VIDEO", "name": "Nightjar Video", "recognised": false},
                {"id": 1, "raw": "HARBOUR MARKET", "name": "Harbour Market", "recognised": true}
            ]}"#,
            ),
        ),
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "name": "Nightjar Video", "category": "unknown", "confidence": "high"},
                {"id": 1, "name": "Harbour Market", "category": "streaming", "confidence": "high"}
            ]}"#,
            ),
        ),
    ]);

    let outcome = run_pack(
        &pack,
        &[dir.join("fixtures/cadence.csv")],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes");

    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };

    // Recurring + unnameable: still found, kind decided by the pack's
    // policy for unknown, surfaced for a person via forced-low
    // confidence — decided by cadence and policy, invented by nobody.
    let nightjar = audit
        .findings
        .iter()
        .find(|f| f.merchant == "Nightjar Video")
        .unwrap_or_else(|| panic!("cadence makes the finding: {:#?}", audit.findings));
    assert_eq!(nightjar.kind, "subscription");
    assert_eq!(nightjar.category, "unknown");
    assert_eq!(
        nightjar.confidence, "low",
        "an unknown category can never be a confident claim"
    );

    // Irregular + confidently named: the name does not make it recur.
    assert!(
        !audit
            .findings
            .iter()
            .any(|f| f.merchant == "Harbour Market"),
        "three irregular payments are not a series: {:#?}",
        audit.findings
    );
    let harbour = audit
        .other
        .iter()
        .find(|s| s.merchant == "Harbour Market")
        .unwrap_or_else(|| panic!("irregular spending lands in other: {:#?}", audit.other));
    assert_eq!(
        harbour.kind, "regular_spend",
        "kind comes from behaviour, not from the model's confidence"
    );
    assert_eq!(
        harbour.category, "streaming",
        "the label is still the model's to give"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The run metadata `build_report` needs. Values are arbitrary — this
/// test is about which merchants reach the checklist, not about the
/// header of the report they reach it in.
#[cfg(feature = "pdf")]
fn run_info_for_declined() -> RunInfo {
    RunInfo {
        id: "run-declined-cadence".to_owned(),
        pack: PackInfo {
            id: "app.kttl.subscription-audit".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Subscription & recurring-spend audit".to_owned(),
        },
        input: InputInfo {
            file: "drifting.csv".to_owned(),
            rows: 12,
            period: DateRange {
                from: chrono::NaiveDate::from_ymd_opt(2025, 1, 3).expect("date"),
                to: chrono::NaiveDate::from_ymd_opt(2025, 12, 3).expect("date"),
            },
            hash: "blake3:0000".to_owned(),
        },
        model: ModelInfo {
            tier: "Steady".to_owned(),
            id: "mock".to_owned(),
        },
        started: "2026-08-02T00:00:00Z".to_owned(),
        finished: "2026-08-02T00:00:01Z".to_owned(),
        currency: "GBP".to_owned(),
    }
}

/// #271, the test the issue named: a declined cadence must be surfaced,
/// not asserted.
///
/// A subscription whose amount drifts by a penny each month finds no
/// exact-amount series to certify (#261, and that part is correct —
/// declining beats inventing). What was wrong is what happened next:
/// the item was filed as regular spending carrying the model's
/// confidence about *category*, a question nobody asked about
/// recurrence. A merchant the 7B correctly called `streaming / high`
/// became regular spending at high confidence — asserted, never shown,
/// never correctable.
///
/// The unit tests cover `looks_periodic` in isolation. This covers the
/// chain, which is delivered by four pieces agreeing: detection
/// declines, `looks_periodic` says the decline is uncertain rather than
/// certain, the confidence is replaced, and the report tags it unsure.
#[test]
fn a_merchant_whose_cadence_was_declined_is_surfaced_not_asserted() {
    let dir = std::env::temp_dir().join(format!("kettle-declined-cadence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let statement = dir.join("drifting.csv");

    // Twelve monthly payments drifting by a penny: periodic to any
    // reader, uncertifiable by exact-amount clustering.
    let mut csv = String::from("Date,Description,Amount\n");
    for month in 1..=12 {
        csv.push_str(&format!("2025-{month:02}-03,ALDER STREAMING,-{}\n", {
            let pence = 10 + month;
            format!("12.{pence:02}")
        }));
    }
    std::fs::write(&statement, &csv).expect("write statement");

    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "raw": "ALDER STREAMING", "name": "Alder Streaming", "recognised": true}
            ]}"#,
            ),
        ),
        // The model is *right* about the category, and sure of it. That
        // is precisely what made the old behaviour dangerous: a correct,
        // confident answer to one question licensed a confident answer
        // to another.
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "name": "Alder Streaming", "kind": "subscription",
                 "category": "streaming", "confidence": "high"}
            ]}"#,
            ),
        ),
    ]);

    let outcome = run_pack(
        &pack,
        &[statement],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_: Progress| {},
        &NoLog,
    )
    .expect("the run completes");

    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    assert!(
        audit.findings.is_empty(),
        "no series was certified, and inventing one is not the fix: {:#?}",
        audit.findings
    );

    let spend = audit
        .other
        .iter()
        .find(|spend| spend.merchant == "Alder Streaming")
        .expect("the merchant is still accounted for somewhere");
    assert_eq!(
        spend.confidence, "low",
        "the confidence on a kind must be about the decision that produced that kind — \
         cadence declined, so the kind is uncertain however sure the model was about category"
    );
    assert_eq!(
        spend.kind_from,
        runner::kinds::KindFrom::CadenceDespitePeriodic,
        "and the record says which decision produced it, so a run can be traced (#272)"
    );

    // Where "surfaced" currently stops. `check_yourself` reads the
    // recurring findings, and a declined cadence is by definition not
    // one — so the merchant is tagged unsure in the spending table and
    // is *not* in the checklist a person is pointed at. The eval scores
    // it as review (`fixture.rs`, low confidence -> NeedsReview) and
    // quotes a review rate that includes it.
    //
    // Asserted here rather than fixed: whether the checklist should
    // carry these is a product call about how long that list may get,
    // and #271's own correction says a change of that kind wants its own
    // decision rather than being smuggled in. This test is the evidence
    // that the two lists differ, so the next person meets the fact
    // rather than the surprise.
    #[cfg(feature = "pdf")]
    {
        let report = runner::aggregate::build_report(&outcome, run_info_for_declined())
            .expect("an audit run reports");
        let checklist = runner::aggregate::check_yourself(&report.recurring);
        assert!(
            !checklist
                .iter()
                .any(|entry| entry.about == "Alder Streaming"),
            "documenting today's behaviour, not endorsing it: {checklist:#?}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// The sibling, and the one that decides whether the fix is worth
/// having: a genuinely detected series keeps its confidence.
///
/// Replacing confidence wherever cadence was involved would trade one
/// dishonest number for a review list nobody can use. The rule is
/// narrower than that — the confidence is replaced only where the
/// decline is *uncertain*, which is where the payments look periodic
/// and no series could be certified.
#[test]
fn a_detected_series_keeps_the_confidence_it_was_given() {
    let dir = std::env::temp_dir().join(format!("kettle-clean-cadence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let statement = dir.join("steady.csv");

    // The same twelve months, at the same amount every time.
    let mut csv = String::from("Date,Description,Amount\n");
    for month in 1..=12 {
        csv.push_str(&format!("2025-{month:02}-03,BIRCH STREAMING,-12.99\n"));
    }
    std::fs::write(&statement, &csv).expect("write statement");

    let pack = load_pack(&pack_dir()).expect("pack loads");
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "raw": "BIRCH STREAMING", "name": "Birch Streaming", "recognised": true}
            ]}"#,
            ),
        ),
        (
            "200 OK",
            completion_envelope(
                r#"{"results": [
                {"id": 0, "name": "Birch Streaming", "kind": "subscription",
                 "category": "streaming", "confidence": "high"}
            ]}"#,
            ),
        ),
    ]);

    let outcome = run_pack(
        &pack,
        &[statement],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_: Progress| {},
        &NoLog,
    )
    .expect("the run completes");

    let Payload::Audit(audit) = &outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    let finding = audit
        .findings
        .iter()
        .find(|finding| finding.merchant == "Birch Streaming")
        .expect("a steady monthly charge is a series");
    assert_eq!(finding.period, Period::Monthly);
    assert_eq!(
        finding.confidence, "high",
        "cadence was certified, so the model's confidence stands"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
