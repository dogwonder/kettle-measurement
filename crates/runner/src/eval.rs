//! The eval harness's documents and its one piece of judgement (#37):
//! the shapes an eval writes to disk, and the rules that turn a set of
//! scores into Pass, Marginal or Fail (brief §6).
//!
//! Types only. The harness that produces them is #25, the scoring
//! functions that fill them in are #36, and `cli eval` is #38.
//!
//! An [`EvalReport`] is a claim about a model made on a particular
//! machine, so the machine travels with it: "0.96 end-to-end" means
//! nothing without "on an M1 Air with 8GB". The JSON also commits to a
//! per-pack `tiers.json` that ships with the app, which the
//! model-manager screen reads — so field names here are a contract with
//! the shell, as in [`crate::results`].
//!
//! Scores and rates are fractions, not money: `f32` is right here. The
//! ban on floats in `CLAUDE.md` is about amounts, and no amount appears
//! in this file.

pub mod ablation;
pub mod bed;
pub mod evidence;
pub mod fixture;
pub mod letters;
pub mod mutation;
pub mod oracle;
pub mod relations;
pub mod renewals;
pub mod replay;
pub mod resume;

/// Re-exported so a selection can be named without reaching into
/// `fixture` — it is a property of a measurement, not of a file.
pub use fixture::EvalSet;

use crate::kinds::KindFrom;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

/// The end-to-end score a run must reach to pass, whatever the pack's
/// per-step bars say (brief §6). Fixed across packs deliberately: it is
/// the promise Kettle makes about a finished report, not a per-task
/// dial.
pub const END_TO_END_BAR: f32 = 0.95;

/// A retired pack key. Older manifests may still contain it, so the
/// runner ignores it rather than mistaking it for a scored model step.
/// Review rate remains reported as a cost but never gates a verdict.
pub const MAX_REVIEW_RATE_KEY: &str = "max_review_rate";

/// What a score or verdict *means* — bumped by hand when scoring changes:
/// a different string metric, tolerance, denominator or verdict rule
/// (#84, #237).
///
/// Numbers recorded under a different version are refused rather than
/// compared, because a comparison across a bump would report
/// real-looking movement that is entirely an artefact of the harness
/// changing underneath. That has already nearly happened once:
/// `similarity` moved from Jaro-Winkler to normalised
/// Damerau-Levenshtein during #36, because the first scored *British
/// Gas* against *British Airways* at 0.92, and every stored `normalise`
/// score would silently have become incomparable.
///
/// The crate version is the wrong pin — it moves for reasons that have
/// nothing to do with scoring. A hash of the scoring functions'
/// behaviour is the right one and nobody maintains it honestly. A
/// hand-bumped integer is the honest minimum.
///
/// It lives here rather than with the baseline that made it necessary
/// because it is a fact about scoring, and both the baseline (#84) and
/// every `tiers.json` entry (#39) stamp themselves with it.
/// Version 5 (#253): kind is derived from cadence in Rust and the item
/// records score the joint system — derived kind plus model category —
/// and a low-confidence classification is recorded as surfaced for a
/// person, never as an assertion.
/// Version 6 (#279): a second metric exists. Extraction runs emit
/// per-item records under their own miss/invent harm lens, so a pack
/// can hold missing an obligation to a tighter ceiling than inventing
/// one. Classification's numbers are unchanged — the six-cell counting
/// is now shared by both metrics rather than reimplemented — but the
/// set of things a report can say has grown, and a baseline that
/// predates the second metric cannot be compared with one that has it.
/// Version 7 (#287): an obligation is joined on the date its deadline
/// *resolves to*, not on the anchor's wording. The anchor reaches no
/// person and the resolver reads only a date inside it, so two dateless
/// anchors were always the same input — charging a run for preferring
/// one wording measured the bed's ambiguity rather than the model's
/// reading. `obligations` therefore means something different, and
/// strictly more forgiving, than it did at version 6: an `expected.json`
/// now authors the due date, and a bed regenerated before this version
/// cannot be compared with one after it.
/// Version 8 (#301): a verdict is no longer computed one way for every
/// pack. A pack declares a [`Gate`], and `pooled` reads a step's rate
/// across every decision by its Wilson lower bound rather than gating
/// on the worst fixture. The scores are untouched; what a *verdict*
/// means is not, which is exactly what this constant guards.
/// Version 9 (#310): a declared ceiling is judged over **distinct
/// decisions** rather than rows. The same merchant, or the same
/// passage, asked again is the same question — at temperature 0 it
/// gets the same answer, and 87% of repeated merchants and 100% of
/// repeated passages did. Counting each row as an independent trial
/// narrowed every interval a ceiling is read against, so gates could
/// clear on repetition rather than on evidence: nine of nineteen
/// subscription gates change verdict under this, all of them from PASS
/// to FAIL. Rows are still reported, as exposure.
/// Version 10 (#442, #443): the harm lens sees every assertion. A
/// found-but-wrong obligation — the right passage, the wrong deadline —
/// now counts confident-wrong in the class that expected it, and an
/// assertion on a passage the bed never authored becomes a scored
/// invention tagged into every gated stratum. Both were invisible:
/// the first full mutation run over the letter bed (#426) showed all
/// 340 one-digit deadline mutants and 1,750 unauthored inventions
/// clearing every gate. Scores move wherever a model asserted wrongly
/// or beyond the bed, so no baseline from version 9 may be compared.
/// Version 11 (#427, #445): relations and referrals enter the verdict.
/// A declared relation that fails now fails the report — invariance
/// and inversion are claims about meaning no per-item gate can see —
/// and an expectation may declare that its correct outcome is a
/// referral, so the unmodelled_term shape finally rewards the
/// surfacing it was authored to test instead of capping a perfect
/// pooled run at ~0.984. Step scores, end-to-end and the verdict all
/// move for packs using either; version 10 baselines are refused.
/// Version 12 (#452): a confident-wrong assertion is one that says
/// something different about what the person acts on, not one whose
/// struct differs. Comparing whole structs counted two fields no
/// pooled score joins on and no report renders — an obligation's
/// `anchor` and a term's `quote` — so the two lenses disagreed about
/// the same items: the first v11 letter run printed
/// `obligations (pooled) 1.00 (n=462)` beside confident-wrong 0.24,
/// on a run whose every `due` date was right, and 128 of 294 renewal
/// terms differed from the authored sentence by a trailing full stop.
/// An anchor is now compared by the date it names and a quote by
/// containment ([`Extracted::same_assertion_as`]); every field the
/// pooled scores join on stays exact. Harm cells and escape counts
/// move wherever a run was right about the answer and worded its
/// evidence differently, so version 11 baselines are refused.
/// Version 13 (#457): a comparison expectation is scored against the
/// term read from **its own** passage. The join asked which term's
/// quote the expectation's passage contained and took the first that
/// did — re-deriving a fact the run already holds, since the model is
/// asked one passage at a time. A quote is evidence that a value is on
/// the page; it was never an identifier of where. The renewal bed's
/// commercial schedules state the same term under three cover
/// sections, the model quoted the bare label (`Annual premium`,
/// `Excess`), and the label is verbatim in all three — so sections two
/// and three were both scored against section one's reading. The
/// 8 August v12 measurement failed the renewal miss ceiling at 0.05
/// (n=302) on 16 wrong assertions, and the run dirs show the model
/// read every value in both documents correctly. Nothing wrong ever
/// reached a person: `diff_terms` refuses a repeated `(term, basis)`
/// and every reading went to review (#377). Scores move wherever a
/// document states one term more than once, so version 12 baselines
/// are refused.
/// Version 14 (#460): a quote must contain the value it evidences.
/// `quote_is_in` — is this text on the page — was necessary and not
/// sufficient: the 8 August v12 renewal run quoted the bare label
/// `Excess` as evidence for three different numbers, and a quote that
/// supports three values supports none of them. A term whose quote
/// does not contain its value is now refused to review at the quote
/// guardrail, so review rates, harm cells and verdicts move wherever a
/// model quoted a label instead of a value. Rule 2 of the same
/// decision — the quote identifies its passage, verbatim in it and in
/// no other passage of that document — is a warning in the claim
/// trace, never a refusal, and moves no score by construction.
/// Version 13 baselines are refused.
/// Version 15 (#552): one definition of "the same obligation". Three
/// parts of the scorer asked whether wording is part of the claim and
/// gave three answers — the pooled join keyed a dated deadline on the
/// day it resolves to and ignored the anchor (#287), the harm lens took
/// the deadline verbatim and the anchor by its date (#452), and the
/// evidence `support` dimension took both verbatim. So one faithful
/// reading could be found, confident-wrong and unsupported at once:
/// the re-authored exam bed of 21 August found all 36 `payment_anchored`
/// obligations and demoted 12 to confident-wrong for copying the
/// letter's date into the deadline, and on the bed that passes
/// `support` failed 78 times per exam run and 79 per development run
/// on `anchor` alone, right date, different words — a `month-end`
/// stratum at 0 of 78 support reporting recall 1.00, because nothing
/// consumed the dimension. [`ObligationIdentity`] is now the one
/// definition and all three read it: the same ask, on the same party,
/// by the same day *arrived at the same way* — the deadline's shape
/// (counted, absolute, pointed, undated) travels with the day, so a
/// date the model computed for itself, or fused from a table row it
/// was not asked about, is still a different assertion from the one
/// the letter wrote. Harm cells, escape counts and support outcomes
/// move wherever a run resolved the right day in other words, so
/// version 14 baselines are refused.
pub const SCORING_VERSION: u32 = 15;

pub use crate::timeline::DeadlineShape;

/// The most two scores can differ by and still be the same score.
///
/// Round-trip noise only, never a tolerance for real drift: scores are
/// `f32` fractions of small integer counts (44 of 50) that make a round
/// trip through JSON text. One answer in a thousand-answer fixture
/// would move a score by 0.001, so this sits three orders of magnitude
/// below the smallest finding anyone could have.
pub const SCORE_NOISE: f32 = 1e-6;

// ---------------------------------------------------------------------------
// Thresholds

/// A pack's `eval` block, read as the bar each model step must clear.
///
/// Built from [`crate::packs::Manifest::eval`], e.g.
/// app.kttl.subscription-audit's `{ "normalise": 0.85 }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// How this pack's verdict is computed from its fixtures (#301).
    pub gate: Gate,
    /// Step name to minimum score, inclusive.
    pub steps: BTreeMap<String, f32>,
    /// Per-item classification ceilings. The report must clear every
    /// declared class in every declared stratum; no averaging path
    /// exists between them.
    pub classification_strata: BTreeMap<String, ClassificationStratum>,
}

/// How a report's verdict is computed from its fixtures (#301).
///
/// One rule cannot serve both shapes of pack, and the evidence is two
/// packs on this same harness behaving in opposite ways.
/// `subscription-audit` carries ~10 decisions per fixture, so a
/// per-fixture score is a rate and gating on it is informative — 35 of
/// its 80 fixtures fall below the bar, spread across the range.
/// `letter-to-actions` carries **one** decision in 245 of its 355
/// fixtures, so a per-fixture score can only be 0.0 or 1.0; a 0.95 bar
/// on those means no errors at all, anywhere, and the verdict reads the
/// same at 444/445 as at 0/445.
///
/// The second is not a strict gate but a gate with no gradient, and a
/// gate with no gradient cannot tell improvement from disaster. That is
/// the failure this enum exists to make impossible to declare by
/// accident, rather than merely to document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// Every fixture must clear each step's bar and [`END_TO_END_BAR`];
    /// the worst fixture wins. Right where a fixture carries enough
    /// decisions for its score to be a rate rather than a coin flip.
    PerFixture,
    /// The pooled rate across every decision, read against the bar by
    /// its Wilson lower bound. No single fixture gates. Right for a bed
    /// of mostly-single-decision fixtures.
    ///
    /// Pooled **per decision**, not per fixture: the harm Kettle can do
    /// is per obligation — one missed deadline is one missed deadline
    /// whether its letter carried one ask or three — and `eval_strata`
    /// already counts items, so this keeps the two gates reading the
    /// same denominator instead of quietly disagreeing. The cost of
    /// that choice is that a pack can move its own score by repacking
    /// decisions per fixture, which is why the per-stratum ceilings stay
    /// on beside it as the check on a bed reshaped into an easier one.
    Pooled,
}

/// How many scored decisions one fixture carries, across every role a
/// pack may ask for. Keyed by role like the expectation vocabulary
/// itself, so a new pack type adds a block here rather than a rule.
/// Counted on the same denominator the step score uses: an obligation
/// the letter does not carry is scored, and matters — it is what the
/// invention ceiling is measured on — but it is not part of the
/// `obligations` rate, so counting it here would report a granularity
/// the bar does not have.
fn decisions_in(fixture: &crate::eval::fixture::Fixture) -> usize {
    let expected = &fixture.expected;
    expected.normalise.len()
        + expected.classify.len()
        + expected.recurring.len()
        + expected
            .obligations
            .iter()
            .filter(|item| item.expect.is_some())
            .count()
        + expected
            .policy_terms
            .iter()
            .filter(|item| item.expect.is_some())
            .count()
}

/// The measured/unmeasured split of the evidence vocabulary (#430).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCoverage {
    pub measured: Vec<evidence::EvidenceDimension>,
    pub not_measured: Vec<evidence::EvidenceDimension>,
}

impl EvidenceCoverage {
    /// `None` when the pack declares no dimensions — an older pack's
    /// report carries no coverage block rather than an empty claim.
    pub fn from_declared(
        declared: &BTreeMap<evidence::EvidenceDimension, evidence::EvidenceDeclaration>,
    ) -> Option<Self> {
        use evidence::EvidenceDimension::*;
        if declared.is_empty() {
            return None;
        }
        let all = [
            Existence,
            Attribution,
            Support,
            Completeness,
            Localisation,
            Derivation,
        ];
        let (measured, not_measured) = all
            .into_iter()
            .partition(|dimension| declared.contains_key(dimension));
        Some(Self {
            measured,
            not_measured,
        })
    }
}

/// A binary harm class for the subscription pack's six-cell decision.
/// The runner owns how it is scored; the pack chooses which classes and
/// strata are meaningful enough to gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarmClass {
    Subscription,
    NotSubscription,
    /// The extraction lens (#279). Deliberately its own pair rather
    /// than a reuse of the subscription words: the two metrics ask
    /// different questions, and a pack declaring the wrong vocabulary
    /// for its metric is refused at load rather than silently gating
    /// nothing.
    Obligation,
    NoObligation,
}

impl HarmClass {
    /// Which metric's vocabulary this class belongs to.
    pub fn metric(self) -> EvalMetric {
        match self {
            Self::Subscription | Self::NotSubscription => EvalMetric::Classification,
            Self::Obligation | Self::NoObligation => EvalMetric::Extraction,
        }
    }

    fn of(classification: &Classification) -> Self {
        if classification.kind == "subscription" {
            Self::Subscription
        } else {
            Self::NotSubscription
        }
    }
}

/// One stratum's classification gate and its plain-language purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationStratum {
    pub description: String,
    pub classes: BTreeMap<HarmClass, ConfidentWrongCeiling>,
}

/// A lower-is-better ceiling on the worst six-cell outcome. The
/// threshold carries its own provenance so it cannot become an orphaned
/// number argued over without its original reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidentWrongCeiling {
    pub max_wilson_95: f64,
    pub reason: String,
    pub date: NaiveDate,
}

impl crate::packs::Pack {
    /// This pack's `eval` block, read as the bars a model must clear.
    pub fn thresholds(&self) -> Thresholds {
        Thresholds::from_eval(&self.manifest.eval)
            // A pack with bars and no gate is refused at load, so the
            // fallback here is only ever reached by a pack with no bars
            // at all — nothing to gate either way.
            .with_gate(self.manifest.eval_gate.unwrap_or(Gate::PerFixture))
            .with_classification_strata(self.manifest.eval_strata.clone())
    }
}

impl Thresholds {
    /// Read a pack's `eval` map. The retired `max_review_rate` key is
    /// ignored so an older installed pack cannot turn a cost into a
    /// verdict gate.
    pub fn from_eval(eval: &BTreeMap<String, f32>) -> Thresholds {
        Thresholds {
            gate: Gate::PerFixture,
            steps: eval
                .iter()
                .filter(|(key, _)| key.as_str() != MAX_REVIEW_RATE_KEY)
                .map(|(key, bar)| (key.clone(), *bar))
                .collect(),
            classification_strata: BTreeMap::new(),
        }
    }

    /// The verdict shape this pack declared. Never inferred from the
    /// bed: a rule the harness picked for you is a rule nobody reviewed.
    pub fn with_gate(mut self, gate: Gate) -> Self {
        self.gate = gate;
        self
    }

    pub fn with_classification_strata(
        mut self,
        strata: BTreeMap<String, ClassificationStratum>,
    ) -> Self {
        self.classification_strata = strata;
        self
    }

    /// Whether this gate can read the bed it is about to judge (#301).
    ///
    /// A per-fixture bar is only meaningful if a fixture can get one
    /// decision wrong and still clear it. With `n` decisions the best
    /// imperfect score is `(n - 1) / n`, so a bar `b` needs
    /// `n >= 1 / (1 - b)` before it separates "nearly right" from
    /// "hopeless". Below that the bar rounds to *no errors at all*, and
    /// the verdict reads the same at 444/445 as at 0/445.
    ///
    /// Derived rather than chosen: the floor falls out of the bar the
    /// pack declared, so there is no second magic number to argue about.
    /// `subscription-audit` at 0.85 needs 7 and carries ~10; the letter
    /// pack at 0.95 needs 20 and carries 1.
    ///
    /// Read on the **median** fixture, not the smallest: a bed is
    /// allowed a few thin fixtures without losing a gate that its bulk
    /// supports.
    pub fn fits(&self, fixtures: &[crate::eval::fixture::Fixture]) -> Result<(), String> {
        if self.gate != Gate::PerFixture || fixtures.is_empty() {
            return Ok(());
        }
        let Some(&worst_bar) = self
            .steps
            .values()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        else {
            return Ok(());
        };
        if worst_bar >= 1.0 {
            return Err(format!(
                "a per-fixture bar of {worst_bar} can never be cleared imperfectly, \
                 whatever the bed"
            ));
        }

        let mut counts: Vec<usize> = fixtures.iter().map(decisions_in).collect();
        counts.sort_unstable();
        let median = counts[counts.len() / 2];
        let needed = (1.0 / (1.0 - worst_bar as f64)).ceil() as usize;
        if median >= needed {
            return Ok(());
        }

        let thin = counts.iter().filter(|&&n| n <= 1).count();
        Err(format!(
            "this pack gates per fixture at {worst_bar}, which needs {needed} decisions in a \
             fixture before one wrong answer can still clear it — but {thin} of {} fixtures \
             carry one decision or none, so the bar means no errors at all, anywhere. \
             Declare \"pooled\" to read the rate across every decision instead.",
            fixtures.len()
        ))
    }

    /// The bar for one step, or `None` if the pack sets none.
    pub fn step(&self, step: &str) -> Option<f32> {
        self.steps.get(step).copied()
    }
}

// ---------------------------------------------------------------------------
// The report

/// One model measured against one pack's fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    /// Pack id, e.g. "app.kttl.subscription-audit".
    pub pack: String,
    pub pack_version: String,
    /// Which independently judged fixture selection produced this
    /// report. Older reports predate the sealed split and read as
    /// development.
    #[serde(default)]
    pub eval_set: fixture::EvalSelection,
    /// The weights under test — `None` for the deterministic floor
    /// (#73), which is a measurement of the pipeline with no model at
    /// all. Absent rather than a placeholder, because a report that
    /// named a model nobody asked would be lying about the one thing
    /// the floor exists to establish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelInfo>,
    /// Results travel with the hardware that produced them.
    pub machine: MachineInfo,
    /// Which evidence dimensions this pack declared and scored, and
    /// which it did not (#430). A reader of a baseline must not have
    /// to guess whether a dimension was clean or simply never asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceCoverage>,
    /// Every declared relation, judged (#427). Empty for packs that
    /// declare none and for records written before relations existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<relations::RelationResult>,
    /// The llama-server that answered, when one did (#74). `None` for a
    /// report produced against a mock endpoint, which is what the tests
    /// and CI use — honest about having no sidecar rather than
    /// inventing a version for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<SidecarInfo>,
    pub fixtures: Vec<FixtureResult>,
    /// How many fixtures were reused from an earlier, interrupted run
    /// (#282). Zero for a run measured in one sitting.
    ///
    /// Recorded for the same reason `sidecar` and `recorded_at` are: a
    /// comparison should be trusted rather than merely believed, and a
    /// run assembled across two sittings has a property a single
    /// sitting does not. The key makes reuse *safe*; this makes it
    /// *visible*.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reused_fixtures: usize,
    /// Runner-owned metric reports derived from the durable item
    /// records. Empty only for legacy reports or packs with no scored
    /// item metrics.
    #[serde(default)]
    pub metrics: BTreeMap<EvalMetric, MetricReport>,
    /// Which questions were asked: a digest of the fixtures this set
    /// actually scored (#320).
    ///
    /// A recording used to say what answered and what the numbers meant,
    /// and nothing about what was asked — so the bed could be rewritten
    /// underneath a baseline and the comparison would report a drop or a
    /// hold, both readings wrong in a way the exit code could not
    /// express. Neither existing guard covers it: a fixture-only change
    /// must not bump the pack version (#319 rewrote 154 exam fixtures and
    /// left development byte for byte identical), and `SCORING_VERSION`
    /// is equally correctly scoped, since what a score means did not move.
    ///
    /// Per eval set, not per pack, and for the same reason `eval_set`
    /// itself is: a pack-wide digest would have retired every development
    /// measurement for #319's exam-only change.
    ///
    /// `None` for a report written before beds were identified. Such a
    /// report is compared with a note rather than refused — refusing
    /// would retire every baseline on disk for a property none of them
    /// could have carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed: Option<String>,
    /// The runtime policy this measurement ran under (#232): context,
    /// reasoning, answer bound — see [`RuntimePolicy`] for why none of
    /// the other provenance fields covers it.
    ///
    /// `None` for a report from before the policy was recorded, for the
    /// deterministic floor (no model is called, so no policy applies),
    /// and for a replay (a recording does not yet say what policy it
    /// ran under). Compared with a note rather than refused, exactly as
    /// a pre-bed report is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimePolicy>,
    /// The verdict for the whole report — see
    /// [`EvalReport::overall_verdict`], which computes it.
    pub verdict: Verdict,
}

impl EvalReport {
    /// What to call this measurement in a table row or a sentence: the
    /// weights' file name, or plain words for the floor that
    /// deliberately has none.
    pub fn model_name(&self) -> &str {
        self.model
            .as_ref()
            .map(|model| model.file.as_str())
            .unwrap_or("without a model")
    }

    /// The verdict across every fixture: the worst one wins. An eval
    /// that averaged its way past a failing fixture would recommend a
    /// tier that cannot do the job on a statement like that one.
    pub fn overall_verdict(&self, thresholds: &Thresholds) -> Verdict {
        let quality = match thresholds.gate {
            Gate::PerFixture => Verdict::worst(
                self.fixtures
                    .iter()
                    .map(|fixture| fixture.verdict(thresholds)),
            ),
            Gate::Pooled => self.pooled_verdict(thresholds),
        };
        if quality == Verdict::Fail || !self.classification_strata_clear(thresholds) {
            return Verdict::Fail;
        }
        // A failed relation fails the report (#427): a model that
        // reads a role swap as two rises is wrong in a way no
        // per-item gate can see. Unjudgeable relations do not fail —
        // review routing is a cost here as everywhere.
        if self
            .relations
            .iter()
            .any(|relation| matches!(relation.outcome, relations::RelationOutcome::Failed { .. }))
        {
            return Verdict::Fail;
        }
        quality
    }

    /// [`Gate::Pooled`]: every decision counts once, and a step clears
    /// its bar only if the **Wilson lower bound** of its pooled rate
    /// does.
    ///
    /// The lower bound rather than the point estimate, because a rate is
    /// an estimate and a bar cleared on thin evidence has not been shown
    /// to be cleared: 19 of 20 is exactly 0.95, and cannot demonstrate a
    /// 95% rate. It is the same reasoning as `max_wilson_95` pointing
    /// the other way — harm is read by its upper bound, quality by its
    /// lower one, and both refuse to be flattered by a small sample.
    ///
    /// A scored step the pack sets no bar for still fails, exactly as
    /// under [`Gate::PerFixture`]: judging it against nothing would
    /// report an unmeasured model as good.
    fn pooled_verdict(&self, thresholds: &Thresholds) -> Verdict {
        if self.fixtures.is_empty() {
            // Nothing ran, so nothing was shown to work — the same
            // answer `Verdict::worst` gives an empty set.
            return Verdict::Fail;
        }

        let mut totals: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for fixture in &self.fixtures {
            for (step, scored) in &fixture.step_scores {
                let entry = totals.entry(step.as_str()).or_insert((0, 0));
                entry.0 += scored.correct;
                entry.1 += scored.expected;
            }
        }

        for (step, (correct, expected)) in totals {
            let Some(bar) = thresholds.step(step) else {
                return Verdict::Fail;
            };
            // A step nothing was expected for cannot demonstrate a rate.
            if expected == 0 {
                return Verdict::Fail;
            }
            let estimate = ProportionEstimate::from_counts(correct, expected);
            let Some(interval) = estimate.wilson_95 else {
                return Verdict::Fail;
            };
            if interval.low < bar as f64 {
                return Verdict::Fail;
            }
        }

        // End to end is the promise about a finished report, so it holds
        // under either gate. Pooled, it is the mean across fixtures
        // rather than the worst one.
        let mean_end_to_end: f32 = self
            .fixtures
            .iter()
            .map(|fixture| fixture.end_to_end)
            .sum::<f32>()
            / self.fixtures.len() as f32;
        if mean_end_to_end < END_TO_END_BAR {
            return Verdict::Fail;
        }

        Verdict::Pass
    }

    /// Whether every declared ceiling clears, read from **whichever
    /// metric the pack declares** (#306).
    ///
    /// This read `EvalMetric::Classification` and nothing else, so an
    /// Extraction pack's ceilings were computed, reported with their
    /// provenance, and then ignored here — the binding never matched and
    /// the verdict failed whatever the gates measured. It failed closed,
    /// which is why it never surfaced as a wrong PASS and why it
    /// survived: the letter pack failed the per-fixture rule too, so one
    /// bug hid behind the other until #301 removed the first.
    ///
    /// A missing metric is still `false`. Failing closed when a declared
    /// gate cannot be evaluated is right; the defect was that one of the
    /// two metrics could never be evaluated at all.
    fn classification_strata_clear(&self, thresholds: &Thresholds) -> bool {
        if thresholds.classification_strata.is_empty() {
            return true;
        }
        thresholds
            .classification_strata
            .iter()
            .all(|(stratum, declaration)| {
                declaration.classes.iter().all(|(class, ceiling)| {
                    self.harm_performance(*class, stratum)
                        .and_then(|performance| performance.confident_wrong_distinct.wilson_95)
                        .is_some_and(|interval| interval.high <= ceiling.max_wilson_95)
                })
            })
    }

    /// One class's performance within one stratum, from the metric that
    /// owns that class's vocabulary. `HarmClass::metric()` decides which
    /// report to read, and a pack declaring a class from the wrong
    /// vocabulary for its metric is already refused at load (#279), so
    /// there is no third case to handle here.
    fn harm_performance(&self, class: HarmClass, stratum: &str) -> Option<&ClassPerformance> {
        match self.metrics.get(&class.metric())? {
            MetricReport::Classification(metrics) => {
                metrics.strata.get(stratum)?.harm_classes.get(&class)
            }
            MetricReport::Extraction(metrics) => {
                metrics.strata.get(stratum)?.harm_classes.get(&class)
            }
        }
    }
}

/// The weights under test. Not [`crate::results::ModelInfo`], which is
/// what a *run* tells a person — a tier name. An eval is the thing that
/// earns a tier its name, so it records the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// File name only, e.g. "qwen2.5-3b-instruct-q4_k_m.gguf" — never a
    /// full path, for the same reason inputs aren't (paths leak a home
    /// directory into a document people share).
    pub file: String,
    /// Parameter count as advertised, e.g. "3B".
    pub params: String,
    /// Quantisation, e.g. "Q4_K_M".
    pub quant: String,
    /// Context window the eval ran at, in tokens.
    pub context: u32,
}

/// The llama-server that ran the weights (#74).
///
/// Kettle designs out most of the model-drift peril — the GGUF is local
/// and pinned, the cache is keyed by model id, no hosted API changes
/// underneath. One channel stays open: the sidecar. A llama-server
/// upgrade can change `json_schema` grammar sampling while the weights
/// are byte-identical, so a score that drops after one would read as
/// "the prompt edit broke it" if the report couldn't name the binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarInfo {
    /// What `llama-server --version` says, e.g. "10050 (b15ca938a)" —
    /// the build number and the commit, which is what identifies it.
    pub version: String,
    /// The binary's file name, e.g. "llama-server-macos-arm64". A file
    /// name, not a path, for the reason [`ModelInfo::file`] is.
    pub file: String,
    /// What the sidecar loaded the model on, in its own startup words
    /// (#490) — e.g. "MTL0 (Apple M1 Pro)", "CUDA0 (NVIDIA GeForce RTX
    /// 5090)", or "CPU" for a build with no accelerator to offer.
    ///
    /// The sidecar's fact, never the host's: the v13 letter baseline
    /// was recorded on a rented RTX 5090 and names only a Xeon, because
    /// `MachineInfo` records what is installed — and what is installed
    /// is not what answered. A CUDA build and a CPU-only build on the
    /// same box are different instruments, and this is the field that
    /// tells them apart. See [`crate::sidecar::device_from_startup`].
    ///
    /// `None` means "not recorded" — a report from before the field
    /// existed, or a startup output that never said — and never means
    /// "no accelerator": a CPU run says "CPU". Compared with a note
    /// rather than refused, exactly as an absent `runtime` or `bed` is;
    /// whether two devices' scores are comparable at all is
    /// `evals/RENTED-GPU.md`'s open question, not this field's claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// The runtime policy a measurement ran under (#232): what the sidecar
/// was told, and what each call was bounded to.
///
/// This is the drift channel none of the other provenance fields cover.
/// The weights are pinned, the bed is digested, the scoring is
/// versioned, the sidecar is named — and still a run under reasoning
/// `auto` and one under reasoning `off` are different measurements of
/// the same everything-else: Gemma 4 under `auto` spent 1,700–2,300
/// hidden tokens per answer and took 10m56s over two fixtures that a
/// deliberate policy runs in seconds. A comparison across that
/// difference reports movement that is only the policy changing, so
/// [`EvalReport`] records the policy and the baseline comparison
/// refuses the drift.
///
/// Built by [`RuntimePolicy::effective`] from the one
/// [`crate::sidecar::SidecarRuntime`] value the sidecar is spawned
/// with, plus the one answer-bound constant every call carries
/// ([`crate::exec::MAX_ANSWER_TOKENS`]) — recorded and executed are the
/// same value by construction, which is the only kind of record worth
/// keeping (#251).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePolicy {
    /// Context window, in tokens — the `-c` the sidecar started with,
    /// and the same value [`ModelInfo::context`] shows beside the
    /// weights.
    pub context: u32,
    /// Concurrent slots — each multiplies the KV cache, so it moves
    /// peak memory without touching a score.
    pub parallel: u32,
    /// Whether the model was allowed to think before answering.
    pub reasoning: crate::sidecar::Reasoning,
    /// The output-token bound on every chat-completions request.
    pub max_answer_tokens: u32,
}

impl RuntimePolicy {
    /// The policy in force for a sidecar spawned with `runtime`.
    pub fn effective(runtime: &crate::sidecar::SidecarRuntime) -> Self {
        RuntimePolicy {
            context: runtime.context,
            parallel: runtime.parallel,
            reasoning: runtime.reasoning,
            max_answer_tokens: crate::exec::MAX_ANSWER_TOKENS,
        }
    }

    /// One line for a CLI message, e.g.
    /// `context 8192, parallel 1, reasoning off, answers bounded at 4096 tokens`.
    pub fn describe(&self) -> String {
        format!(
            "context {}, parallel {}, reasoning {}, answers bounded at {} tokens",
            self.context,
            self.parallel,
            self.reasoning.as_flag(),
            self.max_answer_tokens,
        )
    }
}

/// The machine the numbers were produced on. Without it a score is a
/// boast; with it, it's a tier claim someone can check against their own
/// kit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineInfo {
    /// e.g. "Apple M1".
    pub cpu: String,
    pub ram_gb: u32,
    /// e.g. "macOS 15.5".
    pub os: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One fixture, scored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureResult {
    /// The fixture's file name, e.g. "statement-02-messy.csv".
    pub fixture: String,
    /// Per model step, keyed by the step's name in the pack's `eval`
    /// block.
    pub step_scores: BTreeMap<String, StepScore>,
    /// Durable per-item records from which aggregate scores are derived
    /// (#237). The runner owns their identity, provenance and exchanges;
    /// [`ScoredDecision`] owns the metric-specific payload.
    #[serde(default, alias = "classifications")]
    pub items: Vec<ScoredItem>,
    /// What happened between raw model candidates and the scored
    /// assertions: operational dispositions plus review containment
    /// and wrong answers that escaped into an assertion (#425).
    #[serde(default)]
    pub containment: ContainmentMetrics,
    /// Recurring-set F1 against `expected.json`. `recurring` is
    /// deterministic Rust, so anything below 1.0 here is a Rust bug and
    /// the harness says so (CLAUDE.md).
    pub end_to_end: f32,
    /// Share of the statement that ended up in front of a person:
    /// low-confidence findings plus failed batches.
    pub needs_review_rate: f32,
    pub perf: Perf,
    /// What the repeats disagreed about, when there were repeats (#83).
    ///
    /// `None` for a single run — one run cannot disagree with itself,
    /// and an empty spread beside every score would read as a stability
    /// claim nobody made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability: Option<Stability>,
}

impl FixtureResult {
    /// The authored fixture identity relations join on (#427) — the
    /// items carry it; the file name is presentation.
    pub fn fixture_id(&self) -> &str {
        self.items
            .first()
            .map(|item| item.fixture_id.as_str())
            .unwrap_or(self.fixture.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContainmentMetrics {
    pub candidates: usize,
    pub accepted: usize,
    pub needs_review: usize,
    pub rejected: usize,
    pub deduplicated: usize,
    pub absent_after_retry: usize,
    /// Scored decisions routed to a person rather than asserted.
    pub contained: usize,
    /// Scored decisions asserted incorrectly after their linked traces
    /// had passed the relevant boundaries.
    pub escaped: usize,
    /// Candidates the model got right and a deterministic stage made
    /// wrong (#470) — the errors the containment layer *introduces*,
    /// counted apart so they can never pose as containment or as a
    /// model escape. `#[serde(default)]` keeps v13 baselines
    /// readable: they predate the column and correctly report zero.
    #[serde(default)]
    pub pipeline_introduced: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub by_guardrail: BTreeMap<crate::claim_trace::Guardrail, ContainmentBoundary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContainmentBoundary {
    pub passed: usize,
    pub failed: usize,
    pub contained: usize,
    pub escaped: usize,
}

/// One expected or asserted classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub kind: String,
    pub category: String,
}

/// A scored decision shape understood by this runner. Packs declare the
/// subset they use in `pack.json`; adding Letter to Actions later means
/// adding its metric here, not copying the item-record machinery into a
/// feature pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvalMetric {
    #[default]
    Classification,
    /// What a document obliges someone to do (#279).
    Extraction,
}

/// A reported eval cost. Costs are measured and compared as context,
/// never treated as quality scores or verdict gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCost {
    ReviewRate,
}

/// What the run did with one scored item. Needs-review is a first-class
/// third outcome, not a missing classification and not a false
/// assertion. A low-confidence model answer is retained as `proposed`
/// so the raw decision remains inspectable without pretending Kettle
/// trusted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClassificationOutcome {
    Classified {
        classification: Classification,
    },
    NeedsReview {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proposed: Option<Classification>,
        reason: String,
    },
}

impl ClassificationOutcome {
    pub fn classification(&self) -> Option<&Classification> {
        match self {
            Self::Classified { classification } => Some(classification),
            Self::NeedsReview { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Classified { classification } => {
                format!("{} / {}", classification.kind, classification.category)
            }
            Self::NeedsReview {
                proposed: Some(classification),
                ..
            } => format!(
                "needs-review (proposed {} / {})",
                classification.kind, classification.category
            ),
            Self::NeedsReview { proposed: None, .. } => "needs-review".to_owned(),
        }
    }
}

/// One raw prompt/answer pair that contributed to a scored item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelExchange {
    pub step: String,
    pub batch: usize,
    pub request: String,
    pub response: String,
}

/// The metric-specific part of a scored item. The `metric` discriminator
/// makes the record extensible without making identity, provenance,
/// strata or raw exchanges classification concepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "metric", rename_all = "snake_case")]
pub enum ScoredDecision {
    Classification {
        expected: Classification,
        actual: ClassificationOutcome,
        /// Which pipeline decision produced the asserted `kind` (#272).
        ///
        /// `None` when the run produced no classification at all, so
        /// there was no branch to record — never a default standing in
        /// for one, which is the guess this field exists to remove.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind_from: Option<KindFrom>,
    },
    /// What one passage obliges (#279). `expected: None` is a passage
    /// that asks for nothing — a first-class expectation, because
    /// "there is no deadline here" is exactly the answer a keen
    /// extractor gets wrong.
    Extraction {
        expected: Option<Extracted>,
        /// The expected outcome is a referral (#445): review-routed is
        /// the win, anything asserted or absent is a failure to refer.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        expected_review: bool,
        /// True for a record that exists because the run answered a
        /// passage the bed never authored with "nothing here" (#429):
        /// a correct negative by construction — the bed is synthetic,
        /// so a passage nobody authored genuinely asks nothing. Kept
        /// as a first-class record so the confidence it was declared
        /// at can be calibrated, and excluded from every metric a gate
        /// reads: the declared ceilings were sized on authored
        /// decisions (plus unauthored *assertions*, which are
        /// inventions, #442), and unauthored correct negatives
        /// swelling the no-obligation denominators would move scores
        /// a record-shape change must not move.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unauthored_negative: bool,
        actual: ExtractionOutcome,
    },
}

impl ScoredDecision {
    pub fn metric(&self) -> EvalMetric {
        match self {
            Self::Classification { .. } => EvalMetric::Classification,
            Self::Extraction { .. } => EvalMetric::Extraction,
        }
    }

    pub fn describe_expected(&self) -> String {
        match self {
            Self::Classification { expected, .. } => {
                format!("{} / {}", expected.kind, expected.category)
            }
            Self::Extraction {
                expected: Some(extracted),
                ..
            } => extracted.describe(),
            // The shape-neutral words for "this passage states nothing
            // to extract". "no obligation" was the letter's word for it
            // and would read as a claim about deadlines on a renewal.
            Self::Extraction { expected: None, .. } => "nothing to extract".to_owned(),
        }
    }

    pub fn describe_actual(&self) -> String {
        match self {
            Self::Classification { actual, .. } => actual.describe(),
            Self::Extraction { actual, .. } => actual.describe(),
        }
    }

    pub fn as_classification(&self) -> Option<(&Classification, &ClassificationOutcome)> {
        match self {
            Self::Classification {
                expected, actual, ..
            } => Some((expected, actual)),
            _ => None,
        }
    }

    /// Which pipeline decision produced this item's `kind` (#272).
    /// `None` for a metric whose answers have no kind, and for a
    /// classification the run never reached.
    pub fn kind_from(&self) -> Option<KindFrom> {
        match self {
            Self::Classification { kind_from, .. } => *kind_from,
            Self::Extraction { .. } => None,
        }
    }

    pub fn as_extraction(&self) -> Option<(&Option<Extracted>, &ExtractionOutcome)> {
        match self {
            Self::Extraction {
                expected, actual, ..
            } => Some((expected, actual)),
            _ => None,
        }
    }

    /// True for a record that exists only because the run answered an
    /// unauthored passage with nothing (#429) — calibration evidence,
    /// excluded from every metric a gate reads.
    pub fn is_unauthored_negative(&self) -> bool {
        matches!(
            self,
            Self::Extraction {
                unauthored_negative: true,
                ..
            }
        )
    }
}

/// The auditable core of an obligation: what is asked, of whom, by
/// when. The `ask` prose is deliberately not here — scoring prose wants
/// a reference-distance vocabulary (#158), and a field the scorer
/// ignored would be a fixture lying to its author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedObligation {
    pub kind: String,
    pub party: String,
    pub deadline: String,
    pub anchor: String,
    /// The date the deadline resolves to, authored by the bed rather
    /// than computed at scoring time (#287). Authored, because deriving
    /// it with the same resolver the run used would score the resolver
    /// against itself and agree by construction.
    ///
    /// `None` for a phrase that honestly resolves to no date ("at your
    /// earliest convenience"), where the phrase itself is what a person
    /// reads and so is what gets compared.
    #[serde(default)]
    pub due: Option<NaiveDate>,
}

impl ExpectedObligation {
    /// What this obligation *is*, for every comparison the harness
    /// makes (#554).
    pub fn identity(&self) -> ObligationIdentity {
        ObligationIdentity::of(
            &self.kind,
            &self.party,
            &self.deadline,
            &self.anchor,
            self.due,
        )
    }
}

impl From<&crate::run::Obligation> for ExpectedObligation {
    /// The run's obligation in the shape a bed author writes one —
    /// the one conversion every lens reads a found obligation through,
    /// so a field that joins identity is added here once rather than
    /// to each copy of a struct literal.
    fn from(found: &crate::run::Obligation) -> Self {
        Self {
            kind: found.kind.clone(),
            party: found.party.clone(),
            deadline: found.deadline.clone(),
            anchor: found.anchor.clone(),
            due: found.due.map(|d| d.date),
        }
    }
}

/// One obligation's identity — the single answer to "are these the same
/// obligation" that the pooled join, the harm lens and the evidence
/// `support` dimension all read (#554).
///
/// Before scoring version 15 each of those held its own answer: the
/// join keyed a dated deadline on the day it resolves to and ignored
/// the anchor (#287), the harm lens took the deadline verbatim and the
/// anchor by its date (#452), and `support` took both verbatim. One
/// faithful reading could be found, confident-wrong and unsupported at
/// once.
///
/// The rule: the same kind of ask, on the same party (as the merchant
/// joins read names, case apart), **by the same day arrived at the same
/// way** — and where no day resolved, in the letter's own words, because
/// the words are what a person is shown. A dated deadline's wording and
/// its anchor are the working, not the claim: "within 45 days" counted
/// from the anchor "23 August 2026" and "within 45 days of 23 August
/// 2026" with no anchor are two faithful copies of one letter that
/// resolve to one day by one route. The route — [`DeadlineShape`] — is
/// kept because "by 7 October 2026" for that same letter is the model
/// having done the arithmetic, which the prompt forbids and the report
/// would otherwise present as read from the page; and a fused "by 6
/// March 2026" on a passage whose deadline points at a table row (#544)
/// is likewise not the pointing reading. A wrong anchor date on a
/// counted deadline resolves to the wrong day and is caught there; on
/// an undated one it is compared by the date it names, as #452 had it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObligationIdentity {
    kind: String,
    party: String,
    when: When,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum When {
    Day {
        date: NaiveDate,
        shape: DeadlineShape,
    },
    Words {
        words: String,
        anchor_date: Option<NaiveDate>,
    },
}

impl ObligationIdentity {
    pub fn of(
        kind: &str,
        party: &str,
        deadline: &str,
        anchor: &str,
        due: Option<NaiveDate>,
    ) -> Self {
        let when = match due {
            Some(date) => When::Day {
                date,
                shape: crate::timeline::deadline_shape(deadline),
            },
            None => When::Words {
                words: deadline.to_lowercase(),
                anchor_date: crate::timeline::first_full_date(anchor),
            },
        };
        Self {
            kind: kind.to_owned(),
            party: party.to_ascii_lowercase(),
            when,
        }
    }

    /// The identity as one string, for joins that key on text. Derived
    /// `Debug` quotes and escapes every field, so model-supplied text
    /// cannot collide two obligations the way a bare delimiter could.
    pub fn key(&self) -> String {
        format!("{self:?}")
    }
}

/// A named value a document states (#66, #350): what it is, what it is
/// measured against, what it says, and the passage it was read from.
///
/// `value` is the text as written rather than a `Decimal`, for two
/// reasons. The run's own `Term::value` is verbatim by design — the
/// model copies, Rust parses — so comparing verbatim scores exactly
/// what was read, including a misread currency symbol. And not every
/// named value is money: "14 days" is a term with no decimal to author.
/// The arithmetic that reads these lives in `terms.rs` and is tested
/// there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedTerm {
    pub term: String,
    pub basis: String,
    pub value: String,
    pub quote: String,
}

/// What one passage was read as carrying — in whichever shape its pack
/// extracts (#351).
///
/// The lens around this is generic and stays that way: the miss/invent
/// classification, the Wilson ceilings, the strata and the pooled gate
/// read `describe()` and equality and nothing else. This enum is the
/// one place that knows there is more than one shape, which is the
/// difference between adding a payload and adding an arm to every match
/// site in the harness.
///
/// **Untagged on purpose.** Every recording already on disk writes an
/// obligation as a bare object, and a tag would make each of them
/// unreadable — turning a payload change into a re-measurement of every
/// baseline (#303, #320). The shapes are disjoint and
/// `the_two_payload_shapes_cannot_be_mistaken_for_each_other` is what
/// holds them so; the day a new payload overlaps an existing one, that
/// test fails and somebody chooses a tag deliberately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Extracted {
    Obligation(ExpectedObligation),
    Term(ExpectedTerm),
}

impl Extracted {
    /// This value as one line, for the human table and the item record.
    ///
    /// An obligation's line must never move: it is what a recorded item
    /// says it decided, and changing it would be a scoring change
    /// requiring a `SCORING_VERSION` bump.
    pub fn describe(&self) -> String {
        match self {
            Self::Obligation(obligation) => format!(
                "{} / {} / {}",
                obligation.kind, obligation.party, obligation.deadline
            ),
            Self::Term(term) => format!("{} / {} / {}", term.term, term.basis, term.value),
        }
    }

    /// Do these two say the same thing about what a person acts on
    /// (#452)? The one definition of a correct assertion, read by both
    /// lenses that judge one: the harm cell in
    /// [`extraction_performance`] and the escape count in
    /// `fixture::scored_assertion_is_wrong`.
    ///
    /// Not whole-struct equality, which is what it was. Two fields are
    /// read as written and carry no claim of their own, so comparing
    /// them character by character made the harm lens disagree with the
    /// pooled score on the very same items — letters printed
    /// `obligations (pooled) 1.00 (n=462)` beside a confident-wrong
    /// rate of 0.24, on a run whose every `due` date was right.
    ///
    /// - an **obligation** is the same assertion when it has the same
    ///   [`ObligationIdentity`] — the one the pooled join finds by
    ///   (#287, #554): the same kind of ask, on the same party, by the
    ///   same day arrived at the same way; and where no day resolved,
    ///   in the letter's own words. Version 12 kept the deadline
    ///   verbatim here while the join ignored it, so an obligation could
    ///   be *found* and *confident-wrong* at once — which the re-authored
    ///   exam bed of 21 August reported twelve times, every `due`
    ///   identical.
    /// - a **quote** is compared by containment, exactly as the terms
    ///   lens already joins one to its passage. The run has verified it
    ///   appears verbatim in the source (#258), so a trailing full stop
    ///   or a dropped label prefix is the same evidence said shorter,
    ///   not a different claim.
    ///
    /// Every field the pooled scores join on — term, basis, value —
    /// stays exact, and two payload shapes are never the same assertion.
    pub fn same_assertion_as(&self, other: &Extracted) -> bool {
        match (self, other) {
            (Self::Obligation(want), Self::Obligation(got)) => want.identity() == got.identity(),
            (Self::Term(want), Self::Term(got)) => {
                want.term == got.term
                    && want.basis == got.basis
                    && want.value == got.value
                    && same_quote(&want.quote, &got.quote)
            }
            _ => false,
        }
    }
}

/// Two quotes of the same evidence: either contains the other, on the
/// whitespace rules the terms lens reads a passage by. An empty quote
/// supports nothing, so it is never the same as one that does — and two
/// empty ones are the same absence.
fn same_quote(expected: &str, actual: &str) -> bool {
    if expected.trim().is_empty() || actual.trim().is_empty() {
        return expected.trim().is_empty() && actual.trim().is_empty();
    }
    fixture::same_passage_contains(expected, actual)
        || fixture::same_passage_contains(actual, expected)
}

/// What the run made of one passage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ExtractionOutcome {
    /// A value was asserted from this passage.
    ///
    /// `alias = "obligation"` reads every recording written before the
    /// payload was decoupled; new ones are named for what the field
    /// now holds.
    Found {
        #[serde(alias = "obligation")]
        extracted: Extracted,
    },
    /// The passage reached a person. Neither a hit nor a miss: the
    /// decision was surfaced rather than asserted, and scoring it as a
    /// miss would make the honest floor read as catastrophic.
    NeedsReview { reason: String },
    /// The run read the passage and asserted nothing from it.
    Absent,
}

impl ExtractionOutcome {
    pub fn describe(&self) -> String {
        match self {
            Self::Found { extracted } => extracted.describe(),
            Self::NeedsReview { .. } => "needs-review".to_owned(),
            Self::Absent => "nothing found".to_owned(),
        }
    }
}

/// The confidence a model declared on the answer a scored decision was
/// read from (#429).
///
/// Recorded so calibration can ask whether low/medium/high predicts
/// correctness — a question that needs the confidence *on the scored
/// record*, beside the outcome it was declared for. It is assigned
/// while the scorer still holds the answer paired with its segment,
/// never re-derived from the claim's own text (#457), and a confidence
/// that cannot be traced to the answer that produced *that* decision
/// is recorded as untraceable rather than guessed (#271: a category
/// confidence riding on a cadence decision is a defect, not a
/// calibration data point).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum DeclaredConfidence {
    /// Traced to the answer that produced this decision: the level the
    /// exchange declared, and the id of the claim trace that carries
    /// that answer beside the passage it answered.
    Declared { level: String, trace_id: String },
    /// The exchanges that answered this passage declared more than one
    /// level, so no single level can be traced to the answer that
    /// produced this decision. What was seen is recorded; which answer
    /// this decision inherited is not knowable, so it is never
    /// resolved by choosing one (#271). A calibration reader must
    /// exclude these from every bucket and count them separately.
    Untraceable { levels: Vec<String> },
}

/// One stable, diffable scored decision (#237).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScoredItem {
    /// `pack/fixture_id/item_id`: stable across edits to statement rows,
    /// descriptors and expectation text.
    pub id: String,
    /// Authored, human-readable and pack-unique. Once retired it is
    /// burned permanently.
    pub item_id: String,
    pub pack: String,
    pub pack_version: String,
    /// Content digest of the classify prompt, examples and schema.
    pub prompt_version: String,
    pub fixture: String,
    pub fixture_id: String,
    /// An item may belong to both a fixture-shaped stratum and an
    /// awkward-middle stratum such as an annual renewal.
    pub strata: Vec<String>,
    /// The source text this metric scored. Its meaning belongs to the
    /// decision variant; the runner only preserves it for diagnosis.
    pub raw_input: String,
    /// What makes this one *decision* rather than one row (#310).
    ///
    /// Rows sharing a key are the same question asked again: the same
    /// merchant, the same passage. At temperature 0 they get the same
    /// answer — 87% of repeated merchants and 100% of repeated passages
    /// did — so counting them as separate trials narrows every Wilson
    /// interval a ceiling is judged on, and lets a bed clear a ceiling
    /// on repetition instead of on evidence.
    ///
    /// Written by the scorer, which knows the bed's canonical name,
    /// rather than derived from `raw_input` later: two descriptor forms
    /// of one merchant are one decision, and the string cannot say so.
    #[serde(default)]
    pub decision_key: String,
    #[serde(flatten)]
    pub decision: ScoredDecision,
    /// The declared evidence dimensions, answered for this decision
    /// (#430). Empty when the pack declares none, and on records
    /// written before evidence was scored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evidence: BTreeMap<evidence::EvidenceDimension, evidence::DimensionOutcome>,
    /// Claim-lifecycle records that produced or contained this scored
    /// decision. Empty on historical eval records written before #425.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace_ids: Vec<String>,
    /// The confidence declared on the answer this decision was read
    /// from, with the trace id of the exchange that carried it (#429).
    /// Absent on records written before it existed, and on decisions
    /// no exchange answered (the floor, a batch that failed schema
    /// validation twice): absent means "not recorded", never a level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<DeclaredConfidence>,
    pub exchanges: Vec<ModelExchange>,
}

impl<'de> Deserialize<'de> for ScoredItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            id: String,
            item_id: String,
            pack: String,
            pack_version: String,
            prompt_version: String,
            fixture: String,
            fixture_id: String,
            strata: Vec<String>,
            #[serde(alias = "raw_merchant")]
            raw_input: String,
            #[serde(default)]
            decision_key: String,
            #[serde(default)]
            metric: EvalMetric,
            /// Absent in any record written before #272 — an older item
            /// honestly does not know which branch produced it.
            #[serde(default)]
            kind_from: Option<KindFrom>,
            expected: serde_json::Value,
            actual: serde_json::Value,
            #[serde(default)]
            expected_review: bool,
            #[serde(default)]
            evidence: BTreeMap<evidence::EvidenceDimension, evidence::DimensionOutcome>,
            #[serde(default)]
            trace_ids: Vec<String>,
            /// Absent in any record written before #429 — an older
            /// item honestly does not know what the answer declared.
            #[serde(default)]
            confidence: Option<DeclaredConfidence>,
            #[serde(default)]
            unauthored_negative: bool,
            exchanges: Vec<ModelExchange>,
        }

        let wire = Wire::deserialize(deserializer)?;
        // Each metric owns the shape of its own expected and actual;
        // the shared fields above are read once, whichever it is.
        let decision = match wire.metric {
            EvalMetric::Classification => ScoredDecision::Classification {
                expected: serde_json::from_value(wire.expected)
                    .map_err(serde::de::Error::custom)?,
                actual: serde_json::from_value(wire.actual).map_err(serde::de::Error::custom)?,
                kind_from: wire.kind_from,
            },
            EvalMetric::Extraction => ScoredDecision::Extraction {
                expected: serde_json::from_value(wire.expected)
                    .map_err(serde::de::Error::custom)?,
                expected_review: wire.expected_review,
                unauthored_negative: wire.unauthored_negative,
                actual: serde_json::from_value(wire.actual).map_err(serde::de::Error::custom)?,
            },
        };
        Ok(Self {
            id: wire.id,
            item_id: wire.item_id,
            pack: wire.pack,
            pack_version: wire.pack_version,
            prompt_version: wire.prompt_version,
            fixture: wire.fixture,
            fixture_id: wire.fixture_id,
            strata: wire.strata,
            raw_input: wire.raw_input.clone(),
            // A record written before #310 has no key; the row is its
            // own decision, which is exactly what it meant at the time.
            decision_key: if wire.decision_key.is_empty() {
                wire.raw_input
            } else {
                wire.decision_key
            },
            decision,
            evidence: wire.evidence,
            trace_ids: wire.trace_ids,
            confidence: wire.confidence,
            exchanges: wire.exchanges,
        })
    }
}

/// Review-aware precision and surfaced recall for every classification
/// label, across the whole run and each per-item stratum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationMetrics {
    pub overall: ClassificationSlice,
    pub strata: BTreeMap<String, ClassificationSlice>,
    /// Pack-declared worst-cell gates, including threshold provenance
    /// and the exact evidence judged. Kept in the report so CLI,
    /// baseline and tier output cannot disagree about what passed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gates: BTreeMap<String, BTreeMap<HarmClass, ClassificationGate>>,
    /// #429, and not extraction's alone: the pack that shows a person a
    /// confidence tag is a *classification* pack, so a risk table only
    /// extraction packs could produce would miss the one surface the
    /// question was ever asked about.
    #[serde(default)]
    pub calibration: CalibrationReport,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassificationGate {
    /// The evidence judged: the confident-wrong rate over **distinct
    /// decisions** (#310), never over rows.
    pub observed: ProportionEstimate,
    pub max_wilson_95: f64,
    /// Flattened, so a baseline reads `"outcome": "unproven",
    /// "decisions_needed": 73` beside the ceiling it belongs to rather
    /// than nesting a verdict inside a field called the same thing.
    #[serde(flatten)]
    pub outcome: GateOutcome,
    pub reason: String,
    pub date: NaiveDate,
}

impl<'de> Deserialize<'de> for ClassificationGate {
    /// Reads a gate written before #310 as well as one written after.
    ///
    /// `tiers.json` is a record of measurements already taken, and a
    /// record that stops being readable is a record lost — the same
    /// reason a rendered report keeps the stylesheet it was born with.
    /// Before #310 a gate had two states, so `passes: true` meant Pass
    /// and `passes: false` meant Fail; neither could mean Unproven,
    /// because nothing then could.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            observed: ProportionEstimate,
            max_wilson_95: f64,
            #[serde(default)]
            outcome: Option<String>,
            #[serde(default)]
            decisions_needed: Option<usize>,
            #[serde(default)]
            passes: Option<bool>,
            reason: String,
            date: NaiveDate,
        }

        let wire = Wire::deserialize(deserializer)?;
        let outcome = match (wire.outcome.as_deref(), wire.passes) {
            (Some("pass"), _) => GateOutcome::Pass,
            (Some("fail"), _) => GateOutcome::Fail,
            (Some("unproven"), _) => GateOutcome::Unproven {
                decisions_needed: wire.decisions_needed.unwrap_or_default(),
            },
            (Some(other), _) => {
                return Err(serde::de::Error::custom(format!(
                    "unknown gate outcome {other:?}"
                )))
            }
            (None, Some(true)) => GateOutcome::Pass,
            (None, Some(false)) => GateOutcome::Fail,
            (None, None) => {
                return Err(serde::de::Error::custom(
                    "a gate must say what it came to: outcome, or the pre-#310 passes flag",
                ))
            }
        };
        Ok(ClassificationGate {
            observed: wire.observed,
            max_wilson_95: wire.max_wilson_95,
            outcome,
            reason: wire.reason,
            date: wire.date,
        })
    }
}

/// What a declared ceiling came to (#310).
///
/// Three states, because two cannot tell apart the run that breached a
/// ceiling and the run that could never have demonstrated one. The
/// letter pack made zero errors on 207 distinct decisions against a 1%
/// ceiling needing 381: reporting that as `Fail` says the model got it
/// wrong, when the bed cannot prove it right. `letter_bed.rs` has said
/// so since #242 — "a gate that fails for want of evidence reads
/// exactly like a gate that fails for being wrong" — and this is that
/// sentence made a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GateOutcome {
    Pass,
    Fail,
    /// The bed cannot carry this ceiling's evidence: with zero errors
    /// Wilson's upper bound is `3.84/(n + 3.84)`, so a ceiling of `c`
    /// is unreachable below `3.84/c - 3.84` decisions however well the
    /// model does.
    Unproven {
        decisions_needed: usize,
    },
}

impl GateOutcome {
    /// Whether this ceiling was demonstrated. `Unproven` is not
    /// cleared: you cannot claim what you cannot show.
    pub fn clears(&self) -> bool {
        matches!(self, Self::Pass)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Unproven { .. } => "UNPROVEN",
        }
    }
}

/// The decisions a ceiling of `ceiling` needs before a run with no
/// errors at all could clear it.
pub fn decisions_needed(ceiling: f64) -> usize {
    (3.84 / ceiling - 3.84).ceil().max(0.0) as usize
}

impl ClassificationMetrics {
    pub fn with_gates(mut self, declarations: &BTreeMap<String, ClassificationStratum>) -> Self {
        self.gates = gates_for(declarations, |stratum, class| {
            self.strata
                .get(stratum)
                .and_then(|slice| slice.harm_classes.get(class))
                .map(|performance| performance.confident_wrong_distinct)
        });
        self
    }
}

/// Review-aware precision, surfaced recall and the miss/invent lens for
/// an extraction run (#279), across the whole run and each stratum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionMetrics {
    pub overall: ExtractionSlice,
    pub strata: BTreeMap<String, ExtractionSlice>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gates: BTreeMap<String, BTreeMap<HarmClass, ClassificationGate>>,
    /// #429. `default` so a baseline recorded before this existed still
    /// reads: an absent table is absent evidence, not a zeroed claim.
    #[serde(default)]
    pub calibration: CalibrationReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionSlice {
    pub n: usize,
    /// The miss/invent lens the asymmetric ceilings are read against.
    pub harm_classes: BTreeMap<HarmClass, ClassPerformance>,
}

impl ExtractionMetrics {
    pub fn with_gates(mut self, declarations: &BTreeMap<String, ClassificationStratum>) -> Self {
        self.gates = gates_for(declarations, |stratum, class| {
            self.strata
                .get(stratum)
                .and_then(|slice| slice.harm_classes.get(class))
                .map(|performance| performance.confident_wrong_distinct)
        });
        self
    }
}

/// The declared ceilings judged against whatever evidence a metric
/// observed. Shared by both metrics: a ceiling means the same thing
/// whichever question it is a ceiling on, and one implementation is
/// what stops the two drifting.
fn gates_for(
    declarations: &BTreeMap<String, ClassificationStratum>,
    observed: impl Fn(&str, &HarmClass) -> Option<ProportionEstimate>,
) -> BTreeMap<String, BTreeMap<HarmClass, ClassificationGate>> {
    declarations
        .iter()
        .filter_map(|(stratum, declaration)| {
            if declaration.classes.is_empty() {
                return None;
            }
            let classes = declaration
                .classes
                .iter()
                .map(|(class, ceiling)| {
                    let observed = observed(stratum, class)
                        .unwrap_or_else(|| ProportionEstimate::from_counts(0, 0));
                    let needed = decisions_needed(ceiling.max_wilson_95);
                    let outcome = match observed.wilson_95 {
                        // Below the evidence a flawless run would need,
                        // the ceiling says nothing about the model.
                        _ if observed.n < needed => GateOutcome::Unproven {
                            decisions_needed: needed,
                        },
                        Some(interval) if interval.high <= ceiling.max_wilson_95 => {
                            GateOutcome::Pass
                        }
                        Some(_) => GateOutcome::Fail,
                        // No evidence at all is the extreme of the same
                        // case, not a pass.
                        None => GateOutcome::Unproven {
                            decisions_needed: needed,
                        },
                    };
                    (
                        *class,
                        ClassificationGate {
                            observed,
                            max_wilson_95: ceiling.max_wilson_95,
                            outcome,
                            reason: ceiling.reason.clone(),
                            date: ceiling.date,
                        },
                    )
                })
                .collect();
            Some((stratum.clone(), classes))
        })
        .collect()
}

/// The report shape for one declared [`EvalMetric`]. The map key on
/// [`EvalReport::metrics`] names the metric; this enum keeps each
/// metric's payload typed without imposing classification fields on a
/// future decision shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricReport {
    Classification(ClassificationMetrics),
    Extraction(ExtractionMetrics),
}

impl MetricReport {
    /// One line per declared gate, for diagnoses that need to say why
    /// a verdict fell without the reader re-deriving it (#426).
    pub fn gate_lines(&self) -> Vec<String> {
        let gates = match self {
            MetricReport::Classification(metrics) => &metrics.gates,
            MetricReport::Extraction(metrics) => &metrics.gates,
        };
        gates
            .iter()
            .flat_map(|(stratum, classes)| {
                classes.iter().map(move |(class, gate)| {
                    format!(
                        "  {stratum}/{class:?}: {:?} (observed {}/{} distinct, ceiling {})",
                        gate.outcome, gate.observed.successes, gate.observed.n, gate.max_wilson_95
                    )
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationSlice {
    pub n: usize,
    /// The subscription/not-subscription lens used by the six-cell
    /// appliance-risk gate.
    pub harm_classes: BTreeMap<HarmClass, ClassPerformance>,
    pub kinds: BTreeMap<String, ClassPerformance>,
    pub categories: BTreeMap<String, ClassPerformance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassPerformance {
    /// Correct assertions divided by all assertions of this class.
    /// Needs-review is excluded because Kettle did not assert it.
    pub precision: ProportionEstimate,
    /// Correct assertions plus needs-review divided by all expected
    /// members of this class. Review means the item was surfaced rather
    /// than silently missed.
    pub recall: ProportionEstimate,
    /// Expected members of this class that Kettle confidently asserted
    /// as another class. This is the silent, unrecoverable cell; Stage 3
    /// gives it a per-pack, per-stratum ceiling with provenance.
    pub confident_wrong_rate: ProportionEstimate,
    /// The same cell counted over **distinct decisions** (#310) — one
    /// merchant, one passage, however many rows it produced. This is
    /// what a declared ceiling is judged on, because Wilson assumes
    /// independent trials and repeated rows are not independent: at
    /// temperature 0 the same input gets the same answer.
    ///
    /// A group counts as wrong if *any* of its rows was confidently
    /// wrong. The row rate above stays, as exposure — how much of a
    /// statement a person would see go wrong is a different and equally
    /// real question.
    ///
    /// Defaulted when absent, which is `n = 0` — undefined, so a gate
    /// reading it fails closed. A record written before #310 cannot say
    /// how many decisions it saw, and guessing "the same as its rows"
    /// is the error this field exists to remove.
    #[serde(default)]
    pub confident_wrong_distinct: ProportionEstimate,
    /// That same cell, by the pipeline decision that produced each
    /// wrong answer (#272) — so a failing gate says *what to fix*
    /// rather than leaving somebody to infer the branch from the kind,
    /// which cannot be done and was twice done wrongly.
    ///
    /// Empty for a metric whose answers have no kind, and for records
    /// written before the branch was recorded. Empty means "not known",
    /// never "none".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub confident_wrong_by_path: BTreeMap<KindFrom, usize>,
    pub cells: SixCellCounts,
}

/// One-vs-rest counts for the six outcomes in #237's harm ranking.
/// Kept as counts: weighting them is a pack decision made later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SixCellCounts {
    pub expected_class_asserted_class: usize,
    pub expected_class_needs_review: usize,
    pub expected_class_asserted_other: usize,
    pub expected_other_needs_review: usize,
    pub expected_other_asserted_class: usize,
    pub expected_other_asserted_other: usize,
}

/// Derive classification statistics from durable item records. Strata
/// overlap deliberately: an annual renewal inside a clean statement
/// contributes to both tags, while `overall` counts it once.
pub fn classification_metrics(items: &[ScoredItem]) -> ClassificationMetrics {
    let classification_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|item| item.decision.metric() == EvalMetric::Classification)
        .collect();
    let kind_classes = classes(&classification_items, Dimension::Kind);
    let category_classes = classes(&classification_items, Dimension::Category);
    let overall = classification_slice(&classification_items, &kind_classes, &category_classes);
    let strata = classification_items
        .iter()
        .flat_map(|item| item.strata.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|stratum| {
            let in_stratum: Vec<&ScoredItem> = classification_items
                .iter()
                .copied()
                .filter(|item| item.strata.contains(&stratum))
                .collect();
            (
                stratum,
                classification_slice(&in_stratum, &kind_classes, &category_classes),
            )
        })
        .collect();

    ClassificationMetrics {
        overall,
        strata,
        gates: BTreeMap::new(),
        calibration: calibration(&classification_items),
    }
}

/// Derive extraction statistics from durable item records (#279).
/// Strata overlap exactly as they do for classification.
pub fn extraction_metrics(items: &[ScoredItem]) -> ExtractionMetrics {
    // Unauthored answered-nothing records are calibration evidence,
    // not gate evidence (#429): no declared ceiling was sized on
    // passages nobody authored, so they add records and no evidence.
    let extraction_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|item| item.decision.metric() == EvalMetric::Extraction)
        .filter(|item| !item.decision.is_unauthored_negative())
        .collect();
    let slice = |items: &[&ScoredItem]| ExtractionSlice {
        n: items.len(),
        harm_classes: [HarmClass::Obligation, HarmClass::NoObligation]
            .into_iter()
            .map(|class| (class, extraction_performance(items, class)))
            .collect(),
    };
    let strata = extraction_items
        .iter()
        .flat_map(|item| item.strata.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|stratum| {
            let in_stratum: Vec<&ScoredItem> = extraction_items
                .iter()
                .copied()
                .filter(|item| item.strata.contains(&stratum))
                .collect();
            (stratum, slice(&in_stratum))
        })
        .collect();

    // Calibration reads a wider set than the gates do, deliberately.
    // The gates exclude unauthored answered-nothing records because no
    // ceiling was sized on passages nobody authored; a model's
    // confidence in answering "nothing here" is exactly what a risk
    // table exists to price, so they belong in this one.
    let calibration_items: Vec<&ScoredItem> = items
        .iter()
        .filter(|item| item.decision.metric() == EvalMetric::Extraction)
        .collect();

    ExtractionMetrics {
        overall: slice(&extraction_items),
        strata,
        gates: BTreeMap::new(),
        calibration: calibration(&calibration_items),
    }
}

/// What one decision was, under the lens a risk table needs: was the
/// run right, wrong, or did it decline to say (#429)?
///
/// This is the miss/invent lens of [`extraction_performance`] with its
/// two classes unioned. A miss and an invention are different harms and
/// keep their separate ceilings; for the question *does the declared
/// level predict correctness* they are both simply an error, because
/// the person was told something untrue either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Correctness {
    Right,
    Wrong,
    Routed,
}

fn correctness(item: &ScoredItem) -> Option<Correctness> {
    if let Some((expected, actual)) = item.decision.as_classification() {
        // **Category only, and deliberately.** A classification item
        // carries two dimensions, but the model answers one of them:
        // `kind` is Rust's, from cadence or the pack's category→kind
        // map (#253). The declared confidence is therefore a claim
        // about the category, and charging a cadence error to it would
        // import an answer from a question the model never asked —
        // #271's defect wearing the calibration table as a hat.
        //
        // This narrows what the bucket counts, never what a gate does:
        // the kind and category lenses keep their own ceilings, and a
        // cadence error is as wrong as it ever was. It is only wrong
        // about something this confidence did not claim.
        return Some(match actual {
            ClassificationOutcome::NeedsReview { .. } => Correctness::Routed,
            ClassificationOutcome::Classified { classification } => {
                if classification.category == expected.category {
                    Correctness::Right
                } else {
                    Correctness::Wrong
                }
            }
        });
    }
    let (expected, actual) = item.decision.as_extraction()?;
    Some(match (expected.as_ref(), actual) {
        (_, ExtractionOutcome::NeedsReview { .. }) => Correctness::Routed,
        (Some(want), ExtractionOutcome::Found { extracted }) => {
            if want.same_assertion_as(extracted) {
                Correctness::Right
            } else {
                Correctness::Wrong
            }
        }
        (Some(_), _) => Correctness::Wrong,
        (None, ExtractionOutcome::Found { .. }) => Correctness::Wrong,
        (None, _) => Correctness::Right,
    })
}

/// Confidence levels from most to least confident. Only levels named
/// here can be compared: a ranking claim needs an order, and a level
/// this list does not know has none. Such a level still gets its own
/// bucket in the table — reporting it is honest, ranking by it would
/// not be.
const CONFIDENCE_ORDER: [&str; 3] = ["high", "medium", "low"];

/// Does the level a model declares predict whether it was right (#429)?
///
/// The buckets are the claim. Levels are never mapped to probabilities:
/// "high" means whatever this bed observed it to mean, which on some
/// runs is nothing at all.
///
/// Decisions, not rows, exactly as the ceilings count them (#310) — a
/// person told the wrong thing once has been told the wrong thing. A
/// decision whose rows declared different levels is untraceable to any
/// single one and is counted apart rather than assigned to a bucket by
/// preference (#271).
pub fn calibration(items: &[&ScoredItem]) -> CalibrationReport {
    // decision key -> (levels seen, outcomes seen)
    let mut decisions: BTreeMap<&str, (BTreeSet<&str>, Vec<Correctness>)> = BTreeMap::new();
    let mut untraceable = 0;
    for item in items {
        let Some(outcome) = correctness(item) else {
            continue;
        };
        match item.confidence.as_ref() {
            Some(DeclaredConfidence::Declared { level, .. }) => {
                let entry = decisions.entry(item.decision_key.as_str()).or_default();
                entry.0.insert(level.as_str());
                entry.1.push(outcome);
            }
            Some(DeclaredConfidence::Untraceable { .. }) => untraceable += 1,
            None => untraceable += 1,
        }
    }

    let mut counts: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for (levels, outcomes) in decisions.into_values() {
        let [level] = levels.into_iter().collect::<Vec<_>>()[..] else {
            // Rows of one decision disagreed about the level, so no
            // single level answered for it.
            untraceable += 1;
            continue;
        };
        let entry = counts.entry(level.to_owned()).or_default();
        entry.0 += 1;
        if outcomes.contains(&Correctness::Wrong) {
            entry.1 += 1;
        }
        if outcomes.iter().all(|o| *o == Correctness::Routed) {
            entry.2 += 1;
        }
    }

    let buckets: BTreeMap<String, ConfidenceBucket> = counts
        .into_iter()
        .map(|(level, (decisions, errors, routed_to_review))| {
            // #429's own words: *error among automatically asserted
            // items*. A routed decision was surfaced, not asserted, so
            // it cannot be confidently wrong — counting it in the
            // denominator dilutes the rate with answers the run never
            // gave, and a bucket that was *entirely* routed gets a 0.00
            // it could not have failed to earn.
            //
            // The 14 August subscription replay is why this is not
            // pedantry: 40 of 40 `low` decisions were routed, the
            // guaranteed 0.00 separated from `medium`, and the report
            // called the model INVERTED — while its real comparison,
            // high 0.09 against medium 0.32, points the other way. A
            // bucket with nothing asserted now has an **undefined**
            // rate, which `ranking_signal` cannot compare against.
            let asserted = decisions - routed_to_review;
            (
                level,
                ConfidenceBucket {
                    decisions,
                    errors,
                    routed_to_review,
                    error_rate: ProportionEstimate::from_counts(errors, asserted),
                },
            )
        })
        .collect();

    let signal = ranking_signal(&buckets);
    CalibrationReport {
        buckets,
        untraceable,
        signal,
    }
}

/// What, if anything, the buckets have shown.
///
/// Separation is Wilson intervals that do not overlap. Anything less is
/// **unproven**: two rates that differ by eye have not shown that one
/// is safer than the other, and #429 is explicit that thin evidence is
/// reported as thin rather than quoted as a difference.
fn ranking_signal(buckets: &BTreeMap<String, ConfidenceBucket>) -> RankingSignal {
    let carrying: Vec<(&String, &ConfidenceBucket)> = buckets
        .iter()
        .filter(|(_, bucket)| bucket.decisions > 0)
        .collect();
    match carrying.as_slice() {
        [] => return RankingSignal::NoEvidence,
        [(level, _)] => {
            return RankingSignal::NoVariation {
                level: (*level).clone(),
            }
        }
        _ => {}
    }

    let rank = |level: &str| CONFIDENCE_ORDER.iter().position(|known| *known == level);
    let mut ranks = false;
    for (a_level, a) in &carrying {
        for (b_level, b) in &carrying {
            let (Some(a_rank), Some(b_rank)) = (rank(a_level), rank(b_level)) else {
                continue;
            };
            // `a` is the more confident of the pair.
            if a_rank >= b_rank {
                continue;
            }
            let (Some(a_ci), Some(b_ci)) = (a.error_rate.wilson_95, b.error_rate.wilson_95) else {
                continue;
            };
            if a_ci.high < b_ci.low {
                // The more confident bucket is the safer one, and the
                // intervals say so rather than the point estimates.
                ranks = true;
            } else if b_ci.high < a_ci.low {
                let withdrawn_by = errors_that_would_close_the_gap(b, a_ci.low);
                // The more confident bucket is the *more* wrong one.
                // Reported the moment it is seen, and never averaged
                // against a pair that ranks: routing by a level that
                // points the wrong way spends review on the decisions
                // least likely to need it.
                return RankingSignal::Inverted {
                    more_confident: (*a_level).clone(),
                    less_confident: (*b_level).clone(),
                    withdrawn_by,
                };
            }
        }
    }
    if ranks {
        RankingSignal::Ranks
    } else {
        RankingSignal::Unproven
    }
}

/// The empirical risk carried by each declared confidence level (#429).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub buckets: BTreeMap<String, ConfidenceBucket>,
    /// Decisions no single declared level answered for: none recorded,
    /// or rows that disagreed. Counted, never assigned.
    pub untraceable: usize,
    pub signal: RankingSignal,
}

/// One level's decisions and what they cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceBucket {
    pub decisions: usize,
    pub errors: usize,
    /// Decisions the run declined to assert. Never an error — it was
    /// surfaced, not got wrong — and reported beside the errors
    /// because a rate without its review cost says half of it.
    pub routed_to_review: usize,
    pub error_rate: ProportionEstimate,
}

/// What the buckets established, if anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum RankingSignal {
    /// No decision carried a level to read.
    #[default]
    NoEvidence,
    /// Every decision declared the same level, so nothing is ranked by
    /// it. Not a defect in the measurement — a fact about the model.
    NoVariation { level: String },
    /// Levels varied and no pair of intervals separated. The honest
    /// answer where the bed is too thin, and the one #429 asks for by
    /// name.
    Unproven,
    /// A more confident bucket is measurably safer than a less
    /// confident one.
    Ranks,
    /// A more confident bucket is measurably *less* safe than a less
    /// confident one — the level points the wrong way.
    Inverted {
        more_confident: String,
        less_confident: String,
        /// How many further errors in the safer bucket would end the
        /// separation and withdraw this verdict.
        ///
        /// Reported because non-overlap is a threshold and a reader
        /// cannot see how near it was crossed: the v14 renewal run
        /// separated by 0.0007, which is one decision. A verdict that
        /// fragile is still the honest reading of the evidence, and it
        /// should be quoted with the number that says so.
        withdrawn_by: usize,
    },
}

/// The fewest further errors in `bucket` that would lift its interval
/// to meet `neighbour_low` — the point at which a separation verdict
/// no longer holds.
///
/// Counted by asking the same estimator, rather than solved for: the
/// Wilson bound is what the verdict was read from, so it is what the
/// fragility must be read from too.
fn errors_that_would_close_the_gap(bucket: &ConfidenceBucket, neighbour_low: f64) -> usize {
    let headroom = bucket.decisions.saturating_sub(bucket.errors);
    (1..=headroom)
        .find(|extra| {
            ProportionEstimate::from_counts(bucket.errors + extra, bucket.decisions)
                .wilson_95
                .is_some_and(|interval| interval.high >= neighbour_low)
        })
        .unwrap_or(headroom)
}

#[derive(Clone, Copy)]
enum Dimension {
    Kind,
    Category,
}

fn classes(items: &[&ScoredItem], dimension: Dimension) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    for item in items {
        let Some((expected, actual)) = item.decision.as_classification() else {
            continue;
        };
        classes.insert(label(expected, dimension).to_owned());
        if let Some(asserted) = actual.classification() {
            classes.insert(label(asserted, dimension).to_owned());
        }
    }
    classes
}

fn classification_slice(
    items: &[&ScoredItem],
    kind_classes: &BTreeSet<String>,
    category_classes: &BTreeSet<String>,
) -> ClassificationSlice {
    ClassificationSlice {
        n: items.len(),
        harm_classes: [HarmClass::Subscription, HarmClass::NotSubscription]
            .into_iter()
            .map(|class| (class, harm_performance(items, class)))
            .collect(),
        kinds: performances(items, kind_classes, Dimension::Kind),
        categories: performances(items, category_classes, Dimension::Category),
    }
}

fn performances(
    items: &[&ScoredItem],
    classes: &BTreeSet<String>,
    dimension: Dimension,
) -> BTreeMap<String, ClassPerformance> {
    classes
        .iter()
        .map(|class| (class.clone(), performance(items, class, dimension)))
        .collect()
}

fn performance(items: &[&ScoredItem], class: &str, dimension: Dimension) -> ClassPerformance {
    performance_where(items, |classification| {
        label(classification, dimension) == class
    })
}

fn harm_performance(items: &[&ScoredItem], class: HarmClass) -> ClassPerformance {
    performance_where(items, |classification| {
        HarmClass::of(classification) == class
    })
}

/// What a run asserted about one item, relative to the class being
/// judged. The three states are the whole six-cell model: review is
/// neither assertion, which is why it cannot be folded into `Other`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asserted {
    Class,
    Other,
    Review,
}

/// The six cells, counted from any metric's items. Both metrics feed
/// this: a confident-wrong rate means the same thing whichever
/// question produced it, and one implementation is what stops the
/// subscription lens and the miss/invent lens drifting apart.
fn six_cells(
    items: &[&ScoredItem],
    view: impl Fn(&ScoredItem) -> Option<(bool, Asserted)>,
) -> SixCellCounts {
    let mut cells = SixCellCounts::default();
    for item in items {
        let Some((expected_is_class, asserted)) = view(item) else {
            continue;
        };
        match (expected_is_class, asserted) {
            (true, Asserted::Review) => cells.expected_class_needs_review += 1,
            (false, Asserted::Review) => cells.expected_other_needs_review += 1,
            (true, Asserted::Class) => cells.expected_class_asserted_class += 1,
            (true, Asserted::Other) => cells.expected_class_asserted_other += 1,
            (false, Asserted::Class) => cells.expected_other_asserted_class += 1,
            (false, Asserted::Other) => cells.expected_other_asserted_other += 1,
        }
    }
    cells
}

fn performance_where(
    items: &[&ScoredItem],
    is_class: impl Fn(&Classification) -> bool,
) -> ClassPerformance {
    let view = |item: &ScoredItem| {
        let (expected, actual) = item.decision.as_classification()?;
        let asserted = match actual {
            ClassificationOutcome::NeedsReview { .. } => Asserted::Review,
            ClassificationOutcome::Classified { classification } if is_class(classification) => {
                Asserted::Class
            }
            ClassificationOutcome::Classified { .. } => Asserted::Other,
        };
        Some((is_class(expected), asserted))
    };
    let cells = six_cells(items, view);

    // The silent cell, decomposed by the branch each wrong answer came
    // through. Only that cell: a right answer needs no attribution, and
    // a review-routed one was surfaced rather than asserted.
    let mut by_path: BTreeMap<KindFrom, usize> = BTreeMap::new();
    for item in items {
        if let (Some((true, Asserted::Other)), Some(path)) = (view(item), item.decision.kind_from())
        {
            *by_path.entry(path).or_default() += 1;
        }
    }
    performance_with_paths(cells, by_path, distinct_confident_wrong(items, view))
}

/// One extraction class's performance under the miss/invent lens.
///
/// For `Obligation` the class members are passages that carry one, so
/// the confident-wrong cell is *a miss* — an obligation the run read
/// and asserted nothing about. For `NoObligation` the members are
/// passages that carry none, and the confident-wrong cell is *an
/// invention*. Same six cells, opposite harms, separate ceilings.
fn extraction_performance(items: &[&ScoredItem], class: HarmClass) -> ClassPerformance {
    let wants_obligation = class == HarmClass::Obligation;
    let view = |item: &ScoredItem| {
        let (expected, actual) = item.decision.as_extraction()?;
        let found = matches!(actual, ExtractionOutcome::Found { .. });
        let asserted = match actual {
            ExtractionOutcome::NeedsReview { .. } => Asserted::Review,
            // A found-but-wrong assertion is not a found one (#443):
            // a person told the wrong deadline confidently has been
            // harmed in exactly the way the ceiling bounds, and the
            // mutation harness proved every such mutant cleared every
            // gate while this arm read it as correct. Only where an
            // obligation was expected: an assertion on a passage that
            // expected none is an invention, and stays in this lens's
            // false-positive cell rather than its harm cell.
            ExtractionOutcome::Found { extracted } if wants_obligation => match expected.as_ref() {
                Some(want) if !want.same_assertion_as(extracted) => Asserted::Other,
                _ => Asserted::Class,
            },
            _ if found == wants_obligation => Asserted::Class,
            _ => Asserted::Other,
        };
        Some((expected.is_some() == wants_obligation, asserted))
    };
    performance_with_paths(
        six_cells(items, view),
        BTreeMap::new(),
        distinct_confident_wrong(items, view),
    )
}

fn performance_with_paths(
    cells: SixCellCounts,
    confident_wrong_by_path: BTreeMap<KindFrom, usize>,
    confident_wrong_distinct: ProportionEstimate,
) -> ClassPerformance {
    let precision_successes = cells.expected_class_asserted_class;
    let precision_n = cells.expected_class_asserted_class + cells.expected_other_asserted_class;
    let recall_successes = cells.expected_class_asserted_class + cells.expected_class_needs_review;
    let recall_n = recall_successes + cells.expected_class_asserted_other;

    ClassPerformance {
        precision: ProportionEstimate::from_counts(precision_successes, precision_n),
        recall: ProportionEstimate::from_counts(recall_successes, recall_n),
        confident_wrong_rate: ProportionEstimate::from_counts(
            cells.expected_class_asserted_other,
            recall_n,
        ),
        confident_wrong_distinct,
        confident_wrong_by_path,
        cells,
    }
}

/// The class's members grouped into decisions (#310): one entry per
/// [`ScoredItem::decision_key`], wrong if any of its rows was
/// confidently wrong.
///
/// "Any" rather than "most": a merchant a person is confidently told
/// the wrong thing about on one line of their statement has been told
/// the wrong thing, and a ceiling on unrecoverable harm should not be
/// able to average that away.
fn distinct_confident_wrong(
    items: &[&ScoredItem],
    view: impl Fn(&ScoredItem) -> Option<(bool, Asserted)>,
) -> ProportionEstimate {
    let mut decisions: BTreeMap<&str, bool> = BTreeMap::new();
    for item in items {
        let Some((true, asserted)) = view(item) else {
            continue;
        };
        let wrong = matches!(asserted, Asserted::Other);
        let entry = decisions.entry(item.decision_key.as_str()).or_default();
        *entry = *entry || wrong;
    }
    ProportionEstimate::from_counts(
        decisions.values().filter(|wrong| **wrong).count(),
        decisions.len(),
    )
}

fn label(classification: &Classification, dimension: Dimension) -> &str {
    match dimension {
        Dimension::Kind => &classification.kind,
        Dimension::Category => &classification.category,
    }
}

/// Exact paired comparison on identical classification items. A
/// needs-review outcome is safe for this test: it surfaced the decision
/// without asserting a false fact. Review cost is reported separately.
#[derive(Debug, Clone, PartialEq)]
pub struct McNemarComparison {
    /// Stable ids present in both runs with the same expected answer.
    pub matched: usize,
    /// Safe before, confidently wrong now.
    pub regressions: usize,
    /// Confidently wrong before, safe now.
    pub improvements: usize,
    pub discordant: usize,
    pub discordant_item_ids: Vec<String>,
    pub can_reach_significance: bool,
    pub exact_two_sided_p: f64,
}

pub fn paired_classification_comparison(
    before: &[ScoredItem],
    after: &[ScoredItem],
) -> McNemarComparison {
    let mut regressions = 0;
    let mut improvements = 0;
    let mut matched = 0;
    let mut discordant_item_ids = Vec::new();
    for was in before {
        let Some(now) = after.iter().find(|item| item.id == was.id) else {
            continue;
        };
        // Same item, same question: a pair is only comparable when
        // both runs were scored against the same expectation.
        if was.decision.metric() != now.decision.metric()
            || was.decision.describe_expected() != now.decision.describe_expected()
        {
            continue;
        }
        matched += 1;
        match (
            outcome_is_safe(&was.decision),
            outcome_is_safe(&now.decision),
        ) {
            (true, false) => {
                regressions += 1;
                discordant_item_ids.push(was.id.clone());
            }
            (false, true) => {
                improvements += 1;
                discordant_item_ids.push(was.id.clone());
            }
            _ => {}
        }
    }

    let discordant = regressions + improvements;
    McNemarComparison {
        matched,
        regressions,
        improvements,
        discordant,
        discordant_item_ids,
        // With five discordant pairs the smallest possible exact
        // two-sided p is 0.0625. Six is the first count that can say
        // anything at alpha 0.05, even when every change agrees.
        can_reach_significance: discordant >= 6,
        exact_two_sided_p: exact_mcnemar_p(regressions, improvements),
    }
}

/// Whether a decision's outcome is one nobody is harmed by: right, or
/// surfaced for a person. Review is safe for both metrics — it reached
/// somebody instead of leaving as false reassurance.
fn outcome_is_safe(decision: &ScoredDecision) -> bool {
    match decision {
        ScoredDecision::Classification {
            expected, actual, ..
        } => match actual {
            ClassificationOutcome::NeedsReview { .. } => true,
            ClassificationOutcome::Classified { classification } => classification == expected,
        },
        ScoredDecision::Extraction {
            expected,
            expected_review,
            actual,
            ..
        } => match (expected_review, expected, actual) {
            (_, _, ExtractionOutcome::NeedsReview { .. }) => true,
            // An expected referral that was asserted instead did not
            // refer (#445) — not safe, whatever it asserted.
            (true, _, _) => false,
            (false, Some(want), ExtractionOutcome::Found { extracted }) => extracted == want,
            (false, None, ExtractionOutcome::Absent) => true,
            _ => false,
        },
    }
}

fn exact_mcnemar_p(regressions: usize, improvements: usize) -> f64 {
    let n = regressions + improvements;
    if n == 0 {
        return 1.0;
    }
    let tail = regressions.min(improvements);
    let mut term = 0.5_f64.powi(n as i32);
    let mut cumulative = term;
    for k in 0..tail {
        term *= (n - k) as f64 / (k + 1) as f64;
        cumulative += term;
    }
    (2.0 * cumulative).min(1.0)
}

impl FixtureResult {
    /// This fixture's verdict under a pack's thresholds (brief §6).
    ///
    /// - **Fail** — any scored step below its bar. Also a step the pack
    ///   sets no bar for: a scored step missing from `eval` is a pack
    ///   bug, and judging it against nothing would report an unmeasured
    ///   model as good. Or an end-to-end score short of
    ///   [`END_TO_END_BAR`].
    /// - **Pass** — everything clears on its own.
    ///
    /// Bars are inclusive throughout: a score exactly at its threshold
    /// clears it. Needs-review is the appliance working as designed,
    /// not a quality failure; its rate is retained as a reported cost
    /// and deliberately absent from this computation.
    pub fn verdict(&self, thresholds: &Thresholds) -> Verdict {
        let every_step_clears = self
            .step_scores
            .iter()
            .all(|(step, scored)| thresholds.step(step).is_some_and(|bar| scored.score >= bar));

        if !every_step_clears || self.end_to_end < END_TO_END_BAR {
            Verdict::Fail
        } else {
            Verdict::Pass
        }
    }
}

// ---------------------------------------------------------------------------
// Stability

/// What `--runs 3` found out about one fixture (#83).
///
/// `--runs` exists to confirm stability, not to improve an estimate:
/// grammar-constrained answers at temperature 0 should not move at all,
/// so a spread is a fault to chase rather than sampling noise to smooth
/// away (brief §6). That rules out the obvious shape. A mean would hide
/// exactly the signal the flag exists to surface — 0.95/0.95/0.95 and
/// 1.00/1.00/0.85 average the same, and only one of them is a model
/// anyone should recommend.
///
/// So the reported [`FixtureResult`] stays the **worst** run and carries
/// this beside it: what every repeat agreed on, and what they didn't.
///
/// Deliberately **not** part of [`Verdict`]. Three states are what
/// decision #52 settled and what the app consumes, and "unstable" is a
/// different claim from "not good enough" — it belongs in the output,
/// loudly, not in the enum. Nor is it compared against a baseline: a
/// spread that appears is a fault to chase, but it is not the pack
/// scoring worse than it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stability {
    /// How many repeats this is across. Always at least 2 — a single
    /// run has nothing to be stable about, and reports `None` instead.
    pub runs: u32,
    /// Per model step, keyed as [`FixtureResult::step_scores`] is.
    pub steps: BTreeMap<String, Spread>,
    pub end_to_end: Spread,
    /// The review bucket moving is a stability finding in its own
    /// right: the same statement landed differently in front of a
    /// person, even if every score held.
    pub needs_review_rate: Spread,
    /// One digest per repeat of everything that repeat recorded about
    /// this fixture, deduplicated — so **one entry means every repeat
    /// recorded the same thing** and more than one means they did not.
    ///
    /// The spreads above are a list of the quantities somebody
    /// remembered to watch, and on 19 August the harm ceiling was not
    /// on it (#533): `confident_wrong` is computed from `items`, which
    /// no spread covers, so a repeat could move in exactly the number a
    /// ceiling is a ceiling on while every spread held. This makes the
    /// question total rather than enumerated, and it needs no upkeep
    /// when the report grows a field.
    ///
    /// `perf` is excluded, deliberately: two repeats taking different
    /// wall-clock times is the machine being a machine, not the run
    /// disagreeing with itself.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub record_digests: BTreeSet<String>,
}

impl Stability {
    /// Whether anything the repeats measured disagreed.
    pub fn moved(&self) -> bool {
        self.steps.values().any(Spread::moved)
            || self.end_to_end.moved()
            || self.needs_review_rate.moved()
            || self.record_digests.len() > 1
    }
}

/// The lowest and highest one number reached across a set of repeats.
///
/// Only the ends are kept. The mean is the thing this type exists to
/// avoid reporting, and the individual runs are in the per-run
/// directories for anyone who needs to read the answers themselves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    pub low: f32,
    pub high: f32,
}

impl Spread {
    /// The ends of a set of values. An empty set is `0.0` to `0.0` —
    /// nothing was measured, so nothing moved.
    pub fn across(values: impl IntoIterator<Item = f32>) -> Spread {
        let values: Vec<f32> = values.into_iter().collect();
        Spread {
            low: values.iter().copied().fold(f32::INFINITY, f32::min),
            high: values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        }
        .or_nothing(values.is_empty())
    }

    fn or_nothing(self, empty: bool) -> Spread {
        if empty {
            Spread {
                low: 0.0,
                high: 0.0,
            }
        } else {
            self
        }
    }

    /// Whether the repeats disagreed at all.
    ///
    /// Any movement counts. There is no band here on purpose: at
    /// temperature 0 against a grammar, a threshold would only ever be
    /// a judgement about how much silent drift is acceptable, and the
    /// honest answer is none. [`SCORE_NOISE`] is not that band — it is
    /// there so a JSON round trip cannot be reported as a finding.
    pub fn moved(&self) -> bool {
        self.high - self.low > SCORE_NOISE
    }

    /// "0.85 to 1.00", for a person reading the warning under a table.
    pub fn describe(&self) -> String {
        format!("ranged {:.2} to {:.2}", self.low, self.high)
    }
}

/// A binomial proportion with the evidence needed to interpret it.
/// `None` is the honest result when `n == 0`: precision or recall is
/// undefined, not zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ProportionEstimate {
    pub successes: usize,
    pub n: usize,
    pub estimate: Option<f64>,
    pub wilson_95: Option<ConfidenceInterval>,
}

/// The 95% Wilson score interval around a binomial proportion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub low: f64,
    pub high: f64,
}

impl ProportionEstimate {
    pub fn from_counts(successes: usize, n: usize) -> Self {
        assert!(
            successes <= n,
            "a proportion cannot have more successes than observations"
        );
        if n == 0 {
            return Self {
                successes,
                n,
                estimate: None,
                wilson_95: None,
            };
        }

        let n = n as f64;
        let estimate = successes as f64 / n;
        let z = 1.959_963_984_540_054_f64;
        let z_squared = z * z;
        let denominator = 1.0 + z_squared / n;
        let centre = (estimate + z_squared / (2.0 * n)) / denominator;
        let half_width = z * ((estimate * (1.0 - estimate) / n + z_squared / (4.0 * n * n)).sqrt())
            / denominator;

        Self {
            successes,
            n: n as usize,
            estimate: Some(estimate),
            wilson_95: Some(ConfidenceInterval {
                low: (centre - half_width).max(0.0),
                high: (centre + half_width).min(1.0),
            }),
        }
    }
}

/// How one model step did on one fixture. The counts are kept beside the
/// score so the human table can say "44 of 50" — a fraction people can
/// argue with, where a bare 0.88 is only something to believe.
#[derive(Debug, Clone, PartialEq)]
pub struct StepScore {
    /// 0.0 to 1.0.
    pub score: f32,
    /// How many answers `expected.json` asked for.
    pub expected: usize,
    /// How many the model got right, at the step's tolerance.
    pub correct: usize,
}

impl StepScore {
    pub fn estimate(&self) -> ProportionEstimate {
        ProportionEstimate::from_counts(self.correct, self.expected)
    }
}

impl Serialize for StepScore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Wire {
            score: f32,
            n: usize,
            correct: usize,
            wilson_95: Option<ConfidenceInterval>,
        }

        Wire {
            score: self.score,
            n: self.expected,
            correct: self.correct,
            wilson_95: self.estimate().wilson_95,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StepScore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            score: f32,
            #[serde(alias = "expected")]
            n: usize,
            correct: usize,
            #[serde(default)]
            wilson_95: Option<ConfidenceInterval>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let _ = wire.wilson_95;
        Ok(Self {
            score: wire.score,
            expected: wire.n,
            correct: wire.correct,
        })
    }
}

/// What the run cost. The tier tables are as much about "in four
/// minutes" as about "95% automatic": a model that scores well and takes
/// an hour has failed the *make tea* promise (brief §5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Perf {
    /// Wall-clock time for the whole run, milliseconds.
    pub wall_ms: u64,
    /// Of which was spent waiting on the model.
    pub model_ms: u64,
    pub tokens_per_second: f32,
    /// Peak resident memory, megabytes — the number that decides
    /// whether a tier fits an 8GB machine at all.
    pub peak_rss_mb: u64,
    /// Batches that failed validation and were retried. Non-zero is
    /// worth a look even on a pass.
    pub retries: u32,
}

// ---------------------------------------------------------------------------
// Verdict

/// The answer to "is this model good enough for this pack?".
///
/// Ordered worst-last so [`Verdict::worst`] is a maximum: `Pass` <
/// `Marginal` < `Fail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    /// Retained so historical tier files remain readable. Current
    /// scoring does not derive a marginal verdict from review rate:
    /// appropriately asking a person is not a quality failure.
    Marginal,
    Fail,
}

impl Verdict {
    /// The worst verdict in a set — a report is only as good as its
    /// weakest fixture.
    ///
    /// No fixtures is [`Verdict::Fail`]: nothing ran, so nothing was
    /// shown to work.
    pub fn worst(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
        verdicts.into_iter().max().unwrap_or(Verdict::Fail)
    }

    /// The spelling used in the human table.
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Marginal => "MARGINAL",
            Verdict::Fail => "FAIL",
        }
    }
}

// ---------------------------------------------------------------------------
// tiers.json

/// A pack's `tiers.json` — every model anyone has measured against this
/// pack, with the machine it was measured on (#39, brief §6).
///
/// It ships with the pack, and the model-manager screen reads it to make
/// one sentence true: *"on a machine like yours, this task is typically
/// 95% automatic"*. That is the whole purpose of the file, and the reason
/// its fields are the shape they are.
///
/// It records **every** tier it has measured, including the ones that
/// failed, and leaves the policy about what to recommend to the screen
/// (decision #52). A data file records facts; the UI decides what to do
/// with them. Filtering here would mean the app could not say "we tried
/// this model on a machine like yours and it wasn't good enough", which
/// is a more useful thing to tell someone than silence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TiersFile {
    /// Pack id, e.g. "app.kttl.subscription-audit". The one thing that is true
    /// of the whole file: it is this pack's, and it stays this pack's.
    pub pack: String,
    /// Every measurement, each carrying the pack version and scoring
    /// version it was taken under.
    ///
    /// Provenance is per entry, not per file. A file-level version would
    /// be a claim about rows it cannot check: this file exists to
    /// accumulate measurements taken at different times on machines that
    /// are not all to hand, so re-measuring on one machine would relabel
    /// every other machine's numbers as scored under a version they were
    /// never scored under. The honesty-check machines of brief §5 are
    /// precisely the ones that cannot be re-measured on demand, and
    /// [`SCORING_VERSION`] exists so the app can *refuse* numbers that
    /// are no longer comparable — a promise a single header cannot keep.
    pub tiers: Vec<Tier>,
    /// The deterministic floor, machine by machine (#73): the same
    /// pipeline measured with no model at all. Its own key rather than
    /// a row in `tiers`, because a floor is not a tier — it is the
    /// thing every tier's margin is read against, and the screen must
    /// not be able to offer it for install by iterating `tiers`.
    ///
    /// Per machine because the scores are machine-independent but the
    /// timings are not, and the "in four minutes" half of the sentence
    /// still has to be true on the machine it is said to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baseline: Vec<Tier>,
}

impl TiersFile {
    /// Fold freshly measured tiers into this file, replacing like with
    /// like and leaving everything else exactly as it was.
    ///
    /// An entry is replaced only when the **same model file on the same
    /// machine, pack version, scoring version and eval set** is measured
    /// again — see [`Tier::same_measurement`]. This
    /// is the behaviour the file lives or dies by. Brief §5 plans for
    /// precisely the case that a plain overwrite would destroy: *"keep an
    /// old 8GB laptop as the honesty check"*. Measuring on a 32GB machine
    /// must not silently delete the 8GB numbers, because those are the
    /// ones that make the baseline tier claim a tested fact rather than
    /// an assumption.
    ///
    /// Replacement happens in place, so the order of a file people read
    /// does not churn every time it is rewritten. Genuinely new
    /// measurements go on the end.
    pub fn merge(&mut self, measured: impl IntoIterator<Item = Tier>) {
        for tier in measured {
            let bucket = match tier.model {
                Some(_) => &mut self.tiers,
                // The floor accumulates under `baseline`, replaced
                // machine by machine — same_measurement already treats
                // two no-model entries on one machine as the same
                // measurement, because `None == None`.
                None => &mut self.baseline,
            };
            match bucket
                .iter_mut()
                .find(|existing| existing.same_measurement(&tier))
            {
                Some(existing) => *existing = tier,
                None => bucket.push(tier),
            }
        }
    }
}

/// One model, measured against one pack, on one machine.
///
/// **Every number here is the worst run and the worst fixture.** The
/// screen makes a claim to a person about their own laptop, and the worst
/// case is the honest basis for that sentence (#83). There is no mean
/// anywhere in the file, deliberately: one number per field means the UI
/// cannot quote the flattering one by accident, and a mean would hide
/// exactly what `--runs` exists to surface — 0.95/0.95/0.95 and
/// 1.00/1.00/0.85 average the same and only one is a model worth
/// recommending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    /// `None` is the deterministic floor (#73): the same pipeline, no
    /// model. It lives under [`TiersFile::baseline`], never under
    /// `tiers` — a floor is not a tier, it is the thing tiers are
    /// measured against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelInfo>,
    pub machine: MachineInfo,
    /// The pack version this was measured against.
    pub pack_version: String,
    /// Development, held-out exam and audition measurements are
    /// different claims and must never replace one another.
    #[serde(default)]
    pub eval_set: fixture::EvalSelection,
    /// What these numbers mean, when they were taken — see
    /// [`SCORING_VERSION`]. Never rewritten by a later measurement of
    /// something else.
    pub scoring_version: u32,
    /// Which questions were asked: the bed this measurement ran against
    /// (#320). See [`EvalReport::bed`] for why neither `pack_version`
    /// nor `scoring_version` covers this, and why it is per eval set.
    ///
    /// `None` for a tier recorded before beds were identified — history
    /// is the point of a merging file, and a record that stops being
    /// readable is a record lost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed: Option<String>,
    /// The llama-server that answered, when one did (#74). Absent for a
    /// measurement made against a mock endpoint, as [`EvalReport`] is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<SidecarInfo>,
    /// The runtime policy this measurement ran under (#232), as
    /// [`EvalReport::runtime`] — a tier claim made under reasoning
    /// `auto` is not the claim made under `off`, and the file the
    /// model-manager screen quotes from has to be able to say which it
    /// is. `None` for entries recorded before the policy was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimePolicy>,
    pub measured_at: DateTime<Utc>,
    pub verdict: Verdict,
    /// The share of the statement that never reached a person: `1 -
    /// needs_review_rate`. **This is the number the screen quotes**, and
    /// it is not [`Tier::end_to_end`]. The brief's sentence says it both
    /// ways — "typically 68% automatic — you'll check about 1 in 3 items
    /// yourself" — and 68% against 1 in 3 only reconciles as the review
    /// rate.
    pub automatic: f32,
    /// Wall-clock time for the worst fixture, milliseconds — the "in four
    /// minutes" half of a tier claim (brief §5). One statement's worth,
    /// because that is the unit the sentence is about.
    pub wall_ms: u64,
    /// Per model step, keyed as [`FixtureResult::step_scores`] is.
    /// Version-3 measurements carry the denominator and Wilson interval;
    /// legacy numeric entries remain readable under their older scoring
    /// version.
    pub steps: BTreeMap<String, TierStep>,
    pub end_to_end: f32,
    /// Runner-owned metric evidence, including each proportion's
    /// denominator and interval. Empty only for measurements made
    /// before item metrics existed.
    #[serde(default)]
    pub metrics: BTreeMap<EvalMetric, MetricReport>,
    /// How many repeats this is across.
    pub runs: u32,
    /// Whether every repeat agreed, from [`Stability::moved`]. A single
    /// run is `true`: one run cannot disagree with itself.
    pub steady: bool,
}

impl Tier {
    /// Whether two entries are measurements of the same thing, and so
    /// whether a new one replaces an old one.
    ///
    /// Model file, machine, pack version, scoring version **and** eval
    /// set. Any one omitted can destroy a distinct claim: the same
    /// weights on an M1 Air and an M4 Pro differ, as do evidence from a
    /// changed pack, a changed scoring meaning, or the held-out exam.
    /// The machine is its identifying facts rather than a name, because
    /// "my laptop" is not a thing the file can check.
    pub fn same_measurement(&self, other: &Tier) -> bool {
        let same_model = match (&self.model, &other.model) {
            (Some(mine), Some(theirs)) => mine.file == theirs.file,
            // Two floors on one machine are the same measurement; a
            // floor and a model never are.
            (None, None) => true,
            _ => false,
        };
        same_model
            && self.machine == other.machine
            && self.pack_version == other.pack_version
            && self.scoring_version == other.scoring_version
            && self.eval_set == other.eval_set
    }

    /// The tier a finished report claims, given how many repeats it was
    /// across and when it was measured.
    ///
    /// `report` is already the worst *run* — `cli`'s `worst_run` picks it
    /// — so what is left is to take the worst *fixture* on every axis:
    /// the lowest step score, the lowest end result, the highest share
    /// sent for review, and the longest a statement took. A model that
    /// cannot do a messy statement cannot do the pack, which is the same
    /// rule [`EvalReport::overall_verdict`] already applies to the
    /// verdict.
    pub fn of(report: &EvalReport, runs: u32, measured_at: DateTime<Utc>) -> Tier {
        let mut steps: BTreeMap<String, TierStep> = BTreeMap::new();
        for fixture in &report.fixtures {
            for (step, scored) in &fixture.step_scores {
                let candidate = TierStep::from(scored);
                let worst = steps.entry(step.clone()).or_insert(candidate);
                if candidate.score < worst.score {
                    *worst = candidate;
                }
            }
        }

        // Nothing measured is nothing shown to work: no fixtures reads as
        // "0% automatic, 0.00 end-to-end" rather than as a perfect score
        // arrived at by measuring nothing. It is the same instinct as
        // [`Verdict::worst`] calling an empty set a failure.
        let worst_review_rate = report
            .fixtures
            .iter()
            .map(|fixture| fixture.needs_review_rate)
            .fold(f32::NEG_INFINITY, f32::max);
        let worst_end_to_end = report
            .fixtures
            .iter()
            .map(|fixture| fixture.end_to_end)
            .fold(f32::INFINITY, f32::min);
        let (worst_review_rate, worst_end_to_end) = if report.fixtures.is_empty() {
            (1.0, 0.0)
        } else {
            (worst_review_rate, worst_end_to_end)
        };

        Tier {
            model: report.model.clone(),
            machine: report.machine.clone(),
            pack_version: report.pack_version.clone(),
            eval_set: report.eval_set,
            scoring_version: SCORING_VERSION,
            bed: report.bed.clone(),
            sidecar: report.sidecar.clone(),
            runtime: report.runtime.clone(),
            measured_at,
            verdict: report.verdict,
            automatic: 1.0 - worst_review_rate,
            wall_ms: report
                .fixtures
                .iter()
                .map(|fixture| fixture.perf.wall_ms)
                .max()
                .unwrap_or(0),
            steps,
            end_to_end: worst_end_to_end,
            metrics: report.metrics.clone(),
            runs,
            steady: !report
                .fixtures
                .iter()
                .filter_map(|fixture| fixture.stability.as_ref())
                .any(Stability::moved),
        }
    }
}

/// One tier's worst-fixture step result. Older tiers used a bare score;
/// retaining that wire shape on read and write avoids inventing a
/// denominator for evidence that never recorded one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierStep {
    pub score: f32,
    pub n: Option<usize>,
    pub correct: Option<usize>,
}

impl From<&StepScore> for TierStep {
    fn from(score: &StepScore) -> Self {
        Self {
            score: score.score,
            n: Some(score.expected),
            correct: Some(score.correct),
        }
    }
}

impl Serialize for TierStep {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match (self.n, self.correct) {
            (Some(n), Some(correct)) => {
                #[derive(Serialize)]
                struct Current {
                    score: f32,
                    n: usize,
                    correct: usize,
                    wilson_95: Option<ConfidenceInterval>,
                }
                Current {
                    score: self.score,
                    n,
                    correct,
                    wilson_95: ProportionEstimate::from_counts(correct, n).wilson_95,
                }
                .serialize(serializer)
            }
            _ => self.score.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TierStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Current {
                score: f32,
                n: usize,
                correct: usize,
                #[serde(default)]
                wilson_95: Option<ConfidenceInterval>,
            },
            Legacy(f32),
        }

        Ok(match Wire::deserialize(deserializer)? {
            Wire::Current {
                score,
                n,
                correct,
                wilson_95,
            } => {
                let _ = wilson_95;
                Self {
                    score,
                    n: Some(n),
                    correct: Some(correct),
                }
            }
            Wire::Legacy(score) => Self {
                score,
                n: None,
                correct: None,
            },
        })
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::{
        errors_that_would_close_the_gap, ranking_signal, ConfidenceBucket, ProportionEstimate,
        RankingSignal,
    };
    use std::collections::BTreeMap;

    fn bucket(errors: usize, decisions: usize) -> ConfidenceBucket {
        ConfidenceBucket {
            decisions,
            errors,
            routed_to_review: 0,
            error_rate: ProportionEstimate::from_counts(errors, decisions),
        }
    }

    fn buckets(rows: &[(&str, usize, usize)]) -> BTreeMap<String, ConfidenceBucket> {
        rows.iter()
            .map(|(level, errors, decisions)| ((*level).to_owned(), bucket(*errors, *decisions)))
            .collect()
    }

    /// The v14 renewal run, to the decision: `high` carried every error
    /// and `low` carried none. The levels vary, they separate, and they
    /// separate the wrong way round — so a reader who routed low
    /// confidence to review would have spent 339 reviews and caught
    /// none of the 8 errors.
    /// #429's own words: *error among automatically asserted items*.
    ///
    /// The 14 August subscription replay is why this is not pedantry.
    /// Its `low` bucket held 40 decisions of which **all 40 were routed
    /// to review** — a routed decision is never asserted, so it cannot
    /// be confidently wrong, and its 0.00 error rate was guaranteed by
    /// the routing rather than earned by accuracy. Compared against
    /// `medium`, that produced a confident `Inverted` verdict about a
    /// model whose real comparison — high 0.09 against medium 0.32 —
    /// points the other way.
    ///
    /// So a bucket with no asserted decisions has an **undefined** error
    /// rate, not a zero one, and nothing may be ranked against it.
    #[test]
    fn a_bucket_whose_decisions_were_all_routed_ranks_nothing() {
        let all_routed = ConfidenceBucket {
            decisions: 40,
            errors: 0,
            routed_to_review: 40,
            error_rate: ProportionEstimate::from_counts(0, 0),
        };
        let mut buckets = buckets(&[("medium", 6, 19)]);
        buckets.insert("low".to_owned(), all_routed);
        let signal = ranking_signal(&buckets);
        assert!(
            matches!(signal, RankingSignal::Unproven),
            "a bucket that cannot be wrong is not evidence that it is safe: {signal:?}"
        );
    }

    #[test]
    fn a_level_that_points_the_wrong_way_is_reported_inverted() {
        let signal = ranking_signal(&buckets(&[("high", 8, 342), ("low", 0, 339)]));
        let RankingSignal::Inverted {
            more_confident,
            less_confident,
            withdrawn_by,
        } = signal
        else {
            panic!("high carries every error: {signal:?}");
        };
        assert_eq!(more_confident, "high");
        assert_eq!(less_confident, "low");
        // The separation is 0.0007 wide. Saying so is the difference
        // between a finding and a slogan.
        assert_eq!(withdrawn_by, 1);
    }

    #[test]
    fn a_level_that_points_the_right_way_ranks() {
        let signal = ranking_signal(&buckets(&[("high", 0, 400), ("low", 60, 200)]));
        assert!(matches!(signal, RankingSignal::Ranks), "{signal:?}");
    }

    /// Overlapping intervals have shown nothing, however different the
    /// point estimates look — 0% against 4% is not a finding at n=25.
    #[test]
    fn rates_that_differ_by_eye_without_separating_are_unproven() {
        let signal = ranking_signal(&buckets(&[("high", 1, 25), ("low", 0, 25)]));
        assert!(matches!(signal, RankingSignal::Unproven), "{signal:?}");
    }

    #[test]
    fn one_level_for_everything_ranks_nothing() {
        let signal = ranking_signal(&buckets(&[("high", 24, 476)]));
        assert!(
            matches!(signal, RankingSignal::NoVariation { ref level } if level == "high"),
            "{signal:?}"
        );
    }

    /// A level outside the known order still gets a bucket — reporting
    /// it is honest — but nothing is ranked by it, because a ranking
    /// claim needs an order and this level has none.
    #[test]
    fn an_unordered_level_is_reported_and_never_ranked() {
        let signal = ranking_signal(&buckets(&[("high", 0, 400), ("certain", 60, 200)]));
        assert!(matches!(signal, RankingSignal::Unproven), "{signal:?}");
    }

    #[test]
    fn fragility_counts_up_from_the_errors_the_bucket_already_has() {
        // A bucket with headroom: how many more errors before its upper
        // bound reaches the neighbour's lower bound.
        assert_eq!(errors_that_would_close_the_gap(&bucket(0, 339), 0.0119), 1);
        // A bucket that cannot get there returns its whole headroom
        // rather than pretending a gap survives.
        assert_eq!(errors_that_would_close_the_gap(&bucket(0, 4), 0.99), 4);
    }
}
