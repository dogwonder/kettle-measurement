//! #425: the model's candidate is not the report's claim.
//!
//! These tests hold the structured record between the two: which
//! guardrails a candidate reached, where it stopped, and the one
//! terminal disposition that says what the run did with it.

mod support;

use runner::claim_trace::{
    link_letter_actions, CheckOutcome, ClaimKind, ClaimTrace, Guardrail, TerminalDisposition,
};
use runner::eval::replay::Recording;
use runner::packs::load_pack;
use runner::run::{run_pack_bound, Answers};
use runner::run_dir::{NoLog, RunDir};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

fn renewal_pack() -> runner::packs::Pack {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff");
    load_pack(&path).expect("the built-in renewal pack loads")
}

fn documents(name: &str) -> (PathBuf, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("kettle-claim-trace-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("claim trace scratch directory");
    let previous = dir.join("previous.txt");
    let renewal = dir.join("renewal.txt");
    std::fs::write(&previous, "Annual premium: £480.00.").expect("previous policy");
    std::fs::write(
        &renewal,
        "Policy period: 1 September 2026 to 31 August 2027.",
    )
    .expect("renewal policy");
    (previous, renewal)
}

/// The named first test from #425. The answer is schema-valid and the
/// quote is genuinely on the page, but a policy period is not an amount
/// of money. Rust already routes this candidate to review; the trace is
/// what proves which guards passed before the value-shape guard stopped
/// it.
#[test]
fn a_wrong_term_records_every_guardrail_it_reached_and_one_terminal_disposition() {
    let pack = renewal_pack();
    let (previous, renewal) = documents("wrong-value-shape");
    let previous_text = std::fs::read_to_string(&previous).unwrap();
    let renewal_text = std::fs::read_to_string(&renewal).unwrap();
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                {
                    "id": 0,
                    "segment": previous_text,
                    "confidence": "high",
                    "terms": [{
                        "term": "premium",
                        "basis": "annual",
                        "value": "£480.00",
                        "quote": "Annual premium: £480.00."
                    }]
                },
                {
                    "id": 1,
                    "segment": renewal_text,
                    "confidence": "high",
                    "terms": [{
                        "term": "premium",
                        "basis": "annual",
                        "value": "1 September 2026 to 31 August 2027",
                        "quote": "Policy period: 1 September 2026 to 31 August 2027."
                    }]
                }
            ]
        })
        .to_string(),
    );
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);

    let outcome = run_pack_bound(
        &pack,
        &[("previous", previous.clone()), ("renewal", renewal.clone())],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the guarded run completes");

    let wrong = outcome
        .claim_traces
        .iter()
        .find(|trace| {
            trace.kind == ClaimKind::PolicyTerm
                && trace.candidate["value"] == "1 September 2026 to 31 August 2027"
        })
        .expect("the rejected model candidate remains traceable");

    assert_eq!(wrong.check(Guardrail::Schema), Some(CheckOutcome::Passed));
    assert_eq!(wrong.check(Guardrail::Pairing), Some(CheckOutcome::Passed));
    assert_eq!(wrong.check(Guardrail::Quote), Some(CheckOutcome::Passed));
    assert_eq!(
        wrong.check(Guardrail::ValueShape),
        Some(CheckOutcome::Failed)
    );
    assert_eq!(wrong.terminal, TerminalDisposition::NeedsReview);
    assert_eq!(
        outcome
            .claim_traces
            .iter()
            .filter(|trace| trace.id == wrong.id)
            .count(),
        1,
        "one candidate has one trace and one terminal disposition"
    );

    let _ = std::fs::remove_dir_all(previous.parent().unwrap());
}

#[test]
fn an_accepted_obligation_links_to_its_report_row_and_exportable_action() {
    use runner::results::{
        ActionExport, ActionKind, IcsExport, ProposedAction, ProposedActions, ACTIONS_NOTE,
        PROPOSED_ACTIONS_SCHEMA, STATUS_PROPOSED,
    };

    let passage = "Please send proof of no-claims discount within 14 days.";
    let mut traces = vec![ClaimTrace {
        id: "claim-000002".to_owned(),
        parent_id: Some("claim-000001".to_owned()),
        pack: "app.kttl.letter".to_owned(),
        step: "Reading what the letter asks".to_owned(),
        batch: 1,
        item: 0,
        candidate_index: 0,
        kind: ClaimKind::Obligation,
        source: passage.to_owned(),
        candidate: serde_json::json!({"party": "Your insurer", "deadline": "within 14 days"}),
        attempts: Vec::new(),
        checks: Vec::new(),
        terminal: TerminalDisposition::Accepted,
        outputs: Vec::new(),
    }];
    let actions = ProposedActions {
        schema: PROPOSED_ACTIONS_SCHEMA.to_owned(),
        run_id: "run-01".to_owned(),
        note: ACTIONS_NOTE.to_owned(),
        actions: vec![ProposedAction {
            id: "act-01".to_owned(),
            kind: ActionKind::CalendarReminder,
            title: "Send proof".to_owned(),
            detail: "Review before exporting.".to_owned(),
            evidence: std::collections::BTreeMap::from([(
                "passage_1".to_owned(),
                passage.to_owned(),
            )]),
            disputed: Vec::new(),
            export: ActionExport {
                ics: IcsExport {
                    summary: "Send proof".to_owned(),
                    date: chrono::NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(),
                },
                text: "Send proof within 14 days".to_owned(),
            },
            status: STATUS_PROPOSED.to_owned(),
        }],
    };

    link_letter_actions(&mut traces, &actions);

    assert!(traces[0].outputs.iter().any(
        |link| link.guardrail == Guardrail::Report && link.id == "results.json#/obligations/0"
    ));
    assert!(traces[0]
        .outputs
        .iter()
        .any(|link| link.guardrail == Guardrail::Action && link.id == "act-01"));
}

#[test]
fn a_schema_retry_keeps_two_attempts_but_one_final_claim() {
    let pack = renewal_pack();
    let (previous, renewal) = documents("schema-retry");
    let previous_text = std::fs::read_to_string(&previous).unwrap();
    let renewal_text = std::fs::read_to_string(&renewal).unwrap();
    let corrected = serde_json::json!({
        "results": [
            {
                "id": 0,
                "segment": previous_text,
                "confidence": "high",
                "terms": []
            },
            {
                "id": 1,
                "segment": renewal_text,
                "confidence": "high",
                "terms": []
            }
        ]
    });
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(r#"{"wrong":true}"#)),
        ("200 OK", completion_envelope(&corrected.to_string())),
    ]);

    let outcome = run_pack_bound(
        &pack,
        &[("previous", previous.clone()), ("renewal", renewal.clone())],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the retry recovers");

    let decision = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::Decision && trace.source == previous_text)
        .expect("the source has one final decision");
    assert_eq!(decision.attempts.len(), 2);
    assert_eq!(decision.attempts[0].schema, CheckOutcome::Failed);
    assert_eq!(decision.attempts[1].schema, CheckOutcome::Passed);
    assert_eq!(decision.terminal, TerminalDisposition::Accepted);
    assert_eq!(
        outcome
            .claim_traces
            .iter()
            .filter(|trace| trace.kind == ClaimKind::Decision && trace.source == previous_text)
            .count(),
        1
    );

    let _ = std::fs::remove_dir_all(previous.parent().unwrap());
}

#[test]
fn replay_rebuilds_the_same_claim_trace_without_calling_a_model() {
    let pack = renewal_pack();
    let (previous, renewal) = documents("replay");
    let previous_text = std::fs::read_to_string(&previous).unwrap();
    let renewal_text = std::fs::read_to_string(&renewal).unwrap();
    let candidate = serde_json::json!({
        "results": [
            {"id": 0, "segment": previous_text, "confidence": "high", "terms": []},
            {"id": 1, "segment": renewal_text, "confidence": "high", "terms": []}
        ]
    });
    let mock = MockModel::respond_once("200 OK", completion_envelope(&candidate.to_string()));
    let root = previous.parent().unwrap().join("recordings");
    let run_dir = RunDir::create(&root, "live").expect("recording directory");
    let live = run_pack_bound(
        &pack,
        &[("previous", previous.clone()), ("renewal", renewal.clone())],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &run_dir,
    )
    .expect("the live run completes");

    let recording = Recording::from_run_dirs(&root).expect("raw exchanges form a replay");
    let replayed = run_pack_bound(
        &pack,
        &[("previous", previous.clone()), ("renewal", renewal.clone())],
        &Answers::FromModel(runner::exec::Endpoint::replaying(recording)),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("replay completes without a live model");

    assert_eq!(replayed.claim_traces, live.claim_traces);
    let _ = std::fs::remove_dir_all(previous.parent().unwrap());
}

/// The named first test from #470. This trace is #435's shape: the
/// model read the letter correctly, every guardrail passed, and then
/// `resolve_deadline` derived the anchor itself instead of counting
/// thirty days from it. The model was never the threat here, so the
/// record must not say the pipeline contained one — and it must not
/// say `accepted` either, because what was accepted is not what the
/// model proposed.
#[test]
fn a_correct_proposal_corrupted_downstream_is_not_recorded_as_contained() {
    let mut trace = obligation_trace_with_every_guardrail_passed();

    // The deterministic stage changes the value: thirty days early, in
    // the strongest voice the report has (#435).
    trace.record_derivation(
        "timeline::resolve_deadline",
        "due 2026-06-21, worked out",
        "due 2026-05-22, read and verified",
    );

    assert_eq!(
        trace.terminal,
        TerminalDisposition::PipelineIntroducedError,
        "a correct proposal made wrong downstream is the pipeline's error"
    );
    assert_ne!(trace.terminal, TerminalDisposition::Accepted);
    assert_ne!(
        trace.terminal,
        TerminalDisposition::NeedsReview,
        "never counted as containment"
    );
    assert_ne!(
        trace.terminal,
        TerminalDisposition::Rejected,
        "never counted as containment"
    );

    // The model's own record stays clean: no guardrail it passed is
    // rewritten into a failure, because the proposal was right.
    for guardrail in [
        Guardrail::Schema,
        Guardrail::Pairing,
        Guardrail::Quote,
        Guardrail::ValueShape,
    ] {
        assert_eq!(trace.check(guardrail), Some(CheckOutcome::Passed));
    }

    // The record names its own culprit stage, so #435 and #457 stop
    // being unnameable.
    let derivation = trace
        .checks
        .iter()
        .find(|check| check.guardrail == Guardrail::DeterministicDerivation)
        .expect("the derivation stage leaves a check entry");
    assert_eq!(derivation.outcome, CheckOutcome::ChangedValue);
    let detail = derivation.detail.as_deref().expect("the stage is named");
    assert!(detail.contains("timeline::resolve_deadline"));
    assert!(detail.contains("due 2026-06-21, worked out"));
    assert!(detail.contains("due 2026-05-22, read and verified"));
}

/// The other half of #470's rule, which must not regress: derivation
/// doing its job faithfully changes nothing. `accepted` keeps its
/// meaning — this is an additive disposition, not a redefinition.
#[test]
fn a_faithful_derivation_leaves_an_accepted_claim_accepted() {
    let mut trace = obligation_trace_with_every_guardrail_passed();

    trace.record_derivation(
        "timeline::resolve_deadline",
        "due 2026-06-21, worked out",
        "due 2026-06-21, worked out",
    );

    assert_eq!(trace.terminal, TerminalDisposition::Accepted);
    assert_eq!(
        trace.check(Guardrail::DeterministicDerivation),
        Some(CheckOutcome::Passed),
        "a clean derivation is evidence too, not silence"
    );
}

/// A candidate a guardrail already stopped stays contained even when
/// the derivation stage also mishandles it: the model error was real
/// and was caught, the person sees the item either way, and the
/// stage's fault remains on the record in its check entry.
#[test]
fn a_contained_candidate_stays_contained_when_derivation_changes_the_value() {
    let mut trace = obligation_trace_with_every_guardrail_passed();
    trace
        .checks
        .iter_mut()
        .find(|check| check.guardrail == Guardrail::ValueShape)
        .expect("the builder records a value-shape check")
        .outcome = CheckOutcome::Failed;
    trace.terminal = TerminalDisposition::NeedsReview;

    trace.record_derivation(
        "timeline::resolve_deadline",
        "due 2026-06-21, worked out",
        "due 2026-05-22, read and verified",
    );

    assert_eq!(trace.terminal, TerminalDisposition::NeedsReview);
    assert_eq!(
        trace.check(Guardrail::DeterministicDerivation),
        Some(CheckOutcome::ChangedValue)
    );
}

fn obligation_trace_with_every_guardrail_passed() -> ClaimTrace {
    use runner::claim_trace::ClaimCheck;

    ClaimTrace {
        id: "claim-000007".to_owned(),
        parent_id: Some("claim-000006".to_owned()),
        pack: "app.kttl.letter".to_owned(),
        step: "Reading what the letter asks".to_owned(),
        batch: 1,
        item: 0,
        candidate_index: 0,
        kind: ClaimKind::Obligation,
        source: "Please pay £120.00 within 30 days of 22 May 2026, quoting reference HPS-4821."
            .to_owned(),
        candidate: serde_json::json!({
            "party": "you",
            "deadline": "within 30 days of 22 May 2026"
        }),
        attempts: Vec::new(),
        checks: [
            Guardrail::Schema,
            Guardrail::Pairing,
            Guardrail::Quote,
            Guardrail::ValueShape,
        ]
        .into_iter()
        .map(|guardrail| ClaimCheck {
            guardrail,
            outcome: CheckOutcome::Passed,
            detail: None,
        })
        .collect(),
        terminal: TerminalDisposition::Accepted,
        outputs: Vec::new(),
    }
}

#[test]
fn a_schema_valid_mispaired_candidate_records_the_pairing_guard_failure() {
    let pack = renewal_pack();
    let (previous, renewal) = documents("mispaired");
    let renewal_text = std::fs::read_to_string(&renewal).unwrap();
    let first = serde_json::json!({
        "results": [
            {"id": 0, "segment": "some other policy", "confidence": "high", "terms": []},
            {"id": 1, "segment": renewal_text, "confidence": "high", "terms": []}
        ]
    });
    let reask = serde_json::json!({
        "results": [
            {"id": 0, "segment": "still some other policy", "confidence": "high", "terms": []}
        ]
    });
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(&first.to_string())),
        ("200 OK", completion_envelope(&reask.to_string())),
    ]);

    let outcome = run_pack_bound(
        &pack,
        &[("previous", previous.clone()), ("renewal", renewal.clone())],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the candidate is contained rather than crashing the run");

    let previous_text = std::fs::read_to_string(&previous).unwrap();
    let trace = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::Decision && trace.source == previous_text)
        .expect("the mispaired candidate has a lifecycle");
    assert_eq!(trace.check(Guardrail::Schema), Some(CheckOutcome::Passed));
    assert_eq!(trace.check(Guardrail::Pairing), Some(CheckOutcome::Failed));
    assert_eq!(trace.terminal, TerminalDisposition::NeedsReview);
    assert_eq!(trace.attempts.len(), 2, "the targeted re-ask is retained");

    let _ = std::fs::remove_dir_all(previous.parent().unwrap());
}
