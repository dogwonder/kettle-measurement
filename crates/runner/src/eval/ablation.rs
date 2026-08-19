//! Scoring one claim-lifecycle trace under several system policies
//! (#432).
//!
//! A model leaderboard cannot say whether a model is useful, or whether
//! a guardrail is supplying the reliability the product claims. The
//! only way to separate those is to score complete *policies* — the
//! deterministic floor, a raw candidate, schema-constrained output,
//! schema plus routing, the full pipeline — against the same task
//! outcomes.
//!
//! Deriving the intermediate ones by re-reading a recorded trace is
//! what makes that affordable and what keeps it honest. Affordable,
//! because a policy costs a rescore rather than a bed run. Honest,
//! because the alternative is keeping unsafe modes reachable in order
//! to benchmark them, and a mode that exists to be measured is a mode
//! somebody can run.

use crate::claim_trace::{CheckOutcome, ClaimKind, ClaimTrace, Guardrail};
use crate::eval::{ExpectedObligation, Extracted};
use std::collections::{BTreeMap, BTreeSet};

/// A complete system, named by which boundaries it applies.
///
/// A policy is defined by what it *has*, never by what it removes, so
/// the floor is an empty set rather than a full pipeline with holes
/// knocked in it. That ordering matters when a guardrail is added: a
/// new boundary joins the policies that declare it and leaves the
/// others describing the same system they described yesterday.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub name: String,
    pub active: BTreeSet<Guardrail>,
}

impl Policy {
    pub fn named(name: &str) -> Policy {
        Policy {
            name: name.to_owned(),
            active: BTreeSet::new(),
        }
    }

    /// Builder, because a policy is read far more often than it is
    /// built and the list of boundaries is the thing worth seeing.
    #[must_use]
    pub fn with(mut self, guardrail: Guardrail) -> Policy {
        self.active.insert(guardrail);
        self
    }
}

/// What a recording can say about the value a candidate proposed.
///
/// The third case is the point. Between a raw candidate and a scored
/// assertion sit the deterministic derivation stages, and a policy
/// missing a guard still runs them — but for a claim stopped early
/// those stages never ran, so what they would have produced is not in
/// the recording and cannot be replayed out of it.
///
/// #470's `PipelineIntroducedError` is what makes that matter rather
/// than being pedantry: derivation turning a *correct* candidate into a
/// wrong assertion is invisible to a candidate-level comparison. Count
/// every stopped claim as an error prevented and the scorecard credits
/// the pipeline with catching harm it introduces itself, which inverts
/// the question the ablation exists to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateVerdict {
    /// The recording shows the proposed value was already wrong.
    Wrong,
    /// The recording shows it was right.
    Correct,
    /// It looked right where it was stopped, and what the rest of the
    /// pipeline would have made of it is not recoverable from a
    /// recording.
    Unknown,
}

/// What one policy did with the wrong claims in a trace.
///
/// Claim ids rather than counts, so two rows can be checked against
/// each other: a claim contained by one policy and escaped by another
/// must be the *same* claim, or the policies were scored against
/// different things and the comparison says nothing. Counts cannot
/// carry that check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PolicyRow {
    pub policy: String,
    /// Wrong candidates this policy would have asserted.
    pub escaped: Vec<String>,
    /// Candidates one of its boundaries stopped that the recording
    /// shows were already wrong — a demonstrated catch.
    pub prevented: Vec<String>,
    /// Candidates it stopped that looked right where they were
    /// stopped. Never counted as prevented, and never dropped either:
    /// the size of this column is how a reader sees how tight the
    /// bound on `prevented` is for this row.
    pub unknown: Vec<String>,
}

/// Score every policy against one trace and one set of wrong claims.
///
/// Correctness is an input, never derived here: which claims were wrong
/// is the eval's authored judgement, and a scorecard that re-decided it
/// would be a detector authoring its own oracle (#268).
pub fn scorecard(
    traces: &[ClaimTrace],
    verdicts: &BTreeMap<String, CandidateVerdict>,
    policies: &[Policy],
) -> Vec<PolicyRow> {
    policies
        .iter()
        .map(|policy| {
            let mut row = PolicyRow {
                policy: policy.name.clone(),
                ..PolicyRow::default()
            };
            for trace in traces {
                let verdict = verdicts.get(&trace.id).copied();
                let stopped = stopped_by(trace, policy);
                match (verdict, stopped) {
                    (Some(CandidateVerdict::Wrong), true) => row.prevented.push(trace.id.clone()),
                    (Some(CandidateVerdict::Wrong), false) => row.escaped.push(trace.id.clone()),
                    (Some(CandidateVerdict::Unknown), true) => row.unknown.push(trace.id.clone()),
                    // A candidate the recording cannot judge and no
                    // boundary stopped was asserted, and asserting
                    // something unjudgeable is not the same finding as
                    // asserting something wrong. It belongs in a
                    // burden column rather than a harm one, and
                    // guessing which would be the collapse #432
                    // forbids — so it is carried nowhere until there
                    // is a column that means it.
                    (Some(CandidateVerdict::Unknown), false) => {}
                    // Right, and either stopped (review spent on
                    // something that did not need it) or asserted
                    // (the pipeline working). Neither is harm.
                    (Some(CandidateVerdict::Correct), _) => {}
                    (None, _) => {}
                }
            }
            row
        })
        .collect()
}

/// Whether a boundary this policy applies actually stopped the
/// candidate.
///
/// `Failed` only. `Warned` and `ChangedValue` are deliberately not
/// containment: a warning let the candidate through (#460 rule 2), and
/// a changed value is the pipeline's own act on a candidate that was
/// never at fault (#470). Counting either as a catch would credit a
/// policy with reliability it did not supply, which is the one thing
/// this scorecard exists to measure.
fn stopped_by(trace: &ClaimTrace, policy: &Policy) -> bool {
    trace.checks.iter().any(|check| {
        policy.active.contains(&check.guardrail) && check.outcome == CheckOutcome::Failed
    })
}

/// What a recording can say about the value one candidate proposed.
///
/// For a claim that *was* asserted the eval has already judged it and
/// `scored_assertion_is_wrong` is the answer. This is the other case:
/// a candidate a guard stopped, whose value is absent from the scored
/// decision — `NeedsReview` carries a reason and nothing else — and so
/// has to be read back off the trace.
///
/// The comparison routes through [`Extracted::same_assertion_as`], the
/// same one the assertion path uses. That is the whole discipline here:
/// a parallel comparison would be a second oracle, and a plausible one
/// written in a hurry today called 2,629 of 3,028 real items wrong.
///
/// `Unknown` is returned rather than guessed wherever the recording
/// cannot settle it — no authored expectation, or a candidate that does
/// not parse into the shape the expectation is in. Unjudgeable is a
/// finding; a silent `Correct` would flatter every policy that stopped
/// something.
pub fn verdict_for(trace: &ClaimTrace, expected: Option<&Extracted>) -> CandidateVerdict {
    let Some(expected) = expected else {
        return CandidateVerdict::Unknown;
    };
    let Some(proposed) = proposed_assertion(trace) else {
        return CandidateVerdict::Unknown;
    };
    if expected.same_assertion_as(&proposed) {
        return CandidateVerdict::Correct;
    }
    // They disagree — but a candidate carries no resolved `due`, so a
    // disagreement that lives only there is the deadline resolver not
    // having run rather than the model being wrong. Attributing it to
    // the model would book a pipeline stage's absence as a model error,
    // and in the direction that flatters every guard that stopped
    // something.
    //
    // Only the negative case needs this rule. Equality above is the
    // ordinary comparison, unchanged, so the oracle stays single for
    // every claim it can settle.
    if disagrees_only_in_derived_fields(expected, &proposed) {
        return CandidateVerdict::Unknown;
    }
    CandidateVerdict::Wrong
}

/// Whether everything the model itself supplied agrees, leaving only
/// what a later deterministic stage would have produced.
fn disagrees_only_in_derived_fields(expected: &Extracted, proposed: &Extracted) -> bool {
    match (expected, proposed) {
        (Extracted::Obligation(want), Extracted::Obligation(got)) => {
            want.kind == got.kind
                && want.party == got.party
                && want.deadline == got.deadline
                && want.anchor == got.anchor
        }
        _ => false,
    }
}

/// An obligation exactly as a model answers one.
///
/// Neither of the runner's own obligation types is this shape, and that
/// is not an oversight in either: `run::Obligation` is enriched with
/// the resolved `due` and the passages it was read from, and
/// `ExpectedObligation` is what a bed author writes. The wire contract
/// is narrower than both — five strings, constrained by the pack's
/// schema — and a recorded candidate is that and nothing else.
///
/// So this parses the contract rather than re-deriving anyone's
/// meaning. The comparison still goes through
/// [`Extracted::same_assertion_as`].
#[derive(serde::Deserialize)]
struct ProposedObligation {
    kind: String,
    party: String,
    deadline: String,
    anchor: String,
}

/// The candidate as the shape an expectation is compared against.
///
/// `None` when the trace's own kind has no such shape, or the candidate
/// does not carry one — both of which mean "this recording cannot say",
/// never "it was fine".
fn proposed_assertion(trace: &ClaimTrace) -> Option<Extracted> {
    match trace.kind {
        // The model's own answer shape, not the expectation's. They
        // differ in exactly one field and that field is the whole
        // difficulty: `due` is resolved by `builtin:timeline-sort`
        // after the guards, so a candidate a guard stopped never got
        // one. It is `None` here for that reason and not as a default.
        ClaimKind::Obligation => {
            serde_json::from_value::<ProposedObligation>(trace.candidate.clone())
                .ok()
                .map(|proposed| {
                    Extracted::Obligation(ExpectedObligation {
                        kind: proposed.kind,
                        party: proposed.party,
                        deadline: proposed.deadline,
                        anchor: proposed.anchor,
                        due: None,
                    })
                })
        }
        _ => None,
    }
}

// ## Where this was left, 19 August 2026 — resume here (#432)
//
// What works: a policy ladder scores a trace, and a single candidate
// gets a verdict. What does not exist yet is anything that reads a real
// recording, so this module has never produced a number from the
// archive it was built for.
//
// The next cycle is `verdicts(items, traces) -> BTreeMap<String,
// CandidateVerdict>`, joining the two halves of a recording:
//
//   * a claim that became an assertion was already judged when the run
//     was scored, so reuse `eval::fixture::scored_assertion_is_wrong`
//     — the same predicate `containment_metrics` counts escaped errors
//     with. It is private and would need `pub(super)`. Do not write a
//     second one: a plausible correctness heuristic tried on 19 August
//     called 2,629 of 3,028 real items wrong, because `outcome:
//     absent` is the *correct* answer for a no-obligation item;
//   * a claim a guard stopped was never scored at all, so its verdict
//     comes from `verdict_for` against the trace's candidate.
//
// Its failing test was written and is on the #432 issue thread rather
// than here, so this file stays green.
//
// Then, in order: the floor policy (an empty guardrail set — the row
// every model policy is read against), a walk over a run directory plus
// its baseline, and a CLI seam. Only then is there a scorecard.
//
// Two things known and unfinished:
//
//   * `proposed_assertion` handles `ClaimKind::Obligation` and returns
//     `None` for every other kind, so the renewal pack's `policy-term`
//     claims would come out entirely `Unknown` — honest, and useless
//     for renewal. It wants its own cycle once the letter path
//     produces real rows.
//   * An integration test over real data has nowhere to read a trace
//     from: baselines are committed, `claims.json` files are not
//     (`evals/runs/` is gitignored and the archive is a separate
//     repository). A committed sample trace is a prerequisite for
//     testing this against anything real, and no test may depend on a
//     gitignored path.
