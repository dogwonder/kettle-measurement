//! The path from a model answer to a claim Kettle was willing to keep
//! (#425).
//!
//! Raw exchanges answer "what did the model say?" and scored items
//! answer "was the finished result right?". A claim trace holds the
//! missing middle: which guardrails a candidate reached and the one
//! disposition it ended with. It is diagnostic evidence, never report
//! copy and never an input to a product decision.

use serde::{Deserialize, Serialize};

pub const CLAIM_TRACE_SCHEMA: &str = "kettle/claim-traces@0";

/// The versioned diagnostic document stored beside a run's ordinary
/// outputs. Versioning the envelope lets future traces add boundaries
/// without making old evidence ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTraceDocument {
    pub schema: String,
    pub claims: Vec<ClaimTrace>,
}

impl ClaimTraceDocument {
    pub fn new(claims: &[ClaimTrace]) -> Self {
        Self {
            schema: CLAIM_TRACE_SCHEMA.to_owned(),
            claims: claims.to_vec(),
        }
    }
}

/// What one trace is about. A decision is the model's answer for one
/// source item; obligations and terms are the claims nested inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Decision,
    Normalisation,
    Classification,
    Obligation,
    PolicyTerm,
}

/// A boundary a candidate crossed, or stopped at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Guardrail {
    Schema,
    Pairing,
    Quote,
    /// Rule 2 of #460: is the quote verbatim in exactly one passage of
    /// its document? A property of the document rather than of the
    /// claim, so it warns and never refuses.
    QuoteIdentifiesPassage,
    ValueShape,
    PackCoverage,
    DeterministicDerivation,
    ReviewRouting,
    Report,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Passed,
    Failed,
    NotApplicable,
    /// The stage neither passed the candidate through faithfully nor
    /// stopped it: it produced a different value from the one the
    /// model proposed (#470). Deliberately not `Failed`, because
    /// `Failed` is containment vocabulary — the candidate's fault,
    /// caught — and this is the stage's own act on a candidate that
    /// was never at fault.
    ChangedValue,
    /// The check found the evidence weaker than it looks and let the
    /// candidate through anyway (#460 rule 2). Deliberately not
    /// `Failed` for the same reason `ChangedValue` is not: nothing was
    /// contained, and a person reading the trace should see a caution,
    /// not a catch.
    Warned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimCheck {
    pub guardrail: Guardrail,
    pub outcome: CheckOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The one fate of a candidate. Later output links do not change it: an
/// accepted claim may reach a report and action, while a needs-review
/// candidate remains needs-review however it is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalDisposition {
    Accepted,
    Rejected,
    NeedsReview,
    Deduplicated,
    AbsentAfterRetry,
    /// The model proposed correctly, every guardrail passed, and a
    /// deterministic stage derived something else from it (#470).
    /// Additive, never a redefinition: `Accepted` still means the
    /// model's proposal survived intact, and `Rejected`/`NeedsReview`
    /// still mean a model error was contained. This is the fate
    /// neither could name — twice on record: `resolve_deadline`
    /// promoting a corrupted date to "read and verified" (#435), and
    /// the eval's own join scoring a correct reading against the
    /// wrong passage (#457). Its fix is Rust with a failing test,
    /// never a prompt, tier or model choice — which is why collapsing
    /// it into the others sends a week to the wrong fix.
    PipelineIntroducedError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputLink {
    pub guardrail: Guardrail,
    pub id: String,
}

/// One time the model tried to answer the source item. A trace still
/// has exactly one terminal disposition: attempts explain how it got
/// there, including a schema retry or a targeted pairing re-ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimAttempt {
    pub ordinal: usize,
    pub candidate: serde_json::Value,
    pub schema: CheckOutcome,
    pub pairing: CheckOutcome,
}

/// One model decision or nested claim and the checks actually applied
/// to it. Fields are deliberately explicit: this document is evidence
/// and a missing value must not be mistaken for a check that passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTrace {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub pack: String,
    pub step: String,
    pub batch: usize,
    pub item: usize,
    pub candidate_index: usize,
    pub kind: ClaimKind,
    pub source: String,
    pub candidate: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<ClaimAttempt>,
    pub checks: Vec<ClaimCheck>,
    pub terminal: TerminalDisposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputLink>,
}

impl ClaimTrace {
    pub fn check(&self, guardrail: Guardrail) -> Option<CheckOutcome> {
        self.checks
            .iter()
            .find(|check| check.guardrail == guardrail)
            .map(|check| check.outcome)
    }

    /// Record what a deterministic stage made of this candidate
    /// (#470). `faithful` is the value a correct derivation of the
    /// model's proposal yields; `derived` is what the stage produced.
    /// The run itself cannot tell the two apart — the stage that
    /// corrupted #435 was the same code that would have to check it —
    /// so the caller is whoever established the fact: a failing unit
    /// test, an eval join, or #432's ablation harness.
    ///
    /// When they agree, the derivation boundary is recorded as passed
    /// and the disposition stands. When they differ on a candidate
    /// every guardrail passed, the fate becomes
    /// [`TerminalDisposition::PipelineIntroducedError`] — never
    /// `accepted`, because what survived is not what the model
    /// proposed, and never a contained disposition, because there was
    /// no model error to contain. A candidate already contained stays
    /// contained: the person sees it either way, and the fault is
    /// still on the record in the check entry.
    pub fn record_derivation(&mut self, stage: &str, faithful: &str, derived: &str) {
        if faithful == derived {
            self.checks.push(ClaimCheck {
                guardrail: Guardrail::DeterministicDerivation,
                outcome: CheckOutcome::Passed,
                detail: Some(stage.to_owned()),
            });
            return;
        }
        self.checks.push(ClaimCheck {
            guardrail: Guardrail::DeterministicDerivation,
            outcome: CheckOutcome::ChangedValue,
            detail: Some(format!(
                "{stage}: a faithful derivation yields {faithful}, the stage derived {derived}"
            )),
        });
        if self.terminal == TerminalDisposition::Accepted {
            self.terminal = TerminalDisposition::PipelineIntroducedError;
        }
    }
}

/// Connect accepted obligation candidates to the concrete report row
/// and proposed action built from their evidence. The matching fact is
/// the source passage retained by both sides; generated prose is never
/// used as identity.
pub fn link_letter_actions(traces: &mut [ClaimTrace], actions: &crate::results::ProposedActions) {
    for (index, action) in actions.actions.iter().enumerate() {
        let passages: Vec<&str> = action
            .evidence
            .iter()
            .filter(|(key, _)| key.starts_with("passage_"))
            .map(|(_, value)| value.as_str())
            .collect();
        for trace in traces.iter_mut().filter(|trace| {
            trace.kind == ClaimKind::Obligation
                && trace.terminal == TerminalDisposition::Accepted
                && passages
                    .iter()
                    .any(|passage| same_passage(&trace.source, passage))
        }) {
            trace.outputs.push(OutputLink {
                guardrail: Guardrail::Report,
                id: format!("results.json#/obligations/{index}"),
            });
            trace.outputs.push(OutputLink {
                guardrail: Guardrail::Action,
                id: action.id.clone(),
            });
        }
    }
}

fn same_passage(left: &str, right: &str) -> bool {
    let normal = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    normal(left) == normal(right)
}

/// Run-local id allocation and trace collection. Iteration through
/// batches and JSON arrays is deterministic, so replaying the same
/// recorded answers rebuilds the same ids without persisting hidden
/// state beside them.
#[derive(Debug)]
pub(crate) struct ClaimLedger {
    pack: String,
    next: usize,
    traces: Vec<ClaimTrace>,
}

impl ClaimLedger {
    pub(crate) fn new(pack: impl Into<String>) -> Self {
        Self {
            pack: pack.into(),
            next: 1,
            traces: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push(
        &mut self,
        parent_id: Option<String>,
        step: &str,
        batch: usize,
        item: usize,
        candidate_index: usize,
        kind: ClaimKind,
        source: &str,
        candidate: serde_json::Value,
        checks: Vec<ClaimCheck>,
        terminal: TerminalDisposition,
    ) -> String {
        let id = format!("claim-{:06}", self.next);
        self.next += 1;
        self.traces.push(ClaimTrace {
            id: id.clone(),
            parent_id,
            pack: self.pack.clone(),
            step: step.to_owned(),
            batch,
            item,
            candidate_index,
            kind,
            source: source.to_owned(),
            candidate,
            attempts: Vec::new(),
            checks,
            terminal,
            outputs: Vec::new(),
        });
        id
    }

    pub(crate) fn attach_attempts(&mut self, id: &str, mut attempts: Vec<ClaimAttempt>) {
        for (index, attempt) in attempts.iter_mut().enumerate() {
            attempt.ordinal = index + 1;
        }
        if let Some(trace) = self.traces.iter_mut().find(|trace| trace.id == id) {
            trace.attempts = attempts;
        }
    }

    pub(crate) fn finish(self) -> Vec<ClaimTrace> {
        self.traces
    }
}

pub(crate) fn check(guardrail: Guardrail, outcome: CheckOutcome) -> ClaimCheck {
    ClaimCheck {
        guardrail,
        outcome,
        detail: None,
    }
}
