//! Architectural ablations on one scorecard (#432).
//!
//! A model leaderboard cannot say whether the model is useful or
//! whether a guardrail is supplying the reliability. So a *policy* — a
//! complete system, from the deterministic floor up to the full
//! pipeline — is scored against the same task outcomes, and the
//! intermediate ones are derived by re-reading the claim-lifecycle
//! trace rather than by keeping unsafe modes alive to benchmark them.

use runner::claim_trace::{
    CheckOutcome, ClaimCheck, ClaimKind, ClaimTrace, Guardrail, TerminalDisposition,
};
use runner::eval::ablation::{scorecard, CandidateVerdict, Policy};
use std::collections::BTreeMap;

/// One wrong candidate, stopped by quote validation.
fn wrong_claim_stopped_by_quote() -> ClaimTrace {
    ClaimTrace {
        id: "claim-000042".to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        step: "Reading what it asks of you".to_owned(),
        batch: 1,
        item: 1,
        candidate_index: 0,
        kind: ClaimKind::Obligation,
        source: "Pay £84.00 within 14 days.".to_owned(),
        candidate: serde_json::json!({ "amount": "£99.00" }),
        attempts: Vec::new(),
        checks: vec![
            ClaimCheck {
                guardrail: Guardrail::Schema,
                outcome: CheckOutcome::Passed,
                detail: None,
            },
            ClaimCheck {
                guardrail: Guardrail::Pairing,
                outcome: CheckOutcome::Passed,
                detail: None,
            },
            ClaimCheck {
                guardrail: Guardrail::Quote,
                outcome: CheckOutcome::Failed,
                detail: Some("£99.00 is not in the source".to_owned()),
            },
        ],
        outputs: Vec::new(),
        terminal: TerminalDisposition::Rejected,
        parent_id: None,
    }
}

/// The whole point of the scorecard: the same wrong claim is an escaped
/// error under a policy that has not yet reached the guard that stops
/// it, and a contained one under a policy that has. Attributing it to
/// the guard is what tells you whether the reliability comes from the
/// model or from the pipeline — which a leaderboard cannot answer.
///
/// Both rows must name the same claim id, or the two policies are being
/// scored against different things and the comparison means nothing.
#[test]
fn the_trace_scorecard_attributes_a_contained_wrong_claim_to_the_guard_that_stopped_it() {
    let traces = vec![wrong_claim_stopped_by_quote()];
    let verdicts = BTreeMap::from([("claim-000042".to_owned(), CandidateVerdict::Wrong)]);

    let pre_quote = Policy::named("schema-and-pairing")
        .with(Guardrail::Schema)
        .with(Guardrail::Pairing);
    let full = Policy::named("full-pipeline")
        .with(Guardrail::Schema)
        .with(Guardrail::Pairing)
        .with(Guardrail::Quote);

    let rows = scorecard(&traces, &verdicts, &[pre_quote, full]);

    assert_eq!(rows.len(), 2, "one row per policy");

    let before = &rows[0];
    assert_eq!(before.policy, "schema-and-pairing");
    assert_eq!(
        before.escaped,
        vec!["claim-000042".to_owned()],
        "without the quote guard the wrong value is asserted: {before:?}"
    );
    assert!(before.prevented.is_empty());

    let after = &rows[1];
    assert_eq!(after.policy, "full-pipeline");
    assert_eq!(
        after.prevented,
        vec!["claim-000042".to_owned()],
        "with it, the same claim is stopped: {after:?}"
    );
    assert!(after.escaped.is_empty());

    assert_eq!(
        before.escaped, after.prevented,
        "both rows must name the same claim, or the policies were scored \
         against different things"
    );
}

/// A guard that stops a claim has only *demonstrably* prevented an
/// error when the candidate it stopped was already wrong (#432).
///
/// The bound is one-sided and the column has to say so. Between a raw
/// candidate and a scored assertion sit the deterministic derivation
/// stages, and a policy missing a guard still runs them — but for a
/// claim stopped early, those stages never ran, so what they would have
/// produced is not in the recording and cannot be replayed out of it.
/// #470's `PipelineIntroducedError` is the case that makes this matter
/// rather than being pedantry: derivation turning a *correct* candidate
/// into a wrong assertion is invisible to a candidate-level comparison,
/// so counting every contained claim as an error prevented would credit
/// the pipeline with catching harm it introduces itself.
///
/// So a stopped claim whose candidate was already wrong is prevented,
/// and one whose candidate looked right is **unknown** — never
/// prevented, and never quietly dropped either, because the size of the
/// unknown column is how a reader sees how tight the bound is.
#[test]
fn a_stopped_claim_is_prevented_only_when_its_candidate_was_already_wrong() {
    let mut looked_right = wrong_claim_stopped_by_quote();
    looked_right.id = "claim-000043".to_owned();

    let traces = vec![wrong_claim_stopped_by_quote(), looked_right];
    let verdicts = BTreeMap::from([
        ("claim-000042".to_owned(), CandidateVerdict::Wrong),
        ("claim-000043".to_owned(), CandidateVerdict::Unknown),
    ]);

    let full = Policy::named("full-pipeline")
        .with(Guardrail::Schema)
        .with(Guardrail::Pairing)
        .with(Guardrail::Quote);

    let rows = scorecard(&traces, &verdicts, &[full]);
    let row = &rows[0];

    assert_eq!(
        row.prevented,
        vec!["claim-000042".to_owned()],
        "only the candidate already known wrong is a demonstrated catch: {row:?}"
    );
    assert_eq!(
        row.unknown,
        vec!["claim-000043".to_owned()],
        "a stopped candidate that looked right is unknowable from a \
         recording, not a prevented error: {row:?}"
    );
    assert!(
        row.escaped.is_empty(),
        "the quote guard stopped both, so nothing was asserted"
    );
}

/// A candidate the recording shows was correct is not a catch at all —
/// it is a guard spending review on something that did not need it.
/// Counted nowhere in the harm columns, so it can never flatter them.
#[test]
fn a_stopped_candidate_that_was_correct_is_no_part_of_the_harm_columns() {
    let mut correct = wrong_claim_stopped_by_quote();
    correct.id = "claim-000044".to_owned();

    let verdicts = BTreeMap::from([("claim-000044".to_owned(), CandidateVerdict::Correct)]);
    let full = Policy::named("full-pipeline").with(Guardrail::Quote);

    let rows = scorecard(&[correct], &verdicts, &[full]);
    let row = &rows[0];

    assert!(
        row.prevented.is_empty(),
        "nothing wrong was caught: {row:?}"
    );
    assert!(row.escaped.is_empty());
    assert!(
        row.unknown.is_empty(),
        "it is not unknown — the recording says it was right"
    );
}

/// A verdict on a candidate that never became an assertion (#432).
///
/// The scored decision for a stopped claim says `NeedsReview` and
/// nothing else — the value it would have asserted is not in the eval
/// record. So the verdict has to come from the trace's own `candidate`,
/// compared against the authored expectation through the same
/// comparison the assertion path uses. A parallel comparison here would
/// be a second oracle, and today one line of it called 2,629 of 3,028
/// items wrong.
///
/// This is the case that needs no derivation to settle: the party is
/// the letter's own words on both sides, so a disagreement is wrong
/// whatever the deadline resolver would have made of the rest.
#[test]
fn a_stopped_candidate_naming_the_wrong_party_is_wrong_without_any_derivation() {
    let mut trace = wrong_claim_stopped_by_quote();
    trace.candidate = serde_json::json!({
        "kind": "payment",
        "party": "Someone Else Entirely",
        "ask": "Pay £84.00",
        "deadline": "within 14 days",
        "anchor": "14 days",
    });

    let expected = runner::eval::Extracted::Obligation(runner::eval::ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Harborne Parking Services".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "14 days".to_owned(),
        due: None,
    });

    assert_eq!(
        runner::eval::ablation::verdict_for(&trace, Some(&expected)),
        CandidateVerdict::Wrong,
        "the party disagrees, and no derivation could reconcile that"
    );
}

/// No expectation, no verdict. A claim the bed authored nothing about
/// cannot be judged, and guessing would be the detector authoring its
/// own oracle (#268).
#[test]
fn a_stopped_candidate_with_nothing_authored_about_it_is_unknown() {
    let trace = wrong_claim_stopped_by_quote();

    assert_eq!(
        runner::eval::ablation::verdict_for(&trace, None),
        CandidateVerdict::Unknown,
        "unjudgeable is a finding, not a pass"
    );
}
