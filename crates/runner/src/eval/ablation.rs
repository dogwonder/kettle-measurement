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

use crate::claim_trace::{
    CheckOutcome, ClaimKind, ClaimTrace, ClaimTraceDocument, Guardrail, TerminalDisposition,
};
use crate::eval::fixture::scored_assertion_is_wrong;
use crate::eval::{
    ClassificationOutcome, ExpectedObligation, Extracted, ExtractionOutcome, ScoredDecision,
    ScoredItem,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    /// Whether this policy runs the model at all.
    ///
    /// The deterministic floor is not a pipeline with every guardrail
    /// removed — it is a system that asks the model nothing, and the
    /// difference is the whole comparison. Expressed the same way as
    /// the boundaries, as something a policy *has*.
    pub model: bool,
    pub active: BTreeSet<Guardrail>,
}

impl Policy {
    pub fn named(name: &str) -> Policy {
        Policy {
            name: name.to_owned(),
            model: true,
            active: BTreeSet::new(),
        }
    }

    /// The rungs to score one recording on: the floor, a model with no
    /// boundaries, then each boundary the recording exercised, added
    /// one at a time in the order the pipeline applies them.
    ///
    /// Built from what was observed rather than from a fixed list.
    /// A fixed ladder invents rungs: a pack that never runs a check
    /// gets a policy indistinguishable from the one below it, and two
    /// identical rows read as evidence that the boundary between them
    /// costs nothing — a claim nobody measured. Cumulative because the
    /// question is which guard supplies the reliability, and that is
    /// only readable as the difference between consecutive rows.
    ///
    /// `BTreeSet` iterates [`Guardrail`] in declaration order, which is
    /// pipeline order; the enum is the single place that order lives.
    pub fn ladder(observed: &BTreeSet<Guardrail>) -> Vec<Policy> {
        let mut rungs = vec![Policy::floor(), Policy::named("no-boundaries")];
        let mut rung = Policy::named("");
        for guardrail in observed {
            rung = rung.with(*guardrail);
            rung.name = rung
                .active
                .iter()
                .map(guardrail_label)
                .collect::<Vec<&str>>()
                .join("+");
            rungs.push(rung.clone());
        }
        rungs
    }

    /// The deterministic no-model floor: the row every model policy is
    /// read against.
    ///
    /// Its harm columns are empty by construction, which is exactly
    /// what a perfect system's would look like. [`PolicyRow::answered`]
    /// is what tells the two apart.
    pub fn floor() -> Policy {
        Policy {
            name: "deterministic-floor".to_owned(),
            model: false,
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

/// A boundary's name in a policy name, in the vocabulary the trace uses
/// rather than a display string a template could drift from.
fn guardrail_label(guardrail: &Guardrail) -> &'static str {
    match guardrail {
        Guardrail::Schema => "schema",
        Guardrail::Pairing => "pairing",
        Guardrail::Quote => "quote",
        Guardrail::QuoteIdentifiesPassage => "quote-identifies-passage",
        Guardrail::ValueShape => "value-shape",
        Guardrail::PackCoverage => "pack-coverage",
        Guardrail::DeterministicDerivation => "deterministic-derivation",
        Guardrail::ReviewRouting => "review-routing",
        Guardrail::Report => "report",
        Guardrail::Action => "action",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Every claim this policy put in front of a person as an
    /// assertion, right or wrong.
    ///
    /// Without it a harm-only row ranks the deterministic floor first
    /// on every scorecard, because a system that answers nothing gets
    /// nothing wrong. Safety and usefulness are separate columns for
    /// that reason and are never collapsed into one score whose
    /// weights hide the decision (#432).
    pub answered: Vec<String>,
    /// Claims this policy asserted **and** got right: the end-to-end
    /// task done for the person, without them.
    ///
    /// `answered` alone only half-solves the floor problem, because it
    /// counts assertions without asking whether they were any good. A
    /// row needs both: what reached the person, and how much of it was
    /// right. Together with the harm columns they make a Pareto
    /// frontier readable rather than a single weighted score whose
    /// weights hide the decision (#432).
    pub delivered: Vec<String>,
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
                // A policy with no model asked nothing and was told
                // nothing, so a model claim is neither asserted nor
                // caught by it. Treating the floor as a pipeline that
                // stopped everything would credit it with containment
                // it never performed.
                if !policy.model {
                    continue;
                }
                // Every column shares one denominator. A claim the
                // recording cannot judge belongs in none of them: the
                // letter run records 4,035 claims and can judge 510,
                // and a row counting all 4,035 as answered beside 12
                // escaped errors invites a rate whose numerator and
                // denominator are about different populations.
                let Some(verdict) = verdicts.get(&trace.id).copied() else {
                    continue;
                };
                let stopped = stopped_by(trace, policy);
                if !stopped {
                    row.answered.push(trace.id.clone());
                }
                match (verdict, stopped) {
                    (CandidateVerdict::Wrong, true) => row.prevented.push(trace.id.clone()),
                    (CandidateVerdict::Wrong, false) => row.escaped.push(trace.id.clone()),
                    (CandidateVerdict::Unknown, true) => row.unknown.push(trace.id.clone()),
                    // A candidate the recording cannot judge and no
                    // boundary stopped was asserted, and asserting
                    // something unjudgeable is not the same finding as
                    // asserting something wrong. It belongs in a
                    // burden column rather than a harm one, and
                    // guessing which would be the collapse #432
                    // forbids — so it is carried nowhere until there
                    // is a column that means it.
                    (CandidateVerdict::Unknown, false) => {}
                    // Right and asserted: the pipeline working, and
                    // the only cell in this table that is a *good*
                    // outcome rather than an absence of a bad one.
                    (CandidateVerdict::Correct, false) => row.delivered.push(trace.id.clone()),
                    // Right and stopped. Not harm, and not delivery
                    // either — the answer was correct and the person
                    // still had to do the work, which is review burden
                    // and belongs in neither column here.
                    (CandidateVerdict::Correct, true) => {}
                }
            }
            row
        })
        .collect()
}

/// Whether a boundary this policy applies actually stopped the
/// candidate.
///
/// `Failed` only, among checks. `Warned` and `ChangedValue` are
/// deliberately not containment: a warning let the candidate through
/// (#460 rule 2), and a changed value is the pipeline's own act on a
/// candidate that was never at fault (#470). Counting either as a catch
/// would credit a policy with reliability it did not supply, which is
/// the one thing this scorecard exists to measure.
///
/// A claim that ended in review is contained whether or not a check
/// failed, which is the line `containment_metrics` already draws:
/// honest low confidence reaches a person without failing anything
/// semantic, and review routing is itself the boundary. Read through
/// failed checks alone, the letter run would credit routing with
/// stopping one claim when it stopped sixty-nine.
fn stopped_by(trace: &ClaimTrace, policy: &Policy) -> bool {
    let failed = trace.checks.iter().any(|check| {
        policy.active.contains(&check.guardrail) && check.outcome == CheckOutcome::Failed
    });
    let routed = policy.active.contains(&Guardrail::ReviewRouting)
        && trace.terminal == TerminalDisposition::NeedsReview;
    failed || routed
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
///
/// Read through [`crate::timeline::deadline_signature`] rather than
/// verbatim (#554): two signatures are equal exactly when the resolver,
/// given one letter date, would give the two the same
/// [`super::ObligationIdentity`]. Compared verbatim this was a fourth
/// definition of the same obligation — "within 45 days of 23 August
/// 2026" against the bed's split "within 45 days" + anchor was booked
/// as a wrong candidate the guard prevented, crediting the guard for
/// stopping a faithful reading.
fn disagrees_only_in_derived_fields(expected: &Extracted, proposed: &Extracted) -> bool {
    match (expected, proposed) {
        (Extracted::Obligation(want), Extracted::Obligation(got)) => {
            want.kind == got.kind
                && want.party.eq_ignore_ascii_case(&got.party)
                && crate::timeline::deadline_signature(&want.deadline, &want.anchor)
                    == crate::timeline::deadline_signature(&got.deadline, &got.anchor)
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

/// Both halves of one recording, joined into a verdict per claim id
/// (#432).
///
/// A recording does not speak about its candidates in one voice, and
/// the split is the reason this function exists rather than a lookup:
///
///   * a claim that **became an assertion** was judged when the run was
///     scored, so the scorer's own predicate is the answer. Reusing it
///     is the discipline, not a convenience — a plausible parallel
///     correctness test written on 19 August called 2,629 of 3,028 real
///     items wrong, because `outcome: absent` is the *correct* answer
///     for a no-obligation item;
///   * a claim a guard **stopped** was never scored at all. Its
///     decision carries a reason and no value, so its verdict is read
///     back off the trace's candidate by [`verdict_for`].
///
/// The dividing line is whether the scored decision asserts a value,
/// not whether the run called it a failure. A rejected claim leaves an
/// `Absent` decision that the eval rightly counts as a miss — but a
/// miss is the pipeline's outcome, not a statement about the value the
/// model proposed, and booking it as one would call every contained
/// candidate wrong and credit its guard with catching it.
///
/// Only the claims some item judged as a **value** appear in the map. A
/// trace nothing was scored from has no oracle at all, and inventing an
/// entry for it would put a claim in the scorecard's columns on no
/// evidence.
pub fn verdicts(items: &[ScoredItem], traces: &[ClaimTrace]) -> BTreeMap<String, CandidateVerdict> {
    let by_id: BTreeMap<&str, &ClaimTrace> = traces
        .iter()
        .map(|trace| (trace.id.as_str(), trace))
        .collect();

    // Collected as a set per claim, because two items may be scored
    // from one candidate. Agreeing is the ordinary case and settles
    // it; disagreeing means the recording holds two answers about one
    // value, which is unjudgeable rather than a majority.
    let mut proposed: BTreeMap<String, BTreeSet<CandidateVerdict>> = BTreeMap::new();
    for item in items {
        let judged: Vec<&ClaimTrace> = item
            .trace_ids
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .filter(|trace| answers(trace.kind, &item.decision))
            .collect();
        // One scored decision over several candidate values judged the
        // item, and which of them it judged is generally not in the
        // recording. Attributing it to one is a guess in the direction
        // that flatters whichever guard stopped the other.
        //
        // Unless they propose the *same* assertion, and then there is
        // nothing to attribute: two candidates carrying one value reach
        // one verdict whichever the decision meant, so judging them
        // together decides nothing the recording does not already say.
        // This is not #457's anti-pattern, which is re-deriving a
        // claim's source from its text — it declines to pick, and
        // observes that picking cannot matter.
        //
        // It is the renewal pack's entire unjudgeable rate. A renewal
        // reads two documents, so the same passage is read twice and
        // proposed twice: on the 25 August v16 recording **222 of 222**
        // ambiguous items link byte-identical candidates and none links
        // candidates that differ. The letter bed produces neither case
        // — see [`answers`].
        let verdict = match judged.as_slice() {
            [trace] => item_verdict(item, trace),
            [first, rest @ ..] if rest.iter().all(|other| other.candidate == first.candidate) => {
                item_verdict(item, first)
            }
            _ => CandidateVerdict::Unknown,
        };
        for trace in judged {
            proposed
                .entry(trace.id.clone())
                .or_default()
                .insert(verdict);
        }
    }

    proposed
        .into_iter()
        .map(|(id, seen)| {
            let settled = match seen.len() {
                1 => seen.into_iter().next().unwrap_or(CandidateVerdict::Unknown),
                _ => CandidateVerdict::Unknown,
            };
            (id, settled)
        })
        .collect()
}

/// Whether this claim is the one a scored decision judged.
///
/// An item links the whole chain that produced it, and most of that
/// chain is not a candidate value. Measured on the run behind
/// `evals/baseline-v14-letter.json`: of 3,028 items, 2,148 link a lone
/// `decision`, 510 link a `decision` and the `obligation` nested inside
/// it, and 370 link two `decision` traces — and **none links two
/// value-bearing claims**. So a decision and its own obligation are a
/// passage-level answer and the value read out of it, never two
/// candidates in competition, and refusing to judge the pair would
/// throw away every extraction the letter bed scores.
///
/// Matched by question rather than by proximity, which is #271's rule:
/// a merchant's item links its normalisation trace as well as its
/// classification, and the two answer different questions. A join that
/// took whichever was nearest would calibrate the wrong one.
fn answers(kind: ClaimKind, decision: &ScoredDecision) -> bool {
    match decision {
        ScoredDecision::Extraction { .. } => {
            matches!(kind, ClaimKind::Obligation | ClaimKind::PolicyTerm)
        }
        ScoredDecision::Classification { .. } => matches!(kind, ClaimKind::Classification),
    }
}

/// The verdict one scored item carries about the one candidate it was
/// read from.
fn item_verdict(item: &ScoredItem, trace: &ClaimTrace) -> CandidateVerdict {
    if asserts_a_value(&item.decision) {
        if scored_assertion_is_wrong(&item.decision) {
            return CandidateVerdict::Wrong;
        }
        // `Correct` rather than "not wrong": the two are read
        // differently by [`scorecard`], where a correct candidate a
        // guard stopped is review spent needlessly and an unknown one
        // is a bound the reader has to see.
        return CandidateVerdict::Correct;
    }
    verdict_for(trace, expectation(&item.decision))
}

/// Whether this decision records a value the run actually asserted.
///
/// Review-routed and absent decisions do not: the first was surfaced to
/// a person and the second asserts nothing, and in both the value the
/// candidate proposed lives only in the trace.
fn asserts_a_value(decision: &ScoredDecision) -> bool {
    match decision {
        ScoredDecision::Classification { actual, .. } => {
            matches!(actual, ClassificationOutcome::Classified { .. })
        }
        ScoredDecision::Extraction { actual, .. } => {
            matches!(actual, ExtractionOutcome::Found { .. })
        }
    }
}

/// What the bed authored for this decision, in the shape
/// [`verdict_for`] compares against.
///
/// `None` for a classification: a class is not an [`Extracted`] value
/// and the candidate-level comparison has no shape for one, so a
/// stopped classification is honestly unjudgeable here rather than
/// squeezed into the wrong oracle.
fn expectation(decision: &ScoredDecision) -> Option<&Extracted> {
    match decision {
        ScoredDecision::Extraction { expected, .. } => expected.as_ref(),
        ScoredDecision::Classification { .. } => None,
    }
}

/// One fixture's recording: what its run wrote, and what the eval
/// scored from it.
#[derive(Debug, Clone)]
pub struct Recording {
    /// The fixture's file name, as the eval report records it.
    pub fixture: String,
    pub traces: Vec<ClaimTrace>,
    pub items: Vec<ScoredItem>,
}

/// Authored expectations the run asserted nothing from (#432, #474).
///
/// The other half of the harm, and the half no policy column can carry.
/// A miss produces no claim: the model asserted nothing, so there is no
/// candidate for a boundary to inspect and nothing for any guardrail to
/// stop. Containment operates on assertions, and a miss is the absence
/// of one. It is therefore **constant across every policy**, which is
/// exactly why it is returned separately rather than added to
/// [`PolicyRow`] — a column reading the same number in every row would
/// suggest the policies had been compared on it when they cannot be.
///
/// Reporting it is not optional, though. On 25 August 2026 the letter
/// pack's `escaped` column read 0 under every policy while the eval
/// reported seven wrong answers on the sealed set, because every one of
/// them was a miss (#552's `points-at-a-table` construction). A
/// scorecard showing only the claim-derived columns would have called
/// that pipeline clean.
///
/// Three things are deliberately not misses:
///
/// * a passage that asks nothing, answered with nothing — the correct
///   answer, and the one a keen extractor gets wrong;
/// * a decision routed to a person, which reached somebody and is the
///   honest floor working rather than failing (`expected_review` marks
///   the cases where that *is* the win, #445);
/// * an unauthored negative, which exists to calibrate confidence and
///   is excluded from every metric a gate reads.
pub fn misses(items: &[ScoredItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match &item.decision {
            ScoredDecision::Extraction {
                expected: Some(_),
                expected_review: false,
                unauthored_negative: false,
                actual: ExtractionOutcome::Absent,
            } => Some(item.item_id.clone()),
            _ => None,
        })
        .collect()
}

/// Several recordings, made poolable.
#[derive(Debug, Clone, Default)]
pub struct Pooled {
    pub traces: Vec<ClaimTrace>,
    pub verdicts: BTreeMap<String, CandidateVerdict>,
}

/// Pool recordings into one trace list and one verdict map (#432).
///
/// A claim id is a per-run counter — `claim-000042` is simply the
/// forty-second thing that run recorded — so two fixtures collide on
/// every id they both reach. Pooled on the bare id, one fixture's
/// candidate would be scored against another's expectation, which is
/// the "scored against different things" failure the claim-id column
/// exists to make impossible. So a pooled id names its recording.
///
/// Qualifying here rather than in [`verdicts`] is deliberate: within
/// one recording the bare id is the right key and is what the file on
/// disk says. The fixture is context the pooler has and the recording
/// does not.
pub fn pool(recordings: &[Recording]) -> Pooled {
    let mut pooled = Pooled::default();
    for recording in recordings {
        let verdicts = verdicts(&recording.items, &recording.traces);
        for trace in &recording.traces {
            let mut qualified = trace.clone();
            qualified.id = pooled_id(&recording.fixture, &trace.id);
            qualified.parent_id = trace
                .parent_id
                .as_ref()
                .map(|parent| pooled_id(&recording.fixture, parent));
            pooled.traces.push(qualified);
        }
        for (id, verdict) in verdicts {
            pooled
                .verdicts
                .insert(pooled_id(&recording.fixture, &id), verdict);
        }
    }
    pooled
}

/// A claim id that survives being pooled with another run's.
///
/// `#` because a fixture name carries dots and dashes already, and a
/// separator that could occur in either half would make the id
/// ambiguous in exactly the way the qualification is fixing.
fn pooled_id(fixture: &str, claim: &str) -> String {
    format!("{fixture}#{claim}")
}

/// What a walk over a runs directory found, and what it did not.
#[derive(Debug, Clone, Default)]
pub struct Walk {
    pub recordings: Vec<Recording>,
    /// Fixtures the report names whose recording is not on disk.
    ///
    /// Named rather than counted, and never silently skipped: a
    /// scorecard assembled from half a run has smaller harm columns
    /// than the run it claims to describe, which reads as a better
    /// policy.
    pub missing: Vec<String>,
}

/// Read one eval run's recordings off disk (#432).
///
/// A run directory is self-describing: `claims.json` is what the
/// pipeline recorded, `eval-items.json` is what the harness scored from
/// it, and the two join on trace ids inside that one directory. So the
/// walk needs no third file and no report to pair them.
///
/// Which directory belongs to which fixture is **constructed** through
/// [`crate::run_dir::eval_run_id`], the same function the harness names
/// it with. Parsing the name back is the alternative, and re-deriving a
/// claim's source from its own text is the defect this project has now
/// paid for twice (#361, #457).
pub fn walk(runs_dir: &Path, pack: &str, model_file: Option<&str>, fixtures: &[String]) -> Walk {
    let mut walk = Walk::default();
    for fixture in fixtures {
        let dir = runs_dir.join(crate::run_dir::eval_run_id(pack, model_file, fixture));
        match read_recording(&dir, fixture) {
            Some(recording) => walk.recordings.push(recording),
            None => walk.missing.push(fixture.clone()),
        }
    }
    walk
}

/// One run directory, or nothing.
///
/// Both halves are required. A directory with traces and no scored
/// items has no oracle, and one with items and no traces has no
/// candidates — either way there is no recording here to score, and
/// treating a half as a whole would quietly shrink a harm column.
fn read_recording(dir: &Path, fixture: &str) -> Option<Recording> {
    let claims = std::fs::read_to_string(dir.join("claims.json")).ok()?;
    let document: ClaimTraceDocument = serde_json::from_str(&claims).ok()?;
    let items = std::fs::read_to_string(dir.join("eval-items.json")).ok()?;
    let items: Vec<ScoredItem> = serde_json::from_str(&items).ok()?;
    Some(Recording {
        fixture: fixture.to_owned(),
        traces: document.claims,
        items,
    })
}

// ## Where this was left, 20 August 2026 — resume here (#432)
//
// The scorecard exists and has been read against both committed v14
// baselines through `kettle ablate`. What it says, 20 August 2026:
//
//   letters   510 judgeable claims of 4,035 — 12 escaped, under every
//             policy from a bare model to the full pipeline; 1 stopped,
//             unjudgeable.
//   renewal   363 of 1,132 — 8 escaped, again identical on every rung;
//             7 stopped by pack coverage, all unjudgeable.
//
// So on this evidence **no guardrail demonstrably prevented a single
// wrong claim on either bed**. The quote rules ran 356 times on renewal
// and failed none. That is a one-sided bound and must be read as one:
// `prevented` counts demonstrated catches, and a stopped claim whose
// candidate cannot be judged is `unknown` rather than a catch. But the
// unknown column is 1 and 7, so there is little room in it.
//
// Next, in order: the end-to-end task column (see below), then the
// remaining rungs of #432's list that a recording cannot supply —
// cascades and a second model — which need runs, not re-readings.
//
// Three things known and unfinished:
//
//   * `proposed_assertion` handles `ClaimKind::Obligation` and returns
//     `None` for every other kind, so the renewal pack's `policy-term`
//     claims would come out entirely `Unknown` — honest, and useless
//     for renewal. It wants its own cycle once the letter path
//     produces real rows.
//   * 2,518 of the letter bed's 3,028 items link no value-bearing
//     claim at all — the passages the run read and asserted nothing
//     from. `delivered` counts the task done for the person, which is
//     the end-to-end column at the granularity a recording supports;
//     what is still missing is the other half, the authored
//     expectation nothing proposed at all. A miss is a real cost no
//     guardrail can change, so it is constant across policies and
//     belongs beside the table rather than in it.
//   * An item linking several value-bearing claims settles none of
//     them, and renewal does this constantly: 220 of 356 asserted
//     claims carry no verdict, because a passage stating several terms
//     links them all to one scored decision. Letters never do it. The
//     CLI prints the remainder rather than letting the columns
//     silently fail to add up, but the renewal scorecard is 62%
//     unjudgeable and that is the next thing to fix if renewal is to
//     be read at all.
//   * An integration test over real data has nowhere to read a trace
//     from: baselines are committed, `claims.json` files are not
//     (`evals/runs/` is gitignored and the archive is a separate
//     repository). A committed sample trace is a prerequisite for
//     testing this against anything real, and no test may depend on a
//     gitignored path.
