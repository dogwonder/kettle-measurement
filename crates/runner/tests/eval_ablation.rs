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
use runner::eval::ablation::{pool, scorecard, CandidateVerdict, Policy, Recording};
use runner::eval::{ExtractionOutcome, ScoredDecision, ScoredItem};
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
                field: None,
                guardrail: Guardrail::Schema,
                outcome: CheckOutcome::Passed,
                detail: None,
            },
            ClaimCheck {
                field: None,
                guardrail: Guardrail::Pairing,
                outcome: CheckOutcome::Passed,
                detail: None,
            },
            ClaimCheck {
                field: None,
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

/// The expectation a letter obligation is scored against, differing
/// only in the party — the field the letter states in its own words, so
/// a disagreement there needs no derivation to settle.
fn obligation(party: &str) -> runner::eval::Extracted {
    runner::eval::Extracted::Obligation(runner::eval::ExpectedObligation {
        kind: "payment".to_owned(),
        party: party.to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "14 days".to_owned(),
        amount: "no amount".to_owned(),
        due: None,
    })
}

/// A candidate the guards stopped, exactly as the model proposed it:
/// no `due`, because the resolver never ran on it.
fn stopped_candidate(deadline: &str, anchor: &str) -> ClaimTrace {
    let mut trace = wrong_claim_stopped_by_quote();
    trace.candidate = serde_json::json!({
        "kind": "payment", "party": "Elmswood Lettings", "ask": "Clear the balance",
        "deadline": deadline, "anchor": anchor
    });
    trace
}

/// #554: a stopped candidate carries no resolved day, so the scorecard
/// compares what the model said. Verbatim, that was a fourth definition
/// of the same obligation — the pair `same_assertion_as` now calls one
/// reading was booked as a wrong candidate the guard prevented. Read
/// through the deadline's signature, a faithful copy is unjudgeable
/// (no day to settle it) and a different interval is wrong.
#[test]
fn a_stopped_candidate_worded_like_the_letter_is_not_booked_as_prevented() {
    use runner::eval::ablation::{verdict_for, CandidateVerdict};
    let expected = runner::eval::Extracted::Obligation(runner::eval::ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Elmswood Lettings".to_owned(),
        deadline: "within 45 days".to_owned(),
        anchor: "23 August 2026".to_owned(),
        amount: "no amount".to_owned(),
        due: Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 7).expect("a date")),
    });
    assert_eq!(
        verdict_for(
            &stopped_candidate("within 45 days of 23 August 2026", ""),
            Some(&expected)
        ),
        CandidateVerdict::Unknown,
        "the anchor left in the phrase is the same base by another route"
    );
    assert_eq!(
        verdict_for(
            &stopped_candidate("within 46 days", "23 August 2026"),
            Some(&expected)
        ),
        CandidateVerdict::Wrong,
        "a different count is a different day, whatever the letter date"
    );
    assert_eq!(
        verdict_for(&stopped_candidate("by 7 October 2026", ""), Some(&expected)),
        CandidateVerdict::Wrong,
        "a computed date is not the counted deadline the letter wrote"
    );
}

/// One scored decision, linked to the claim it was read from.
fn scored_item(
    trace_id: &str,
    expected: Option<runner::eval::Extracted>,
    actual: ExtractionOutcome,
) -> ScoredItem {
    ScoredItem {
        id: format!("app.kttl.letter-to-actions/fixture-01/{trace_id}"),
        item_id: trace_id.to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:test".to_owned(),
        fixture: "letter-01.txt".to_owned(),
        fixture_id: "fixture-01".to_owned(),
        strata: vec!["any".to_owned()],
        raw_input: "Pay £84.00 within 14 days.".to_owned(),
        decision_key: "Pay £84.00 within 14 days.".to_owned(),
        decision: ScoredDecision::Extraction {
            expected,
            expected_review: false,
            unauthored_negative: false,
            actual,
        },
        evidence: Default::default(),
        trace_ids: vec![trace_id.to_owned()],
        confidence: None,
        exchanges: Vec::new(),
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
        amount: "no amount".to_owned(),
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

/// The two halves of a recording, joined into one map (#432).
///
/// A recording says nothing about candidates in one voice. A claim that
/// became an assertion was judged when the run was scored, and the
/// scorer's own predicate is the answer — reusing it rather than
/// writing a parallel one is the whole discipline of this module. A
/// claim a guard stopped was never scored at all: its decision says
/// `NeedsReview` and carries no value, so its verdict has to be read
/// back off the trace's candidate.
///
/// Both must land in one map keyed by claim id, or the policy rows
/// cannot be read against each other.
#[test]
fn verdicts_cover_the_asserted_half_and_the_stopped_half_of_one_recording() {
    let mut asserted = wrong_claim_stopped_by_quote();
    asserted.id = "claim-asserted".to_owned();
    asserted.checks = vec![ClaimCheck {
        field: None,
        guardrail: Guardrail::Quote,
        outcome: CheckOutcome::Passed,
        detail: None,
    }];
    asserted.terminal = TerminalDisposition::Accepted;

    let mut stopped = wrong_claim_stopped_by_quote();
    stopped.id = "claim-stopped".to_owned();
    stopped.candidate = serde_json::json!({
        "kind": "payment",
        "party": "Someone Else Entirely",
        "ask": "Pay £84.00",
        "deadline": "within 14 days",
        "anchor": "14 days",
    });

    let items = vec![
        // Scored, and scored wrong: the run asserted the wrong party.
        scored_item(
            "claim-asserted",
            Some(obligation("Harborne Parking Services")),
            ExtractionOutcome::Found {
                extracted: obligation("Someone Else Entirely"),
            },
        ),
        // Routed to a person, so the eval never judged a value.
        scored_item(
            "claim-stopped",
            Some(obligation("Harborne Parking Services")),
            ExtractionOutcome::NeedsReview {
                reason: "quote not found in the source".to_owned(),
            },
        ),
    ];

    let verdicts = runner::eval::ablation::verdicts(&items, &[asserted, stopped]);

    assert_eq!(
        verdicts.get("claim-asserted"),
        Some(&CandidateVerdict::Wrong),
        "the eval already judged this one: {verdicts:?}"
    );
    assert_eq!(
        verdicts.get("claim-stopped"),
        Some(&CandidateVerdict::Wrong),
        "nothing scored this one, so its candidate is read off the \
         trace: {verdicts:?}"
    );
}

/// A right assertion is `Correct`, not merely "not wrong".
///
/// The distinction is load-bearing: `Unknown` and `Correct` are read
/// differently by [`scorecard`], and a correct candidate a guard
/// stopped is review spent needlessly rather than an unknowable bound.
#[test]
fn an_assertion_the_eval_scored_right_is_correct_rather_than_unknown() {
    let mut trace = wrong_claim_stopped_by_quote();
    trace.id = "claim-right".to_owned();

    let items = vec![scored_item(
        "claim-right",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::Found {
            extracted: obligation("Harborne Parking Services"),
        },
    )];

    let verdicts = runner::eval::ablation::verdicts(&items, &[trace]);

    assert_eq!(
        verdicts.get("claim-right"),
        Some(&CandidateVerdict::Correct),
        "the recording says it was right: {verdicts:?}"
    );
}

/// An item judging several candidates at once settles none of them.
///
/// A passage yielding two candidates links both to the item scored
/// from it, and the scored decision judges the *item*. Which of the two
/// it judged is not in the recording, so attributing the verdict to
/// either is a guess — and a guess in the direction that flatters
/// whichever guard stopped the other one.
#[test]
fn an_item_linking_two_different_candidates_attributes_its_verdict_to_neither() {
    let mut first = wrong_claim_stopped_by_quote();
    first.id = "claim-a".to_owned();
    // Genuinely a *different* candidate, which this test previously was
    // not: it cloned one trace twice and called them two. Two proposals
    // of the same assertion cannot disagree about a verdict, so that
    // data exercised the judgeable case while asserting the
    // unjudgeable one — and the rule it exists to hold is about
    // candidates that could disagree.
    let mut second = wrong_claim_stopped_by_quote();
    second.id = "claim-b".to_owned();
    second.candidate = serde_json::json!({
        "kind": "payment",
        "party": "A Different Company Entirely",
        "deadline": "within 7 days",
        "anchor": "the date of this letter",
    });

    let mut item = scored_item(
        "claim-a",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::Found {
            extracted: obligation("Someone Else Entirely"),
        },
    );
    item.trace_ids = vec!["claim-a".to_owned(), "claim-b".to_owned()];

    let verdicts = runner::eval::ablation::verdicts(&[item], &[first, second]);

    assert_eq!(
        verdicts.get("claim-a"),
        Some(&CandidateVerdict::Unknown),
        "one scored decision cannot name which candidate it judged: {verdicts:?}"
    );
    assert_eq!(
        verdicts.get("claim-b"),
        Some(&CandidateVerdict::Unknown),
        "and the same is true of the other: {verdicts:?}"
    );
}

/// The deterministic floor is the row every model policy is read
/// against, and a harm-only row would rank it first (#432).
///
/// The floor runs no model, so no model claim can escape it: its harm
/// columns are empty by construction, exactly as a perfect system's
/// would be. What separates them is that it asserted nothing, and if
/// the row cannot say so then the scorecard has collapsed safety and
/// usefulness into one number whose winner is always the system that
/// answers no questions — the collapse this issue forbids.
///
/// So a row carries what the policy *answered* beside what it got
/// wrong, and the floor's is empty.
#[test]
fn the_deterministic_floor_answers_nothing_and_its_row_says_so() {
    let traces = vec![wrong_claim_stopped_by_quote()];
    let verdicts = BTreeMap::from([("claim-000042".to_owned(), CandidateVerdict::Wrong)]);

    let rows = scorecard(&traces, &verdicts, &[Policy::floor()]);
    let floor = &rows[0];

    assert!(
        floor.escaped.is_empty(),
        "a system with no model cannot assert a model's error: {floor:?}"
    );
    assert!(
        floor.prevented.is_empty(),
        "and it caught nothing either — there was no candidate: {floor:?}"
    );
    assert!(
        floor.answered.is_empty(),
        "the column that stops an empty harm row reading as safety: {floor:?}"
    );
}

/// The rung directly above the floor: a model with no boundaries at
/// all. Everything it proposes is asserted, so its answered column is
/// full and its wrong candidates are all escapes.
#[test]
fn a_model_with_no_boundaries_answers_everything_and_escapes_its_errors() {
    let mut right = wrong_claim_stopped_by_quote();
    right.id = "claim-000045".to_owned();

    let traces = vec![wrong_claim_stopped_by_quote(), right];
    let verdicts = BTreeMap::from([
        ("claim-000042".to_owned(), CandidateVerdict::Wrong),
        ("claim-000045".to_owned(), CandidateVerdict::Correct),
    ]);

    let rows = scorecard(&traces, &verdicts, &[Policy::named("raw-candidate")]);
    let raw = &rows[0];

    assert_eq!(
        raw.answered,
        vec!["claim-000042".to_owned(), "claim-000045".to_owned()],
        "no boundary stopped anything, so both were asserted: {raw:?}"
    );
    assert_eq!(
        raw.escaped,
        vec!["claim-000042".to_owned()],
        "and the wrong one reached the person: {raw:?}"
    );
}

/// A guard removes a claim from what the policy answered, whatever the
/// recording says about the candidate. Containment is a fact about the
/// pipeline; the verdict is a separate question, and a stopped claim is
/// unanswered under both.
#[test]
fn a_stopped_claim_is_not_among_what_the_policy_answered() {
    let traces = vec![wrong_claim_stopped_by_quote()];
    let verdicts = BTreeMap::from([("claim-000042".to_owned(), CandidateVerdict::Wrong)]);

    let full = Policy::named("full-pipeline").with(Guardrail::Quote);
    let rows = scorecard(&traces, &verdicts, &[full]);
    let row = &rows[0];

    assert!(
        row.answered.is_empty(),
        "the quote guard stopped it, so nobody was told it: {row:?}"
    );
    assert_eq!(row.prevented, vec!["claim-000042".to_owned()]);
}

/// A claim id is a per-run counter, so pooling fixtures on the bare id
/// merges different claims (#432).
///
/// `claim-000042` in one fixture and `claim-000042` in the next are two
/// unrelated candidates that happen to have been the forty-second thing
/// each run recorded. A scorecard pooled on the bare id would score one
/// against the other's expectation — precisely the "scored against
/// different things" failure the claim-id column exists to make
/// impossible.
#[test]
fn pooling_two_fixtures_keeps_their_identically_numbered_claims_apart() {
    let wrong = Recording {
        fixture: "letter-01.txt".to_owned(),
        traces: vec![wrong_claim_stopped_by_quote()],
        items: vec![scored_item(
            "claim-000042",
            Some(obligation("Harborne Parking Services")),
            ExtractionOutcome::Found {
                extracted: obligation("Someone Else Entirely"),
            },
        )],
    };
    // Same id, different fixture, and this one was read correctly.
    let right = Recording {
        fixture: "letter-02.txt".to_owned(),
        traces: vec![wrong_claim_stopped_by_quote()],
        items: vec![scored_item(
            "claim-000042",
            Some(obligation("Harborne Parking Services")),
            ExtractionOutcome::Found {
                extracted: obligation("Harborne Parking Services"),
            },
        )],
    };

    let pooled = pool(&[wrong, right]);

    assert_eq!(
        pooled.verdicts.len(),
        2,
        "two claims, not one merged pair: {:?}",
        pooled.verdicts
    );
    assert_eq!(
        pooled.traces.len(),
        2,
        "and two traces to score them against"
    );
    let verdicts: Vec<CandidateVerdict> = pooled.verdicts.values().copied().collect();
    assert!(
        verdicts.contains(&CandidateVerdict::Wrong)
            && verdicts.contains(&CandidateVerdict::Correct),
        "each fixture keeps its own answer: {:?}",
        pooled.verdicts
    );
    for id in pooled.verdicts.keys() {
        assert!(
            id.contains("letter-01.txt") || id.contains("letter-02.txt"),
            "a pooled id must name the recording it came from, or it \
             cannot be looked up again: {id}"
        );
    }
}

/// An item links the whole chain that produced it, and only one link is
/// a candidate value (#432).
///
/// Measured on the run that backs `evals/baseline-v14-letter.json`:
/// of 3,028 scored items, 2,148 link one `decision` trace, 510 link a
/// `decision` and the `obligation` nested inside it, and 370 link two
/// `decision` traces. Not one links two value-bearing candidates.
///
/// So a decision and its own obligation are not two candidates in
/// competition — they are a passage-level answer and the value read out
/// of it. Refusing to judge the pair would throw away every extraction
/// the letter bed scores. The item's verdict belongs to the trace that
/// carries the value it judged, and the #271 rule applies: take the
/// claim that answers *this* question, never whichever one is nearest.
#[test]
fn an_extraction_item_judges_the_obligation_nested_in_its_decision() {
    let mut decision = wrong_claim_stopped_by_quote();
    decision.id = "claim-000003".to_owned();
    decision.kind = ClaimKind::Decision;
    decision.checks = vec![ClaimCheck {
        field: None,
        guardrail: Guardrail::Schema,
        outcome: CheckOutcome::Passed,
        detail: None,
    }];
    decision.terminal = TerminalDisposition::Accepted;

    let mut nested = wrong_claim_stopped_by_quote();
    nested.id = "claim-000004".to_owned();
    nested.parent_id = Some("claim-000003".to_owned());

    let mut item = scored_item(
        "claim-000004",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::Found {
            extracted: obligation("Someone Else Entirely"),
        },
    );
    item.trace_ids = vec!["claim-000003".to_owned(), "claim-000004".to_owned()];

    let verdicts = runner::eval::ablation::verdicts(&[item], &[decision, nested]);

    assert_eq!(
        verdicts.get("claim-000004"),
        Some(&CandidateVerdict::Wrong),
        "the obligation is the value the item judged: {verdicts:?}"
    );
    assert_eq!(
        verdicts.get("claim-000003"),
        None,
        "and the decision it hung from was not judged as a value at \
         all, so it carries no verdict: {verdicts:?}"
    );
}

/// The 370 items whose only links are decisions (#432).
///
/// A passage the run read and asserted nothing from has no candidate
/// value on the record, so no candidate verdict can be read off it. It
/// is not `Unknown` either — nothing was stopped and nothing is
/// pending; there is simply no claim here for a guardrail to have
/// caught, which is why a miss is a constant across every policy rather
/// than a difference between them. It belongs in the end-to-end task
/// column, which does not exist yet.
#[test]
fn an_item_with_no_candidate_value_puts_nothing_in_the_harm_columns() {
    let mut first = wrong_claim_stopped_by_quote();
    first.id = "claim-000001".to_owned();
    first.kind = ClaimKind::Decision;
    let mut second = wrong_claim_stopped_by_quote();
    second.id = "claim-000007".to_owned();
    second.kind = ClaimKind::Decision;

    // Expected an obligation, asserted nothing: a miss, and the eval
    // rightly counts it as one. No guardrail could have changed it.
    let mut item = scored_item(
        "claim-000001",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::Absent,
    );
    item.trace_ids = vec!["claim-000001".to_owned(), "claim-000007".to_owned()];

    let verdicts = runner::eval::ablation::verdicts(&[item], &[first, second]);

    assert!(
        verdicts.is_empty(),
        "a miss is not a wrong candidate, and booking it as one would \
         call every policy's misses containment failures: {verdicts:?}"
    );
}

/// A per-test scratch directory. Tests share one process, so pid plus
/// name is unique enough (the posture the cache tests take).
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kettle-ablation-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// One fixture's run directory, as the harness writes it.
fn write_recording(runs: &std::path::Path, pack: &str, model: &str, fixture: &str) {
    let dir = runs.join(runner::run_dir::eval_run_id(pack, Some(model), fixture));
    std::fs::create_dir_all(&dir).expect("create run dir");

    let mut decision = wrong_claim_stopped_by_quote();
    decision.id = "claim-000003".to_owned();
    decision.kind = ClaimKind::Decision;
    let mut nested = wrong_claim_stopped_by_quote();
    nested.id = "claim-000004".to_owned();
    nested.parent_id = Some("claim-000003".to_owned());
    nested.candidate = serde_json::json!({
        "kind": "payment",
        "party": "Someone Else Entirely",
        "ask": "Pay £84.00",
        "deadline": "within 14 days",
        "anchor": "14 days",
    });

    let claims = runner::claim_trace::ClaimTraceDocument::new(&[decision, nested]);
    std::fs::write(
        dir.join("claims.json"),
        serde_json::to_string(&claims).expect("claims serialise"),
    )
    .expect("write claims.json");

    let mut item = scored_item(
        "claim-000004",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::NeedsReview {
            reason: "quote not found in the source".to_owned(),
        },
    );
    item.trace_ids = vec!["claim-000003".to_owned(), "claim-000004".to_owned()];
    std::fs::write(
        dir.join("eval-items.json"),
        serde_json::to_string(&[item]).expect("items serialise"),
    )
    .expect("write eval-items.json");
}

/// The walk, over a directory laid out the way the harness lays one out
/// (#432).
///
/// A run directory is self-describing — `claims.json` is what the
/// pipeline recorded and `eval-items.json` is what the harness scored
/// from it — so the walk needs no third file to join them. Which
/// directory belongs to which fixture is **constructed** from the same
/// function the harness names it with, never parsed back out of the
/// directory name (#303).
#[test]
fn a_walk_over_a_runs_directory_scores_the_policies_from_its_own_recordings() {
    let runs = scratch("walk");
    let pack = "app.kttl.letter-to-actions";
    let model = "qwen3.5-4b-q4_k_m.gguf";
    write_recording(&runs, pack, model, "letter-01.txt");
    write_recording(&runs, pack, model, "letter-02.txt");

    let walk = runner::eval::ablation::walk(
        &runs,
        pack,
        Some(model),
        &["letter-01.txt".to_owned(), "letter-02.txt".to_owned()],
    );

    assert!(
        walk.missing.is_empty(),
        "both recordings are on disk: {:?}",
        walk.missing
    );

    let pooled = pool(&walk.recordings);
    let full = Policy::named("full-pipeline").with(Guardrail::Quote);
    let rows = scorecard(&pooled.traces, &pooled.verdicts, &[Policy::floor(), full]);

    assert!(rows[0].answered.is_empty(), "the floor answered nothing");
    assert_eq!(
        rows[1].prevented,
        vec![
            "letter-01.txt#claim-000004".to_owned(),
            "letter-02.txt#claim-000004".to_owned(),
        ],
        "the quote guard caught the same wrong candidate in both \
         fixtures, and the ids say which was which: {:?}",
        rows[1]
    );
}

/// A recording that is not on disk is named, never counted as clean.
///
/// A scorecard assembled from half a run would otherwise report smaller
/// harm columns and look like a better policy.
#[test]
fn a_fixture_whose_recording_is_missing_is_reported_rather_than_skipped() {
    let runs = scratch("missing");
    let pack = "app.kttl.letter-to-actions";
    let model = "qwen3.5-4b-q4_k_m.gguf";
    write_recording(&runs, pack, model, "letter-01.txt");

    let walk = runner::eval::ablation::walk(
        &runs,
        pack,
        Some(model),
        &["letter-01.txt".to_owned(), "letter-02.txt".to_owned()],
    );

    assert_eq!(walk.recordings.len(), 1);
    assert_eq!(
        walk.missing,
        vec!["letter-02.txt".to_owned()],
        "a scorecard built from half a run must say so: {:?}",
        walk.missing
    );
}

/// The ladder is built from the boundaries the recording actually
/// exercised (#432).
///
/// A fixed ladder would invent rungs: a pack that never runs a check
/// would get a policy indistinguishable from the one below it, and a
/// reader would take two identical rows for evidence that the boundary
/// between them costs nothing. So the rungs come from what was
/// observed, cumulatively, in the order the pipeline applies them —
/// which is what makes "this guard is where the reliability comes from"
/// a readable claim rather than an inference.
#[test]
fn the_ladder_is_the_observed_boundaries_added_one_at_a_time() {
    let observed = [
        Guardrail::Quote,
        Guardrail::Schema,
        Guardrail::ReviewRouting,
    ]
    .into_iter()
    .collect();

    let names: Vec<String> = Policy::ladder(&observed)
        .into_iter()
        .map(|policy| policy.name)
        .collect();

    assert_eq!(
        names,
        vec![
            "deterministic-floor".to_owned(),
            "no-boundaries".to_owned(),
            "schema".to_owned(),
            "schema+quote".to_owned(),
            "schema+quote+review-routing".to_owned(),
        ],
        "the floor, a bare model, then one boundary at a time in \
         pipeline order"
    );
}

/// A boundary the recording never applied gets no rung.
#[test]
fn a_boundary_nothing_exercised_is_not_a_rung() {
    let observed = [Guardrail::Schema].into_iter().collect();
    let rungs = Policy::ladder(&observed);

    assert_eq!(rungs.len(), 3, "floor, bare model, schema: {rungs:?}");
    assert!(
        rungs
            .iter()
            .all(|policy| !policy.active.contains(&Guardrail::Action)),
        "nothing claims a boundary the run never reached: {rungs:?}"
    );
}

/// Honest low confidence reaches a person without failing any semantic
/// check, and that is still containment (#432).
///
/// Measured on the run behind `evals/baseline-v14-letter.json`: 69
/// claims ended in `needs_review` and exactly **one** of them failed a
/// review-routing check. Read through failed checks alone, a policy
/// with review routing would be credited with stopping one claim when
/// it stopped sixty-nine — and the reliability the routing supplies
/// would be booked to the model instead.
///
/// `containment_metrics` already draws the line this way: review
/// routing is itself the boundary. So the fate of the claim counts, not
/// only the checks along the way.
#[test]
fn a_claim_routed_to_review_is_contained_even_with_no_failed_check() {
    let mut low_confidence = wrong_claim_stopped_by_quote();
    low_confidence.id = "claim-000070".to_owned();
    low_confidence.checks = vec![ClaimCheck {
        field: None,
        guardrail: Guardrail::ReviewRouting,
        outcome: CheckOutcome::Passed,
        detail: None,
    }];
    low_confidence.terminal = TerminalDisposition::NeedsReview;

    let verdicts = BTreeMap::from([("claim-000070".to_owned(), CandidateVerdict::Wrong)]);

    let without = Policy::named("schema").with(Guardrail::Schema);
    let with = Policy::named("schema+review-routing")
        .with(Guardrail::Schema)
        .with(Guardrail::ReviewRouting);
    let rows = scorecard(&[low_confidence], &verdicts, &[without, with]);

    assert_eq!(
        rows[0].escaped,
        vec!["claim-000070".to_owned()],
        "a policy without routing asserts it: {:?}",
        rows[0]
    );
    assert_eq!(
        rows[1].prevented,
        vec!["claim-000070".to_owned()],
        "a policy with routing puts it in front of a person, whatever \
         the checks say: {:?}",
        rows[1]
    );
    assert!(
        rows[1].answered.is_empty(),
        "and nobody was told it as a finding: {:?}",
        rows[1]
    );
}

/// Every column of a row shares one denominator (#432).
///
/// The letter run records 4,035 claims and can judge 510 of them: the
/// rest are passage-level decisions with no candidate value. A row
/// whose `answered` counted all 4,035 beside 12 escaped errors invites
/// the reading 12/4,035, and the population those twelve came out of is
/// 510. Two of the four columns would be about different things.
///
/// So a row describes the claims the recording can judge, and the
/// unjudged remainder is reported as what it is: the size of the blind
/// spot, not a denominator.
#[test]
fn a_policy_row_counts_only_the_claims_the_recording_can_judge() {
    let mut unjudged = wrong_claim_stopped_by_quote();
    unjudged.id = "claim-000100".to_owned();
    unjudged.kind = ClaimKind::Decision;
    unjudged.checks = vec![ClaimCheck {
        field: None,
        guardrail: Guardrail::Schema,
        outcome: CheckOutcome::Passed,
        detail: None,
    }];
    unjudged.terminal = TerminalDisposition::Accepted;

    let mut judged = unjudged.clone();
    judged.id = "claim-000101".to_owned();

    let verdicts = BTreeMap::from([("claim-000101".to_owned(), CandidateVerdict::Wrong)]);
    let rows = scorecard(
        &[unjudged, judged],
        &verdicts,
        &[Policy::named("schema").with(Guardrail::Schema)],
    );

    assert_eq!(
        rows[0].answered,
        vec!["claim-000101".to_owned()],
        "the claim with no verdict is in no column, because no column \
         could say anything true about it: {:?}",
        rows[0]
    );
}

/// What the policy actually got right for the person (#432).
///
/// Harm, containment and the bound on containment are three columns
/// about failure. None of them says whether the system was any use, and
/// a scorecard of failure columns alone still ranks the system that
/// answers nothing first — `answered` only fixes half of that, because
/// it counts assertions without asking whether they were right.
///
/// So a row also carries what it delivered: claims it asserted *and*
/// got right. That is the end-to-end task column this issue asks for,
/// at the granularity a recording can support, and it is what makes a
/// Pareto frontier readable — the floor delivers nothing and harms
/// nobody, and every rung above it trades one against the other.
#[test]
fn a_row_counts_what_the_policy_got_right_as_well_as_what_it_got_wrong() {
    let mut right = wrong_claim_stopped_by_quote();
    right.id = "claim-right".to_owned();
    right.checks = vec![ClaimCheck {
        field: None,
        guardrail: Guardrail::Quote,
        outcome: CheckOutcome::Passed,
        detail: None,
    }];
    right.terminal = TerminalDisposition::Accepted;

    let mut wrong = right.clone();
    wrong.id = "claim-wrong".to_owned();

    let mut stopped = right.clone();
    stopped.id = "claim-stopped".to_owned();
    stopped.terminal = TerminalDisposition::NeedsReview;

    let verdicts = BTreeMap::from([
        ("claim-right".to_owned(), CandidateVerdict::Correct),
        ("claim-wrong".to_owned(), CandidateVerdict::Wrong),
        // Correct, but routed to a person: the answer was right and
        // the person still had to do the work.
        ("claim-stopped".to_owned(), CandidateVerdict::Correct),
    ]);

    let rows = scorecard(
        &[right, wrong, stopped],
        &verdicts,
        &[
            Policy::floor(),
            Policy::named("routing").with(Guardrail::ReviewRouting),
        ],
    );

    assert!(
        rows[0].delivered.is_empty(),
        "the floor delivered nothing, which is the cost its empty harm \
         columns hide: {:?}",
        rows[0]
    );
    assert_eq!(
        rows[1].delivered,
        vec!["claim-right".to_owned()],
        "one right answer asserted; the wrong one is not delivery and \
         the routed one was handed back to the person: {:?}",
        rows[1]
    );
}

/// The half of the harm no policy column can carry (#432, #474).
///
/// A miss is an authored expectation the run asserted **nothing** from.
/// It produces no claim, so the claim-lifecycle trace cannot see it and
/// no guardrail can act on it: containment operates on assertions, and
/// there is nothing here to contain. It is therefore constant across
/// every policy, and belongs beside the table rather than in it — a
/// column that read the same number in every row would suggest the
/// policies had been compared on it, and they cannot be.
///
/// It has to be reported *somewhere*, though, and that is what this
/// test is for. On 25 August 2026 the letter pack's escaped-error
/// columns read 0 across every rung while its eval reported seven wrong
/// answers on the sealed set, because every one of them was a miss. A
/// scorecard that showed only the first number would have said the
/// pipeline was clean on a bed where a person was told an invoice asked
/// nothing of them.
#[test]
fn an_authored_expectation_nothing_was_asserted_from_is_a_miss_outside_every_policy() {
    let asserted = scored_item(
        "claim-000001",
        Some(obligation("Ashcombe Housing Trust")),
        ExtractionOutcome::Found {
            extracted: obligation("Ashcombe Housing Trust"),
        },
    );
    let missed = scored_item(
        "claim-000002",
        Some(obligation("Belwood Insurance")),
        ExtractionOutcome::Absent,
    );
    // Asks nothing and nothing was asserted: the correct answer, and
    // never a miss. Counting it would make every courtesy line in the
    // bed read as harm.
    let correctly_silent = scored_item("claim-000003", None, ExtractionOutcome::Absent);
    // Surfaced to a person rather than asserted. Not a miss either:
    // the decision reached somebody, which is the honest floor working
    // rather than failing.
    let referred = scored_item(
        "claim-000004",
        Some(obligation("Cranleigh Energy")),
        ExtractionOutcome::NeedsReview {
            reason: "two readings disagree".to_owned(),
        },
    );

    let items = vec![asserted, missed, correctly_silent, referred];
    let missed_ids = runner::eval::ablation::misses(&items);

    assert_eq!(
        missed_ids,
        vec!["claim-000002".to_owned()],
        "only the authored expectation the run asserted nothing from is a miss"
    );
}

/// The renewal pack's whole unjudgeable rate, and why it is not a guess
/// to settle it (#432, #457).
///
/// A renewal run reads two documents, so the same passage is read
/// twice and proposes the same value twice. One scored decision then
/// links both claims, and refusing to judge either cost the renewal
/// scorecard 62% of its asserted claims — 222 of 222 ambiguous items on
/// the 25 August v16 recording link candidates that are **byte
/// identical**, and none links candidates that differ.
///
/// Settling those is not #457's anti-pattern. That rule says never
/// re-derive which claim a decision judged from its text, and this does
/// the opposite: it declines to pick between them, and observes that
/// picking cannot matter, because two proposals of the same assertion
/// reach the same verdict whichever one the decision meant. Where the
/// candidates differ, the choice would change the answer and Unknown
/// still stands.
#[test]
fn an_item_linking_two_identical_candidates_settles_both() {
    let mut first = wrong_claim_stopped_by_quote();
    first.id = "claim-a".to_owned();
    let mut second = wrong_claim_stopped_by_quote();
    second.id = "claim-b".to_owned();
    assert_eq!(
        first.candidate, second.candidate,
        "the premise of this test: the same passage read from two documents"
    );

    let mut item = scored_item(
        "claim-a",
        Some(obligation("Harborne Parking Services")),
        ExtractionOutcome::Found {
            extracted: obligation("Someone Else Entirely"),
        },
    );
    item.trace_ids = vec!["claim-a".to_owned(), "claim-b".to_owned()];

    let verdicts = runner::eval::ablation::verdicts(&[item], &[first, second]);

    assert_eq!(
        verdicts.get("claim-a"),
        Some(&CandidateVerdict::Wrong),
        "identical candidates cannot disagree, so the decision settles both: {verdicts:?}"
    );
    assert_eq!(
        verdicts.get("claim-b"),
        Some(&CandidateVerdict::Wrong),
        "and the same is true of the other: {verdicts:?}"
    );
}

/// A field-level check refuses one reading and leaves the obligation
/// standing (review of #626, Task 4): the amount is dropped, the ask
/// is still asserted. Crediting that as the guard *containing* the
/// claim would count an obligation that still reached the report as
/// prevented harm. The claim is asserted, and judged as asserted.
#[test]
fn a_refused_reading_leaves_the_claim_asserted_and_is_no_prevented_error() {
    let mut standing = wrong_claim_stopped_by_quote();
    standing.id = "claim-000045".to_owned();
    standing.terminal = TerminalDisposition::Accepted;
    standing.checks = vec![
        ClaimCheck {
            field: None,
            guardrail: Guardrail::Schema,
            outcome: CheckOutcome::Passed,
            detail: None,
        },
        ClaimCheck {
            field: None,
            guardrail: Guardrail::Pairing,
            outcome: CheckOutcome::Passed,
            detail: None,
        },
        ClaimCheck {
            field: Some("amount".to_owned()),
            guardrail: Guardrail::Quote,
            outcome: CheckOutcome::Failed,
            detail: Some("amount's value is not in passage 3: refused".to_owned()),
        },
    ];
    let verdicts = BTreeMap::from([("claim-000045".to_owned(), CandidateVerdict::Wrong)]);
    let full = Policy::named("full-pipeline")
        .with(Guardrail::Schema)
        .with(Guardrail::Pairing)
        .with(Guardrail::Quote);

    let rows = scorecard(&[standing], &verdicts, &[full]);
    let row = &rows[0];
    assert!(
        row.prevented.is_empty(),
        "removing an amount does not contain an obligation that still appears: {row:?}"
    );
    assert_eq!(
        row.escaped,
        vec!["claim-000045".to_owned()],
        "the claim was asserted, and a wrong assertion escaped: {row:?}"
    );
}
