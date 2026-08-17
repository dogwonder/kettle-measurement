//! #430: whether the evidence *supports* a claim, scored beside whether
//! its words exist.
//!
//! `quote_is_in` proves source fidelity — the words are really there.
//! It cannot prove semantic support: a verbatim passage can negate the
//! claim it is offered for, come from the wrong document, or omit the
//! qualifier that changes its meaning. Each dimension here is a
//! distinct recorded outcome on a scored item, so "wrong" can say *how*
//! it was wrong.
//!
//! Ground truth is human-authored fixture data. Support is decided by
//! set membership against what the bed's author declared — the
//! expectation itself is the supported claim, and `unsupported` names
//! the near misses worth diagnosing — never by any computed entailment
//! and never by a model's judgement.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The evidence questions a pack may declare it can score. The runner
/// owns this vocabulary; the pack owns which dimensions it declares and
/// the fixture data that answers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDimension {
    /// The quoted words are present in the source.
    Existence,
    /// The words come from the document, page and passage the claim is
    /// about.
    Attribution,
    /// The passage entails the extracted claim.
    Support,
    /// No material qualifier needed for the claim is omitted.
    Completeness,
    /// The evidence points to a place a person can find.
    Localisation,
    /// A date or amount transformed from the source is transformed
    /// correctly.
    Derivation,
}

/// One recorded answer to one dimension's question for one scored item.
/// A dimension nobody declared is absent, never silently passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionOutcome {
    Pass,
    Fail { reason: String },
}

/// Why a pack believes it can truthfully score a dimension — the same
/// posture as `eval_costs`: a declaration carries its reason and date,
/// so the manifest records an argument, not a flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceDeclaration {
    pub reason: String,
    pub date: chrono::NaiveDate,
}

/// Human-authored evidence ground truth for one expectation. The
/// positive case needs no authoring here — the expectation's own
/// `expect` tuple *is* the supported claim — so this block carries the
/// diagnoses: claims a keen extractor would wrongly read from this
/// passage, each with the reason it is unsupported.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEvidence {
    #[serde(default)]
    pub unsupported: Vec<CounterClaim>,
    /// Words the claim's evidence must keep for the claim to mean what
    /// it says. A true figure quoted without its qualifier is not a
    /// wrong answer; it is misleading evidence, and that distinction is
    /// exactly what completeness records.
    #[serde(default)]
    pub qualifiers: Vec<Qualifier>,
    /// The arithmetic behind this item's derived date, when it has one.
    #[serde(default)]
    pub derivation: Option<DerivationSpec>,
}

/// One authored material qualifier: the words, why they are material,
/// and which claims they qualify (all of this item's claims when no
/// selector is given).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualifier {
    pub text: String,
    pub why: String,
    #[serde(default)]
    pub applies_to: Option<ClaimSelector>,
}

/// One authored derivation: the source value and the closed, named
/// operation that turns it into the claim's derived value. Executed in
/// Rust and compared — never by re-running the pipeline's own resolver,
/// because a check derived with the run's resolver would agree with it
/// by construction.
// No deny_unknown_fields here: serde cannot combine it with
// #[serde(flatten)], which the op tag needs. The op enum itself still
// refuses an operation the harness cannot execute.
#[derive(Debug, Clone, Deserialize)]
pub struct DerivationSpec {
    pub from: chrono::NaiveDate,
    #[serde(flatten)]
    pub op: DerivationOp,
}

/// The closed vocabulary of operations a derivation may name. Small on
/// purpose: an operation the harness cannot execute deterministically
/// has no place in ground truth.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DerivationOp {
    AddDays { days: u32 },
    EndOfMonth,
    Identity,
}

impl DerivationSpec {
    fn derived(&self) -> Option<chrono::NaiveDate> {
        match self.op {
            DerivationOp::AddDays { days } => self
                .from
                .checked_add_days(chrono::Days::new(u64::from(days))),
            DerivationOp::EndOfMonth => end_of_month(self.from),
            DerivationOp::Identity => Some(self.from),
        }
    }
}

fn end_of_month(date: chrono::NaiveDate) -> Option<chrono::NaiveDate> {
    use chrono::Datelike;
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(year, month, 1)?.pred_opt()
}

/// One authored near miss: a claim this passage does not support, and
/// why. The why is the sentence a diagnosis shows a person.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterClaim {
    pub claim: ClaimSelector,
    pub why: String,
}

/// Which asserted claims a counter-claim speaks about. Absent fields
/// match anything, so an author can say "any payment claim" without
/// enumerating phrasings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimSelector {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub term: Option<String>,
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

impl ClaimSelector {
    fn matches(&self, obligation: &crate::run::Obligation) -> bool {
        self.kind
            .as_deref()
            .is_none_or(|kind| kind == obligation.kind)
    }

    fn matches_term(&self, term: &crate::terms::Term) -> bool {
        self.term.as_deref().is_none_or(|t| t == term.term)
            && self.basis.as_deref().is_none_or(|b| b == term.basis)
            && self.value.as_deref().is_none_or(|v| v == term.value)
    }
}

/// Score the declared dimensions for one asserted obligation. Only
/// dimensions the pack declared are answered; an undeclared dimension
/// must stay absent rather than passing vacuously.
pub(crate) fn obligation_evidence(
    declared: &BTreeMap<EvidenceDimension, EvidenceDeclaration>,
    want: &super::fixture::ObligationExpectation,
    found: &crate::run::Obligation,
    sources: &[String],
) -> BTreeMap<EvidenceDimension, DimensionOutcome> {
    let mut outcomes = BTreeMap::new();
    if declared.contains_key(&EvidenceDimension::Existence) {
        outcomes.insert(EvidenceDimension::Existence, existence(found, sources));
    }
    if declared.contains_key(&EvidenceDimension::Attribution) {
        outcomes.insert(EvidenceDimension::Attribution, attribution(want, found));
    }
    if declared.contains_key(&EvidenceDimension::Support) {
        outcomes.insert(EvidenceDimension::Support, support(want, found));
    }
    if let Some(derivation) = want
        .evidence
        .as_ref()
        .and_then(|evidence| evidence.derivation.as_ref())
    {
        if declared.contains_key(&EvidenceDimension::Derivation) {
            outcomes.insert(
                EvidenceDimension::Derivation,
                derivation_outcome(derivation, found.due.map(|due| due.date)),
            );
        }
    }
    outcomes
}

/// The claim's derived date against the authored operation's result.
/// Two independent authorings meeting: the bed writes the source and
/// the operation, the pipeline resolves the phrase, and a disagreement
/// names both dates rather than quietly preferring either.
fn derivation_outcome(spec: &DerivationSpec, due: Option<chrono::NaiveDate>) -> DimensionOutcome {
    let Some(derived) = spec.derived() else {
        return DimensionOutcome::Fail {
            reason: format!(
                "the authored operation does not produce a date from {}",
                spec.from
            ),
        };
    };
    match due {
        Some(due) if due == derived => DimensionOutcome::Pass,
        Some(due) => DimensionOutcome::Fail {
            reason: format!(
                "the authored operation derives {derived} but the claim resolved {due}"
            ),
        },
        None => DimensionOutcome::Fail {
            reason: format!(
                "the authored operation derives {derived} but the claim resolved no date"
            ),
        },
    }
}

/// The claim's evidence passages are really in a source document,
/// verbatim modulo whitespace.
fn existence(found: &crate::run::Obligation, sources: &[String]) -> DimensionOutcome {
    for segment in &found.evidence {
        if !sources
            .iter()
            .any(|source| contains_squashed(source, &segment.text))
        {
            return DimensionOutcome::Fail {
                reason: format!(
                    "the quoted passage is not in any source document: {:?}",
                    segment.text
                ),
            };
        }
    }
    DimensionOutcome::Pass
}

/// The claim's evidence passages are the passage this decision is
/// about, not real words borrowed from elsewhere in the document.
fn attribution(
    want: &super::fixture::ObligationExpectation,
    found: &crate::run::Obligation,
) -> DimensionOutcome {
    for segment in &found.evidence {
        if !same_squashed(&segment.text, &want.segment) {
            return DimensionOutcome::Fail {
                reason: format!(
                    "the claim's evidence is a different passage: {:?}",
                    segment.text
                ),
            };
        }
    }
    DimensionOutcome::Pass
}

/// Set membership against the authored ground truth: the expectation's
/// own tuple is the supported claim, an `unsupported` counter-claim is
/// a diagnosed near miss, and anything else the bed's author never
/// wrote is unsupported by construction.
fn support(
    want: &super::fixture::ObligationExpectation,
    found: &crate::run::Obligation,
) -> DimensionOutcome {
    if let Some(expect) = &want.expect {
        let asserted = super::ExpectedObligation {
            kind: found.kind.clone(),
            party: found.party.clone(),
            deadline: found.deadline.clone(),
            anchor: found.anchor.clone(),
            due: found.due.map(|d| d.date),
        };
        if asserted == *expect {
            return DimensionOutcome::Pass;
        }
    }
    if let Some(counter) = want
        .evidence
        .as_ref()
        .into_iter()
        .flat_map(|evidence| &evidence.unsupported)
        .find(|counter| counter.claim.matches(found))
    {
        return DimensionOutcome::Fail {
            reason: counter.why.clone(),
        };
    }
    DimensionOutcome::Fail {
        reason: "the passage was not authored as supporting this claim".to_owned(),
    }
}

/// Score the declared dimensions for one asserted policy term. The
/// same questions as an obligation's, asked in the comparison
/// typology's vocabulary — most notably support, because a schedule
/// passage routinely states two values and the quote contains both.
pub(crate) fn term_evidence(
    declared: &BTreeMap<EvidenceDimension, EvidenceDeclaration>,
    want: &super::fixture::TermExpectation,
    found: &crate::terms::Term,
    sources: &[String],
) -> BTreeMap<EvidenceDimension, DimensionOutcome> {
    let mut outcomes = BTreeMap::new();
    if declared.contains_key(&EvidenceDimension::Existence) {
        let outcome = if sources
            .iter()
            .any(|source| contains_squashed(source, &found.quote))
        {
            DimensionOutcome::Pass
        } else {
            DimensionOutcome::Fail {
                reason: format!(
                    "the quoted words are not in any source document: {:?}",
                    found.quote
                ),
            }
        };
        outcomes.insert(EvidenceDimension::Existence, outcome);
    }
    if declared.contains_key(&EvidenceDimension::Attribution) {
        // The join that produced this claim already required the right
        // document index and a quote inside this decision's passage;
        // both facts are re-stated here as a recorded outcome so a
        // reader of the item need not know the join to trust them.
        let outcome = if contains_squashed(&want.segment, &found.quote) {
            DimensionOutcome::Pass
        } else {
            DimensionOutcome::Fail {
                reason: format!(
                    "the quote is not from the passage this claim is about: {:?}",
                    found.quote
                ),
            }
        };
        outcomes.insert(EvidenceDimension::Attribution, outcome);
    }
    if declared.contains_key(&EvidenceDimension::Support) {
        outcomes.insert(EvidenceDimension::Support, term_support(want, found));
    }
    if declared.contains_key(&EvidenceDimension::Completeness) {
        outcomes.insert(
            EvidenceDimension::Completeness,
            term_completeness(want, found),
        );
    }
    outcomes
}

/// Every authored material qualifier that applies to this claim must
/// survive into its quote. The claim itself may be right; evidence
/// that stops before the qualifying words misleads the person checking
/// it, which is a failure of the evidence, not of the answer.
fn term_completeness(
    want: &super::fixture::TermExpectation,
    found: &crate::terms::Term,
) -> DimensionOutcome {
    let missing = want
        .evidence
        .as_ref()
        .into_iter()
        .flat_map(|evidence| &evidence.qualifiers)
        .filter(|qualifier| {
            qualifier
                .applies_to
                .as_ref()
                .is_none_or(|selector| selector.matches_term(found))
        })
        .find(|qualifier| !contains_squashed(&found.quote, &qualifier.text));
    match missing {
        Some(qualifier) => DimensionOutcome::Fail {
            reason: format!("the quote omits {:?}: {}", qualifier.text, qualifier.why),
        },
        None => DimensionOutcome::Pass,
    }
}

fn term_support(
    want: &super::fixture::TermExpectation,
    found: &crate::terms::Term,
) -> DimensionOutcome {
    if let Some(expect) = &want.expect {
        if found.term == expect.term && found.basis == expect.basis && found.value == expect.value {
            return DimensionOutcome::Pass;
        }
    }
    if let Some(counter) = want
        .evidence
        .as_ref()
        .into_iter()
        .flat_map(|evidence| &evidence.unsupported)
        .find(|counter| counter.claim.matches_term(found))
    {
        return DimensionOutcome::Fail {
            reason: counter.why.clone(),
        };
    }
    DimensionOutcome::Fail {
        reason: "the passage was not authored as supporting this claim".to_owned(),
    }
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains_squashed(haystack: &str, needle: &str) -> bool {
    squash(haystack).contains(&squash(needle))
}

fn same_squashed(left: &str, right: &str) -> bool {
    squash(left) == squash(right)
}
