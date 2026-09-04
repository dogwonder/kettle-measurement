//! Scoring one fixture's run against its `expected.json` (#25).
//!
//! What is scored is the pipeline's own output. [`crate::run::RunOutcome`]
//! already carries the raw merchant beside the normalised name, and the
//! classification beside both, so the harness reads the run rather than
//! instrumenting it — the thing measured is exactly the thing shipped.

use super::{
    Classification, ClassificationOutcome, ClassificationStratum, ContainmentMetrics, EvalCost,
    EvalMetric, EvalReport, ExpectedObligation, ExpectedTerm, Extracted, ExtractionOutcome,
    FixtureResult, MachineInfo, MetricReport, ModelExchange, ModelInfo, Perf, ScoredDecision,
    ScoredItem, SidecarInfo, StepScore, Verdict,
};
use crate::document;
use crate::exec::BatchItem;
use crate::kinds::KindFrom;
use crate::packs::{InputSpec, Pack};
use crate::recurrence::Period;
use crate::run::{
    run_pack_bound_with_resources, run_pack_with_resources, Answers, AuditOutcome, Payload,
    RunOutcome, RunResources,
};
use crate::run_dir::{RunDir, RunLog};
use crate::scoring::{keyed_accuracy, set_f1, Tolerance};
use crate::sidecar::PeakRss;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// The confidence a model marks an answer with when it wants a person
/// to look — the spelling the packs' classify schemas enum. One
/// definition, in the pipeline that emits it (#271).
use crate::run::LOW_CONFIDENCE;

/// One fixture's `expected.json`: the answers a good run produces.
#[derive(Debug, Clone, Deserialize)]
pub struct Expected {
    /// Authored identity for this fixture, independent of its file name.
    /// Required whenever the fixture contains classification items.
    #[serde(default)]
    pub fixture_id: String,
    /// Development fixtures are available during prompt iteration.
    /// Exam fixtures are sealed until an explicit pack-version-bump run.
    #[serde(default)]
    pub eval_set: EvalSet,
    /// The documents this fixture is made of, by the role each is
    /// supplied as (#354). File names, relative to the fixture's own
    /// directory.
    ///
    /// Absent means the fixture is the single document beside this
    /// file, bound to the pack's sole role — which is every fixture
    /// written before comparison packs existed.
    ///
    /// Named rather than ordered, for the reason #332 gives: order is
    /// invisible at the call site and unverifiable afterwards, and a
    /// renewal diff run the wrong way round does not fail — it reports
    /// a price cut where there was a rise.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    /// Raw statement merchant to the name it should become.
    #[serde(default)]
    pub normalise: Vec<NormaliseExpectation>,
    #[serde(default)]
    pub classify: Vec<ClassifyExpectation>,
    #[serde(default)]
    pub recurring: Vec<RecurringExpectation>,
    /// What a good run extracts from a document (#240). Named for the
    /// role that answers it, like every block above — the expectation
    /// vocabulary is keyed by role, and a pack type adds a block plus a
    /// scorer, not a harness.
    #[serde(default)]
    pub obligations: Vec<ObligationExpectation>,
    /// What a good run reads out of each passage of a comparison
    /// (#356). Keyed by the role that answers it, like every block
    /// above — and spelled exactly as `MODEL_ROLES` spells it, so the
    /// expectation block, the step score and the bar a pack sets in its
    /// manifest are all literally the same string.
    #[serde(default, rename = "policy-terms")]
    pub policy_terms: Vec<TermExpectation>,
    /// Step name to tolerance spelling, e.g. `"fuzzy:0.85"`.
    #[serde(default)]
    pub tolerances: BTreeMap<String, String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalSet {
    #[default]
    Development,
    Exam,
}

impl EvalSet {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Exam => "exam",
        }
    }
}

/// What an eval run selects from the bed: the development set, the
/// sealed exam set, or the audition subset (#539) — the tagged
/// development fixtures a candidate model runs before earning a full
/// bed run. A selection, not a fixture property: fixtures declare
/// `eval_set` and the `audition` overlay, and this names which slice a
/// run asked for — and a report carries it, because a recording must
/// describe itself (#303).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalSelection {
    /// Older reports predate the sealed split and read as development.
    #[default]
    Development,
    Exam,
    Audition,
}

impl EvalSelection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Exam => "exam",
            Self::Audition => "audition",
        }
    }
}

impl Expected {
    /// Is this fixture answerable at all?
    ///
    /// A cadence Kettle doesn't write is a mistake in the fixture, not a
    /// miss by the model, and it must be said out loud: scored quietly,
    /// `"annual"` where the pipeline writes `"yearly"` costs a perfect
    /// model half its end-to-end score and reads in the table as a model
    /// failure. That is the harness lying about the one thing it exists
    /// to measure.
    pub fn validate(&self) -> Result<(), String> {
        if !self.classify.is_empty() {
            validate_authored_id(&self.fixture_id, "fixture id")?;
        }

        let mut item_ids = BTreeSet::new();
        for item in &self.classify {
            validate_authored_id(&item.id, "classification item id")?;
            if !item_ids.insert(item.id.as_str()) {
                return Err(format!(
                    "classification item id '{}' is used more than once",
                    item.id
                ));
            }
            if item.strata.is_empty() {
                return Err(format!(
                    "classification item '{}' has no stratum tags",
                    item.id
                ));
            }
            for stratum in &item.strata {
                validate_kebab_case(stratum, "stratum tag")?;
            }
            if let Some(raw) = &item.raw {
                if !self
                    .normalise
                    .iter()
                    .any(|expectation| expectation.raw.eq_ignore_ascii_case(raw))
                {
                    return Err(format!(
                        "classification item '{}' names raw descriptor '{}' but the normalise \
                         expectations never contain it",
                        item.id, raw
                    ));
                }
            }
        }

        for merchant in self
            .classify
            .iter()
            .map(|e| &e.name)
            .chain(self.recurring.iter().map(|e| &e.merchant))
        {
            // Both scores are keyed on the raw statement merchant so
            // they cannot move with the model's naming, and the
            // normalise block is the only thing tying an expected name
            // back to the statement. Without it the answer could never
            // be matched, and the fixture would quietly score zero for
            // a run that was right.
            if !self.normalise.is_empty()
                && !self
                    .normalise
                    .iter()
                    .any(|e| e.name.eq_ignore_ascii_case(merchant))
            {
                return Err(format!(
                    "the expectations name a merchant ({merchant}) that the normalise \
                     expectations never produce — add the raw statement merchant it comes from."
                ));
            }
        }

        for series in &self.recurring {
            if Period::from_wire(&series.period).is_none() {
                return Err(format!(
                    "'{}' isn't a cadence Kettle knows (in the expectations for {}). \
                     Write weekly, monthly, quarterly or yearly.",
                    series.period, series.merchant,
                ));
            }
        }
        Ok(())
    }
}

/// IDs are part of the diff's human interface. Lowercase kebab-case
/// with at least three segments rules out opaque ordinals such as
/// `item-0042` while keeping names compact enough to scan.
fn validate_authored_id(id: &str, what: &str) -> Result<(), String> {
    let segments: Vec<&str> = id.split('-').collect();
    if segments.len() < 3 || validate_kebab_case(id, what).is_err() {
        return Err(format!(
            "{what} {id:?} is not descriptive lowercase kebab-case — use a name \
             such as annual-renewal-once-yearly-01, never an ordinal such as item-0042"
        ));
    }
    Ok(())
}

fn validate_kebab_case(value: &str, what: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    if valid {
        Ok(())
    } else {
        Err(format!("{what} {value:?} must be lowercase kebab-case"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NormaliseExpectation {
    pub raw: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassifyExpectation {
    /// Authored in the fixture generator's input spec, human-readable,
    /// pack-unique and immutable (#237).
    pub id: String,
    /// Per-item because awkward-middle strata cross fixture boundaries.
    pub strata: Vec<String>,
    /// The authored raw descriptor this decision scores. Optional for
    /// older fixtures where the expected normalised name uniquely
    /// identifies one raw group; required when several descriptors
    /// intentionally merge to the same name.
    #[serde(default)]
    pub raw: Option<String>,
    pub name: String,
    pub kind: String,
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecurringExpectation {
    pub merchant: String,
    pub period: String,
}

/// One scored extraction decision a fixture authors (#240, #279).
///
/// Scored on the auditable core — kind, party, deadline and anchor, all
/// exact. The deadline and anchor are read as written, never computed,
/// so a near miss is a wrong reading rather than a rounding error. The
/// `ask` prose is deliberately not an expectation: scoring prose wants
/// a reference-distance vocabulary (#158), and a field the scorer
/// silently ignored would be a fixture lying to its author.
///
/// `expect: null` is a first-class expectation, not an absent one: a
/// passage that asks for nothing is exactly what a keen extractor gets
/// wrong, and a bed that could not say so could not measure inventions.
#[derive(Debug, Clone, Deserialize)]
pub struct ObligationExpectation {
    /// Authored, human-readable, pack-unique — the #237 identity rules.
    pub id: String,
    /// The difficulties this decision is here to plant.
    pub strata: Vec<String>,
    /// The passage this decision is about, as the document shows it.
    pub segment: String,
    /// What a correct run extracts from that passage, or nothing.
    pub expect: Option<ExpectedObligation>,
    /// Authored evidence ground truth (#430): near-miss claims this
    /// passage does not support, each with its why. The supported
    /// claim needs no authoring — `expect` is it.
    #[serde(default)]
    pub evidence: Option<super::evidence::ExpectedEvidence>,
}

/// One passage of one document, and the named value a correct run reads
/// out of it (#356).
///
/// `expect: null` is a first-class expectation for the same reason it is
/// for an obligation: a passage that states no named value is exactly
/// what a keen extractor invents one from, and a bed that could not say
/// so could not measure inventions.
#[derive(Debug, Clone, Deserialize)]
pub struct TermExpectation {
    /// Authored, human-readable, pack-unique — the #237 identity rules.
    pub id: String,
    /// The difficulties this decision is here to plant.
    pub strata: Vec<String>,
    /// Which document this passage is in, by the role it is supplied
    /// as. Required, and scored: the same value read out of the wrong
    /// year is what turns a £250 rise into a £250 cut, and a bed that
    /// did not say which document it meant could not catch it.
    pub role: String,
    /// The passage this decision is about, as the document shows it.
    pub segment: String,
    /// Authored evidence ground truth (#430): near-miss claims this
    /// passage does not support, each with its why.
    #[serde(default)]
    pub evidence: Option<super::evidence::ExpectedEvidence>,
    /// This passage's correct outcome is a referral (#445): the value
    /// is real, the pack does not model it, and surfacing it to a
    /// person is the win the bed plants. `expect` carries the tuple
    /// the referral shows.
    #[serde(default)]
    pub review: bool,
    /// What a correct run reads from that passage, or nothing.
    pub expect: Option<ExpectedTerm>,
}

#[derive(Debug, Clone)]
struct CapturedExchange {
    exchange: ModelExchange,
    items: Vec<(usize, String)>,
}

/// Keep exchanges in the scored document and, when requested, in the
/// existing run directory too. One logging path prevents the durable
/// item record and the debugging files from disagreeing.
struct EvalRunLog<'a> {
    disk: Option<&'a RunDir>,
    exchanges: RefCell<Vec<CapturedExchange>>,
}

impl<'a> EvalRunLog<'a> {
    fn new(disk: Option<&'a RunDir>) -> Self {
        Self {
            disk,
            exchanges: RefCell::new(Vec::new()),
        }
    }
}

impl RunLog for EvalRunLog<'_> {
    fn exchange(
        &self,
        step: &str,
        batch: usize,
        items: &[BatchItem],
        request: &str,
        response: &str,
    ) {
        if let Some(disk) = self.disk {
            disk.exchange(step, batch, items, request, response);
        }
        self.exchanges.borrow_mut().push(CapturedExchange {
            exchange: ModelExchange {
                step: step.to_owned(),
                batch,
                request: request.to_owned(),
                response: response.to_owned(),
            },
            items: items
                .iter()
                .filter_map(|item| item.source.as_ref().map(|source| (item.id, source.clone())))
                .collect(),
        });
    }
}

/// Measures one model against one pack's fixtures — the half of the
/// harness that needs a model in the room.
///
/// The caller owns the sidecar and passes an [`Endpoint`], exactly as
/// [`run_pack`] does: an eval is a run of the pack, and it must not be
/// able to drift from one by talking to the model differently.
pub struct FixtureEvaluator {
    /// Where answers come from: a model at an endpoint, or the
    /// deterministic pass-through that measures the no-model floor
    /// (#73).
    pub answers: Answers,
    /// The weights under test — `None` when there deliberately are none
    /// ([`Answers::WithoutModel`]), and the report says so rather than
    /// inventing a model that was never asked.
    pub model: Option<ModelInfo>,
    pub machine: MachineInfo,
    /// The llama-server the endpoint is served by, when there is one
    /// (#74). `None` against a mock endpoint.
    pub sidecar: Option<SidecarInfo>,
    /// Live peak-memory measurement for the sidecar process. `None`
    /// means this evaluator has no sidecar (the no-model floor or a mock
    /// endpoint), never that host memory should be substituted.
    pub peak_rss: Option<PeakRss>,
    /// Score against statements here instead of the pack's own
    /// `fixtures/` — `--fixture-dir`. `None` uses the pack's.
    pub fixtures_dir: Option<PathBuf>,
    /// Keep each fixture's model exchanges under here, one run
    /// directory per fixture. `None` keeps nothing.
    ///
    /// An eval that scores 0.60 and cannot say which answers were wrong
    /// is a number nobody can act on: the next prompt edit would be
    /// guesswork against an aggregate. The raw exchanges are the same
    /// ones a real run keeps (brief §11), written the same way.
    pub runs_dir: Option<PathBuf>,
    /// Reuse fixtures already scored under an identical key, and keep
    /// this run's for next time (#282). `None` measures everything, as
    /// before.
    ///
    /// Deliberately separate from `runs_dir`: that directory is
    /// *replaced* each run so two runs' exchanges never mix (#118), and
    /// a cache whose job is to survive the next run cannot live
    /// somewhere built to be replaced by it.
    pub resume_dir: Option<PathBuf>,
    /// Where libpdfium is, so a PDF fixture is read the way the app
    /// reads one (#256). `None` means a PDF fixture cannot be run — and
    /// the eval says so, rather than scoring the bed as if it held only
    /// text. Before this field every eval ran with `RunResources::default()`,
    /// so the PDF path was unmeasured by construction whatever a bed held.
    pub pdfium_dir: Option<PathBuf>,
}

impl FixtureEvaluator {
    /// Run every scorable fixture in `pack` and score it.
    ///
    /// `Err` is a plain-language sentence: the eval could not be run at
    /// all. A fixture the *model* did badly on is not an error — it is
    /// the measurement, and it comes back as a low score.
    pub fn evaluate(&self, pack: &Pack) -> Result<EvalReport, String> {
        self.evaluate_selected(pack, EvalSelection::Development)
    }

    /// Run the sealed exam set on its own. Keeping development out of
    /// this report prevents prompt-tuned evidence masking a held-out
    /// failure at a pack-version bump.
    pub fn evaluate_exam(&self, pack: &Pack) -> Result<EvalReport, String> {
        self.evaluate_selected(pack, EvalSelection::Exam)
    }

    /// Run the audition subset on its own (#539): the committed
    /// go/no-go bed for a candidate model, minutes not hours, whose one
    /// output is whether a full bed run is worth scheduling.
    pub fn evaluate_audition(&self, pack: &Pack) -> Result<EvalReport, String> {
        self.evaluate_selected(pack, EvalSelection::Audition)
    }

    fn evaluate_selected(
        &self,
        pack: &Pack,
        selection: EvalSelection,
    ) -> Result<EvalReport, String> {
        self.evaluate_filtered(pack, selection, None)
    }

    /// One fixture of the selection, by name — the mutation harness
    /// (#426) replays a mutant against only the fixture it touched and
    /// splices the result into its baseline, because replaying a
    /// hundred untouched fixtures per mutant measures nothing extra.
    pub(crate) fn evaluate_only(
        &self,
        pack: &Pack,
        selection: EvalSelection,
        fixture: &str,
    ) -> Result<EvalReport, String> {
        self.evaluate_filtered(pack, selection, Some(fixture))
    }

    fn evaluate_filtered(
        &self,
        pack: &Pack,
        selection: EvalSelection,
        only: Option<&str>,
    ) -> Result<EvalReport, String> {
        let fixtures = match &self.fixtures_dir {
            Some(dir) => fixtures_at_for_eval(
                dir,
                &pack.manifest.eval_items.retired,
                &pack.manifest.eval_items.audition,
                selection,
                &pack.manifest.inputs,
            )?,
            None => fixtures_at_for_eval(
                &pack.dir.join("fixtures"),
                &pack.manifest.eval_items.retired,
                &pack.manifest.eval_items.audition,
                selection,
                &pack.manifest.inputs,
            )?,
        };
        let fixtures: Vec<Fixture> = match only {
            Some(name) => fixtures
                .into_iter()
                .filter(|fixture| fixture.name == name)
                .collect(),
            None => fixtures,
        };
        validate_declared_strata(&fixtures, &pack.manifest.eval_strata)?;
        if fixtures.is_empty() {
            return Err(format!(
                "{} has no fixtures with expectations to score against",
                pack.manifest.id
            ));
        }
        if fixtures
            .iter()
            .any(|fixture| !fixture.expected.classify.is_empty())
            && !pack
                .manifest
                .eval_metrics
                .contains(&EvalMetric::Classification)
        {
            return Err(format!(
                "{} has classification expectations but does not declare the classification \
                 metric in eval_metrics",
                pack.manifest.id
            ));
        }
        if !pack.manifest.eval_costs.contains_key(&EvalCost::ReviewRate) {
            return Err(format!(
                "{} reports review_rate but does not declare its reason and date in eval_costs",
                pack.manifest.id
            ));
        }

        // Digest whichever asking surface this bed actually scores. A
        // prompt edit is the one change this project cannot review
        // unmeasured (CLAUDE.md), and that is as true of an
        // obligations prompt as of a classify one.
        let prompt_version = if fixtures
            .iter()
            .any(|fixture| !fixture.expected.classify.is_empty())
        {
            role_prompt_version(pack, "classify")?
        } else if fixtures
            .iter()
            .any(|fixture| !fixture.expected.obligations.is_empty())
        {
            role_prompt_version(pack, "obligations")?
        } else if fixtures
            .iter()
            .any(|fixture| !fixture.expected.policy_terms.is_empty())
        {
            // As true of a policy-terms prompt as of a classify one: an
            // unmeasured prompt edit is the one change this project
            // cannot review, and it can only be measured if the item
            // record says which prompt produced it.
            role_prompt_version(pack, "policy-terms")?
        } else {
            "not-applicable".to_owned()
        };
        let cache = self
            .resume_dir
            .as_ref()
            .map(|dir| super::resume::ResumeCache::at(dir));
        let mut reused = 0usize;
        let mut unrunnable: Vec<String> = Vec::new();
        let mut results = Vec::new();
        // Which questions this set was asked (#320), collected as they
        // are scored so it can only ever describe the fixtures that
        // actually ran.
        let mut bed: Vec<(String, String)> = Vec::new();
        for fixture in &fixtures {
            // What this fixture asked, hashed once: the resume key wants
            // it, and so does the bed digest (#320). One computation, so
            // the two can never disagree about what a fixture is.
            let digest = digest_of(fixture);
            bed.push((fixture.name.clone(), digest.clone()));
            // A fixture already scored under an identical key is not
            // measured again (#282). The key is computed even on a
            // miss, so this run's result can be kept for the next one.
            let resume_key = cache.as_ref().map(|_| super::resume::ResumeKey {
                pack: pack.manifest.id.clone(),
                pack_version: pack.manifest.version.clone(),
                prompt_version: prompt_version.clone(),
                model: self
                    .model
                    .as_ref()
                    .map(|model| model.file.clone())
                    .unwrap_or_else(|| "no-model".to_owned()),
                sidecar: self
                    .sidecar
                    .as_ref()
                    .map(|sidecar| sidecar.version.clone())
                    .unwrap_or_else(|| "none".to_owned()),
                device: self
                    .sidecar
                    .as_ref()
                    .and_then(|sidecar| sidecar.device.clone())
                    .unwrap_or_else(|| "unrecorded".to_owned()),
                scoring_version: super::SCORING_VERSION,
                eval_set: fixture.expected.eval_set,
                fixture: fixture.name.clone(),
                fixture_digest: digest.clone(),
            });
            if let (Some(cache), Some(key)) = (cache.as_ref(), resume_key.as_ref()) {
                if let Some(kept) = cache.get(key) {
                    reused += 1;
                    results.push(kept);
                    continue;
                }
            }
            // A document this build has no reader for is not a
            // measurement and not a crash: it is a fixture this machine
            // cannot ask about. Named in the report and skipped here,
            // because failing the whole run made the deterministic
            // floor untestable on any machine without pdfium, which is
            // every CI runner (#256).
            if !document::readable_here(&fixture.path, self.pdfium_dir.as_deref())
                || fixture
                    .inputs
                    .iter()
                    .any(|(_, path)| !document::readable_here(path, self.pdfium_dir.as_deref()))
            {
                unrunnable.push(fixture.name.clone());
                continue;
            }
            // An endpoint is shared across this evaluator's fixtures;
            // each report row must start at zero rather than inheriting
            // the statement before it.
            self.answers.take_model_metrics();
            let started = Instant::now();
            let kept = self.run_dir_for(pack, fixture);
            let log = EvalRunLog::new(kept.as_ref());
            // Bound by role where the fixture says which document is
            // which (#354); otherwise the sole-role binding `run_pack`
            // has always made, which is every single-document fixture.
            let resources = RunResources {
                pdfium_dir: self.pdfium_dir.as_deref(),
            };
            let outcome = match fixture.inputs.as_slice() {
                [] => run_pack_with_resources(
                    pack,
                    std::slice::from_ref(&fixture.path),
                    &self.answers,
                    resources,
                    &AtomicBool::new(false),
                    &mut |_| {},
                    &log,
                ),
                bound => {
                    let named: Vec<(&str, PathBuf)> = bound
                        .iter()
                        .map(|(role, path)| (role.as_str(), path.clone()))
                        .collect();
                    run_pack_bound_with_resources(
                        pack,
                        &named,
                        &self.answers,
                        resources,
                        &AtomicBool::new(false),
                        &mut |_| {},
                        &log,
                    )
                }
            }
            .map_err(|e| format!("{} couldn't be run: {e}", fixture.name))?;
            if let Some(dir) = &kept {
                let _ = dir.record_claims(&outcome.claim_traces);
            }
            let model_perf = self.answers.take_model_metrics();
            // The model is one constituent of the run. Keep that
            // invariant even when llama-server and the harness round
            // their millisecond clocks differently.
            let wall_ms = (started.elapsed().as_millis() as u64).max(model_perf.model_ms);

            let exchanges = log.exchanges.into_inner();
            let gated_strata: Vec<String> = pack
                .manifest
                .eval_strata
                .iter()
                .filter(|(_, declaration)| !declaration.classes.is_empty())
                .map(|(stratum, _)| stratum.clone())
                .collect();
            // Evidence existence is checked against the source text
            // (#430). A binary source no scorer can read as text
            // contributes nothing — a declared existence dimension then
            // honestly fails on any claim whose words cannot be found.
            let sources: Vec<String> = fixture
                .inputs
                .iter()
                .filter_map(|(_, path)| std::fs::read_to_string(path).ok())
                .collect();
            let result = score_fixture_with_provenance(
                &fixture.name,
                &fixture.expected,
                &outcome,
                Some(Perf {
                    wall_ms,
                    model_ms: model_perf.model_ms,
                    tokens_per_second: model_perf.tokens_per_second,
                    peak_rss_mb: self.peak_rss.as_ref().map(PeakRss::megabytes).unwrap_or(0),
                }),
                model_perf.retries.total(),
                &ItemProvenance {
                    pack: &pack.manifest.id,
                    pack_version: &pack.manifest.version,
                    prompt_version: &prompt_version,
                    exchanges: &exchanges,
                    evidence: &pack.manifest.eval_evidence,
                    sources: &sources,
                    gated_strata: &gated_strata,
                },
            );
            if let Some(dir) = &kept {
                let document = serde_json::to_string_pretty(&result.items)
                    .expect("scored item records serialise")
                    + "\n";
                // Same posture as raw exchange logging: a full disk
                // must not discard the measurement returned in memory.
                let _ = dir.write_output("eval-items.json", &document);
            }
            if let (Some(cache), Some(key)) = (cache.as_ref(), resume_key.as_ref()) {
                // Same posture as run logging: failing to write a note
                // about a measurement is no reason to discard it.
                let _ = cache.put(key, &result);
            }
            results.push(result);
        }

        // Declared relations, judged over the scored fixtures (#427).
        // Relations are bed content: the same fixtures under different
        // declarations are a different measurement, so the file joins
        // the bed digest the fixtures already feed.
        let relations_path = match &self.fixtures_dir {
            Some(dir) => dir.join("relations.json"),
            None => pack.dir.join("fixtures").join("relations.json"),
        };
        bed.extend(relations_entry(&relations_path));
        // A single-fixture replay (the mutation harness) judges none:
        // its relations would name fixtures deliberately absent.
        let relations = if only.is_some() {
            Vec::new()
        } else {
            let path = relations_path;
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let file: super::relations::RelationsFile = serde_json::from_str(&text)
                        .map_err(|e| {
                            format!("{} isn't a valid relations file: {e}", path.display())
                        })?;
                    let carries =
                        |id: &str| results.iter().any(|fixture| fixture.fixture_id() == id);
                    let in_selection: Vec<super::relations::RelationDeclaration> = file
                        .relations
                        .into_iter()
                        .filter(|declaration| {
                            let left = carries(&declaration.left);
                            let right = carries(&declaration.right);
                            // The audition is a *declared* subset (#539)
                            // — six to ten fixtures chosen for what a
                            // go/no-go needs — so carrying one half of a
                            // twin pair is the set working as designed,
                            // and a relation it cannot judge is simply
                            // not judged. Relations never print on a
                            // partial bed.
                            //
                            // Development and exam are complete beds, so
                            // a half-present relation there reaches
                            // across the set boundary: absent by mistake
                            // rather than by declaration, and a bed that
                            // cannot state its declared relationships is
                            // a different bed. Keeping it here is what
                            // makes `judge` refuse it, deliberately.
                            //
                            // Only the selection can tell those two
                            // apart, which is why this is not simply
                            // "both halves present".
                            if selection == EvalSelection::Audition {
                                left && right
                            } else {
                                left || right
                            }
                        })
                        .collect();
                    super::relations::judge(&in_selection, &results)?
                }
                Err(_) => Vec::new(),
            }
        };

        let all_items: Vec<ScoredItem> = results
            .iter()
            .flat_map(|fixture| fixture.items.iter().cloned())
            .collect();
        let mut metrics = BTreeMap::new();
        if pack
            .manifest
            .eval_metrics
            .contains(&EvalMetric::Classification)
        {
            metrics.insert(
                EvalMetric::Classification,
                MetricReport::Classification(
                    super::classification_metrics(&all_items)
                        .with_gates(&pack.manifest.eval_strata),
                ),
            );
        }
        if pack.manifest.eval_metrics.contains(&EvalMetric::Extraction) {
            metrics.insert(
                EvalMetric::Extraction,
                MetricReport::Extraction(
                    super::extraction_metrics(&all_items).with_gates(&pack.manifest.eval_strata),
                ),
            );
        }
        let thresholds = pack.thresholds();
        let mut report = EvalReport {
            reused_fixtures: reused,
            unrunnable,
            pack: pack.manifest.id.clone(),
            pack_version: pack.manifest.version.clone(),
            eval_set: selection,
            model: self.model.clone(),
            machine: self.machine.clone(),
            evidence: super::EvidenceCoverage::from_declared(&pack.manifest.eval_evidence),
            relations,
            sidecar: self.sidecar.clone(),
            fixtures: results,
            bed: Some(super::resume::bed_digest(&bed)),
            // Stamped by the caller that spawned the sidecar — this
            // evaluator only sees an endpoint, and inventing a policy
            // it cannot know would be the drift #232 exists to close.
            runtime: None,
            metrics,
            // Replaced immediately below — the verdict is derived from
            // the fixtures, never passed in.
            verdict: Verdict::Fail,
        };
        report.verdict = report.overall_verdict(&thresholds);
        Ok(report)
    }

    /// Somewhere to keep one fixture's exchanges, if asked for and if
    /// the disk allows it. Re-running an eval replaces what the last
    /// one wrote (see the `replace` call below).
    ///
    /// A directory that cannot be made stops the logging, never the
    /// eval: losing the answers because nobody could write a note about
    /// them is the wrong way round (see [`RunLog`]).
    fn run_dir_for(&self, pack: &Pack, fixture: &Fixture) -> Option<RunDir> {
        let root = self.runs_dir.as_ref()?;
        let run_id = crate::run_dir::eval_run_id(
            &pack.manifest.id,
            self.model.as_ref().map(|model| model.file.as_str()),
            &fixture.name,
        );
        // Deliberately `replace`: these ids are a function of what was
        // scored, so running the same eval again means this run's
        // exchanges, not a mix of two (#118).
        let dir = RunDir::replace(root, &run_id).ok()?;
        // Whose answers these are, written where a replay can read it
        // (#303). The id above names the model too, but a directory
        // name is a label — parsing one back is guesswork the moment a
        // file name changes, and a recording should describe itself.
        let _ = dir.record_model(self.model.as_ref());
        Some(dir)
    }
}

/// One statement a pack can be scored against, and the answers a good
/// run produces for it.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// File name only, e.g. "statement-02-messy.csv" — what the table
    /// prints and what [`FixtureResult::fixture`] records.
    ///
    /// A baseline records this, so it is identity rather than
    /// decoration: renaming a fixture retires every measurement of it.
    pub name: String,
    /// The fixture's first document. Kept because a single-document
    /// fixture is still what almost every fixture is, and because
    /// `--fixture-dir` and the tests read it directly.
    pub path: PathBuf,
    /// Every document this fixture is made of, and the role each is
    /// supplied as (#354), in the order the pack declares its inputs.
    ///
    /// Empty when discovery had no manifest to bind against —
    /// [`fixtures_at`] on somebody's own directory. The run then binds
    /// `path` to the pack's sole role exactly as it always has, so an
    /// empty list means "one document, role resolved at run time"
    /// rather than "no documents".
    pub inputs: Vec<(String, PathBuf)>,
    /// The `expected.json` these answers were read from. Held rather
    /// than derived from `path`: a multi-document fixture is named
    /// after its expectations, because no one of its documents is the
    /// fixture.
    pub expectations: PathBuf,
    pub expected: Expected,
}

/// The relations half of a bed, or nothing when a set declares none.
///
/// A bed is two kinds of thing: the fixtures, and the relations judged
/// over them (#427). The same fixtures under different declarations are
/// a different measurement, so both feed the digest — and both must be
/// composed in exactly one place. #546 recomputes a bed from the working
/// tree to check a claim's `bed change` trigger, and a second, separate
/// spelling of "what enters a bed" is how that check would come to
/// disagree with the run it is checking. It did, on the first attempt:
/// the fixtures matched and the relations were missing, which reads as
/// two live claims going stale on a bed that never moved.
pub fn relations_entry(path: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    Some((
        "relations.json".to_owned(),
        format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex()),
    ))
}

/// A bed's entries, as a run records them: every fixture it scored,
/// plus the relations declared over them. Feed to
/// [`crate::eval::resume::bed_digest`].
pub fn bed_entries(fixtures: &[Fixture], relations_path: &Path) -> Vec<(String, String)> {
    let mut bed: Vec<(String, String)> = fixtures
        .iter()
        .map(|fixture| (fixture.name.clone(), digest_of(fixture)))
        .collect();
    bed.extend(relations_entry(relations_path));
    bed
}

/// Everything this fixture asks, hashed: every document plus the
/// expectations beside them (#354).
///
/// A single-document fixture hashes to exactly what
/// [`crate::eval::resume::fixture_digest`] has always produced, because
/// every recorded bed digest (#320) and every resume key is built from
/// it — a changed hash refuses baselines that are perfectly valid.
pub fn digest_of(fixture: &Fixture) -> String {
    crate::eval::resume::fixture_digest_of(&fixture.documents(), &fixture.expectations)
}

impl Fixture {
    /// Every document this fixture puts into a run, in binding order.
    /// One document when discovery had no manifest — see [`Self::inputs`].
    pub fn documents(&self) -> Vec<PathBuf> {
        if self.inputs.is_empty() {
            return vec![self.path.clone()];
        }
        self.inputs.iter().map(|(_, path)| path.clone()).collect()
    }
}

/// The fixtures in a pack's `fixtures/` directory that can be scored:
/// each statement that has an `expected.json` beside it, named for it.
///
/// A statement without expectations is skipped rather than refused. It
/// is a perfectly good fixture for a person running the pack by hand
/// with `--fixture-dir` — it just has no answers to be marked against,
/// and inventing a verdict for it would be worse than leaving it out.
///
/// Ordered by name, so a table's rows don't shuffle between runs.
pub fn fixtures_in(pack: &Pack) -> Result<Vec<Fixture>, String> {
    fixtures_at_with_roles(
        &pack.dir.join("fixtures"),
        &pack.manifest.eval_items.retired,
        &pack.manifest.inputs,
    )
}

/// The scorable fixtures in any directory — what `--fixture-dir` points
/// at when someone scores a model against their own statements rather
/// than the pack's synthetic ones. Those files are gitignored and never
/// come into the repo (CLAUDE.md, data rules), so they are always
/// somewhere else entirely.
pub fn fixtures_at(dir: &Path) -> Result<Vec<Fixture>, String> {
    fixtures_at_with_retired(dir, &[])
}

/// Select fixtures for an eval without weakening pack-wide ID checks.
///
/// Every fixture is loaded and validated first, including the sealed
/// exam set, so an exam item cannot reuse a live or retired development
/// ID merely because the ordinary run filters it out afterwards.
///
/// `roles` is the pack's declared inputs (#354): a fixture may then say
/// which of its documents is which, and a role the pack never declared
/// is refused here rather than after a sidecar is up and the first
/// fixtures are spent.
pub fn fixtures_at_for_eval(
    dir: &Path,
    retired_item_ids: &[String],
    audition: &[String],
    selection: EvalSelection,
    roles: &[InputSpec],
) -> Result<Vec<Fixture>, String> {
    let fixtures = fixtures_at_with_roles(dir, retired_item_ids, roles)?;
    if selection == EvalSelection::Audition {
        return audition_fixtures(dir, fixtures, audition);
    }
    let selected = match selection {
        EvalSelection::Development => EvalSet::Development,
        EvalSelection::Exam => EvalSet::Exam,
        EvalSelection::Audition => unreachable!("handled above"),
    };
    Ok(fixtures
        .into_iter()
        .filter(|fixture| fixture.expected.eval_set == selected)
        .collect())
}

/// Resolve the manifest's declared audition names (#539) against what
/// is actually on disk. Every failure is a refusal rather than a
/// filter: a silently shrinking audition would still print a digest and
/// read as the committed instrument.
fn audition_fixtures(
    dir: &Path,
    fixtures: Vec<Fixture>,
    audition: &[String],
) -> Result<Vec<Fixture>, String> {
    // Zero fixtures scoring zero questions would read as a pass, and
    // the set that exists to gatekeep full runs must fail without its
    // fixtures, never skip quietly into green (PR #99).
    if audition.is_empty() {
        return Err(format!(
            "the audition set is empty: the pack declares no audition fixtures \
             (eval_items.audition) for {}",
            dir.display()
        ));
    }
    let mut by_name: BTreeMap<&str, &Fixture> = BTreeMap::new();
    for fixture in &fixtures {
        by_name.insert(fixture.name.as_str(), fixture);
    }
    for name in audition {
        let Some(fixture) = by_name.get(name.as_str()) else {
            return Err(format!(
                "the audition list names {name}, and no such fixture exists in {}",
                dir.display()
            ));
        };
        // The holdout's job is to be unseen, and a set candidate models
        // run against during triage is the opposite of unseen.
        if fixture.expected.eval_set == EvalSet::Exam {
            return Err(format!(
                "the audition list names {name}, which is in the sealed exam set: \
                 audition draws on development fixtures only"
            ));
        }
    }
    let declared: BTreeSet<&str> = audition.iter().map(String::as_str).collect();
    Ok(fixtures
        .into_iter()
        .filter(|fixture| declared.contains(fixture.name.as_str()))
        .collect())
}

/// Refuse typo-shaped strata before spending model time.
///
/// Empty declarations retain compatibility for packs that have not
/// adopted stratified classification. Once a pack declares the
/// registry, every per-item tag must name one of its entries, including
/// report-only diagnostic strata whose class map is deliberately empty.
pub fn validate_declared_strata(
    fixtures: &[Fixture],
    declared: &BTreeMap<String, ClassificationStratum>,
) -> Result<(), String> {
    if declared.is_empty() {
        return Ok(());
    }
    for fixture in fixtures {
        // Every per-item block that carries strata, whichever role
        // answers it. A block this loop did not know about could tag
        // items with a stratum nobody declared, and a typo'd tag scores
        // in a slice no ceiling reads.
        let tagged = fixture
            .expected
            .classify
            .iter()
            .map(|item| (&item.id, &item.strata))
            .chain(
                fixture
                    .expected
                    .policy_terms
                    .iter()
                    .map(|item| (&item.id, &item.strata)),
            )
            .chain(
                fixture
                    .expected
                    .obligations
                    .iter()
                    .map(|item| (&item.id, &item.strata)),
            );
        for (id, strata) in tagged {
            for stratum in strata {
                if !declared.contains_key(stratum) {
                    return Err(format!(
                        "{} item '{}' uses undeclared stratum '{}'; declare it in pack.json \
                         before running the model",
                        fixture.name, id, stratum
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Load fixtures and enforce the pack-wide identity registry. Retired
/// ids are tombstones: a fixture may disappear, but no later fixture can
/// make an old baseline's key mean something new.
pub fn fixtures_at_with_retired(
    dir: &Path,
    retired_item_ids: &[String],
) -> Result<Vec<Fixture>, String> {
    fixtures_at_with_roles(dir, retired_item_ids, &[])
}

/// As above, binding each fixture's documents to the roles the pack
/// declares (#354).
///
/// Two kinds of fixture are discovered, and the first is every fixture
/// written before comparison packs existed:
///
/// - **A document with expectations beside it**, named for it. Bound to
///   the pack's sole role, which is the binding `run_pack` made for
///   itself. Its name and its digest are unchanged, deliberately: both
///   are recorded in baselines.
/// - **An `expected.json` naming its own `inputs`**, one file per role.
///   The fixture is named after the expectations, because no one of its
///   documents is the fixture.
///
/// A document with no expectations is still skipped rather than
/// refused — it is a perfectly good fixture to run by hand, it just has
/// no answers to be marked against.
pub fn fixtures_at_with_roles(
    dir: &Path,
    retired_item_ids: &[String],
    roles: &[InputSpec],
) -> Result<Vec<Fixture>, String> {
    let mut fixtures = multi_document_fixtures(dir, roles)?;
    // Documents already claimed by a multi-document fixture must not
    // also be discovered as fixtures of their own: they would be run
    // alone, against expectations describing a comparison, and score as
    // a run that found nothing.
    let claimed: BTreeSet<PathBuf> = fixtures
        .iter()
        .flat_map(|fixture| fixture.documents())
        .collect();

    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("couldn't read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            // Every input a preprocess builtin can read: statements
            // (`statement-parse`) and documents (`document-text`). A
            // fixture the runner could run but the harness would not
            // discover is a bed that silently scores less than it
            // holds (#279).
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| {
                    matches!(
                        extension.as_str(),
                        "csv"
                            | "pdf"
                            | "txt"
                            | "md"
                            | "markdown"
                            | "jpg"
                            | "jpeg"
                            | "heic"
                            | "heif"
                    )
                })
        })
        .collect();
    entries.sort();

    for path in entries {
        if claimed.contains(&path) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let expectations = path.with_extension("expected.json");
        if !expectations.exists() {
            continue;
        }
        let expected = read_expectations(&expectations)?;
        validate_term_roles(&expected, &expectations, roles)?;
        // The sole-role binding, made explicit where the manifest is
        // known. A pack declaring several roles has no unambiguous
        // binding for a lone document, so the fixture keeps none and
        // the run refuses it by name (`RoleUnstated`) rather than
        // guessing which document it is.
        let inputs = match roles {
            [only] => vec![(only.role.clone(), path.clone())],
            _ => Vec::new(),
        };
        fixtures.push(Fixture {
            name,
            path,
            inputs,
            expectations,
            expected,
        });
    }
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    validate_pack_item_ids(&fixtures, retired_item_ids)?;
    Ok(fixtures)
}

/// Fixtures whose `expected.json` names its own documents by role.
fn multi_document_fixtures(dir: &Path, roles: &[InputSpec]) -> Result<Vec<Fixture>, String> {
    let mut expectation_files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("couldn't read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".expected.json"))
        })
        .collect();
    expectation_files.sort();

    let mut fixtures = Vec::new();
    for expectations in expectation_files {
        let expected = read_expectations(&expectations)?;
        if expected.inputs.is_empty() {
            continue; // a document's own expectations, handled above
        }
        let named = |role: &str| -> Result<(), String> {
            if roles.iter().any(|declared| declared.role == role) {
                return Ok(());
            }
            Err(format!(
                "{} is supplied as “{role}”, which this pack has nothing called",
                expectations.display()
            ))
        };
        // Bound in the pack's declared order, so the earlier document
        // of a comparison is the one the manifest says it is (#350) and
        // never the order a directory listing happened to produce.
        let mut inputs = Vec::new();
        for role in expected.inputs.keys() {
            named(role)?;
        }
        for declared in roles {
            let Some(file) = expected.inputs.get(&declared.role) else {
                continue;
            };
            let path = dir.join(file);
            if !path.exists() {
                return Err(format!(
                    "{} names {file}, which is not in {}",
                    expectations.display(),
                    dir.display()
                ));
            }
            inputs.push((declared.role.clone(), path));
        }
        validate_term_roles(&expected, &expectations, roles)?;
        let name = expectations
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .trim_end_matches(".expected.json")
            .to_owned();
        let path = inputs
            .first()
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| expectations.clone());
        fixtures.push(Fixture {
            name,
            path,
            inputs,
            expectations,
            expected,
        });
    }
    Ok(fixtures)
}

/// Every term expectation must say which document it is about, in a
/// role the pack declares (#356).
///
/// Refused here rather than scored: an expectation naming a role that
/// was never bound matches nothing, so every item under it is a miss —
/// and a bed typo reads in the table as a model that could not read a
/// document at all.
fn validate_term_roles(
    expected: &Expected,
    expectations: &Path,
    roles: &[InputSpec],
) -> Result<(), String> {
    if roles.is_empty() {
        return Ok(()); // no manifest to check against
    }
    for want in &expected.policy_terms {
        if !roles.iter().any(|declared| declared.role == want.role) {
            return Err(format!(
                "{} expects '{}' from “{}”, which this pack has nothing called",
                expectations.display(),
                want.id,
                want.role
            ));
        }
    }
    Ok(())
}

fn read_expectations(path: &Path) -> Result<Expected, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
    let expected: Expected = serde_json::from_str(&raw)
        .map_err(|e| format!("{} isn't valid expectations: {e}", path.display()))?;
    expected
        .validate()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(expected)
}

fn validate_pack_item_ids(fixtures: &[Fixture], retired_item_ids: &[String]) -> Result<(), String> {
    let retired: BTreeSet<&str> = retired_item_ids.iter().map(String::as_str).collect();
    let mut fixture_ids: BTreeMap<&str, &str> = BTreeMap::new();
    let mut item_ids: BTreeMap<&str, &str> = BTreeMap::new();

    for fixture in fixtures {
        if !fixture.expected.classify.is_empty() {
            if let Some(previous) = fixture_ids.insert(&fixture.expected.fixture_id, &fixture.name)
            {
                return Err(format!(
                    "fixture id '{}' is used by both {} and {}",
                    fixture.expected.fixture_id, previous, fixture.name
                ));
            }
        }
        for item in &fixture.expected.classify {
            if retired.contains(item.id.as_str()) {
                return Err(format!(
                    "{} reuses retired classification item id '{}'; retired ids are \
                     burned permanently",
                    fixture.name, item.id
                ));
            }
            if let Some(previous) = item_ids.insert(&item.id, &fixture.name) {
                return Err(format!(
                    "classification item id '{}' is used by both {} and {}",
                    item.id, previous, fixture.name
                ));
            }
        }
    }
    Ok(())
}

/// Version the complete asking surface of one role, not only the
/// prose: examples and the constrained response schema can change an
/// answer too. Length-prefix each part so concatenation cannot make
/// distinct triples hash alike.
fn role_prompt_version(pack: &Pack, wanted: &str) -> Result<String, String> {
    let Some(crate::packs::PipelineStep::Model {
        prompt,
        schema: Some(schema),
        examples,
        ..
    }) = pack.manifest.pipeline.iter().find(|step| {
        matches!(
            step,
            crate::packs::PipelineStep::Model {
                role: Some(role),
                ..
            } if role == wanted
        )
    })
    else {
        return Err(format!(
            "{} has {wanted} expectations but no {wanted} model step",
            pack.manifest.id
        ));
    };

    let mut hasher = blake3::Hasher::new();
    for (label, relative) in [
        ("prompt", Some(prompt.as_str())),
        ("examples", examples.as_deref()),
        ("schema", Some(schema.as_str())),
    ] {
        hasher.update(label.as_bytes());
        match relative {
            Some(relative) => {
                let bytes = std::fs::read(pack.dir.join(relative))
                    .map_err(|e| format!("couldn't read {relative} for prompt provenance: {e}"))?;
                hasher.update(&(bytes.len() as u64).to_le_bytes());
                hasher.update(&bytes);
            }
            None => {
                hasher.update(&0_u64.to_le_bytes());
            }
        };
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

/// What a `.gguf` file name says about the weights inside it.
///
/// Read from the name rather than the file's own metadata on purpose:
/// the name is what a person typed at `--model`, what the table has to
/// print, and what they will compare against a tier list. Reading the
/// header would be more authoritative and would name a model the person
/// cannot match to the thing they asked for.
///
/// A name that says nothing claims nothing — [`UNKNOWN`] rather than a
/// guess. An invented "7B" sitting beside real scores is a tier claim
/// nobody can check, which is the one thing an eval must not produce.
pub fn model_info(file: &str, context: u32) -> ModelInfo {
    let stem = file.trim_end_matches(".gguf").to_lowercase();
    let parts: Vec<&str> = stem.split(['-', '.', '_']).collect();

    ModelInfo {
        file: file.to_owned(),
        params: parameter_count(&parts).unwrap_or_else(|| UNKNOWN.to_owned()),
        quant: quantisation(&stem).unwrap_or_else(|| UNKNOWN.to_owned()),
        context,
    }
}

/// What is honestly recorded when a file name doesn't say.
const UNKNOWN: &str = "unknown";

/// The `7b` in `qwen2.5-7b-instruct`: a number followed by b or m, on
/// its own. Spelled back out in the capital everyone writes it in.
fn parameter_count(parts: &[&str]) -> Option<String> {
    parts
        .iter()
        .find(|part| {
            let (digits, unit) = part.split_at(part.len().saturating_sub(1));
            matches!(unit, "b" | "m")
                && !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(|part| part.to_uppercase())
}

/// The `q4_k_m` in `qwen2.5-7b-instruct-q4_k_m`. Matched on the whole
/// stem rather than a part, because the quantisation is the one field
/// that is itself full of separators.
///
/// A `q` only starts one if a digit follows it — otherwise every model
/// whose name merely begins with the letter (`qwen`) would report its
/// own name as its quantisation.
fn quantisation(stem: &str) -> Option<String> {
    let at = stem
        .char_indices()
        .filter(|&(_, c)| c == 'q')
        .map(|(at, _)| at)
        .find(|&at| stem[at + 1..].starts_with(|c: char| c.is_ascii_digit()))?;

    let rest = &stem[at..];
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(rest[..end].to_uppercase())
}

/// Score one finished run against what the fixture said it should find.
pub fn score_fixture(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    perf: Perf,
) -> FixtureResult {
    score_fixture_with_provenance(
        fixture,
        expected,
        outcome,
        Some(perf),
        0,
        &ItemProvenance {
            pack: "unversioned-pack",
            pack_version: "unversioned",
            prompt_version: "unversioned",
            exchanges: &[],
            evidence: &BTreeMap::new(),
            sources: &[],
            gated_strata: &[],
        },
    )
}

struct ItemProvenance<'a> {
    pack: &'a str,
    pack_version: &'a str,
    prompt_version: &'a str,
    exchanges: &'a [CapturedExchange],
    /// The evidence dimensions the pack declared it can score (#430).
    evidence:
        &'a BTreeMap<super::evidence::EvidenceDimension, super::evidence::EvidenceDeclaration>,
    /// Each source document's text, for existence checks. Empty when
    /// the caller has no readable text to offer.
    sources: &'a [String],
    /// The declared strata that carry ceilings, for tagging synthetic
    /// items so pooled gates read them (#442).
    gated_strata: &'a [String],
}

fn score_fixture_with_provenance(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    perf: Option<Perf>,
    retries: u32,
    provenance: &ItemProvenance<'_>,
) -> FixtureResult {
    let items = match &outcome.payload {
        Payload::Audit(_) => classification_items(fixture, expected, outcome, provenance),
        Payload::Extraction(extraction) => {
            extraction_items(fixture, expected, outcome, extraction, provenance)
        }
        Payload::Comparison(comparison) => {
            comparison_items(fixture, expected, outcome, comparison, provenance)
        }
    };

    // A step the fixture says nothing about is not scored at all. A
    // vacuous 1.0 would sail past the pack's bar and report a model
    // nobody measured as good enough.
    // Each typology is scored in its own vocabulary — merchants, series
    // and cadences for an audit, obligations for a letter. Matching the
    // payload is what stops a letter run being scored as though it had
    // failed to find any subscriptions (#238, #240).
    let (step_scores, end_to_end, needs_review_rate) = match &outcome.payload {
        Payload::Audit(audit) => {
            let mut step_scores = BTreeMap::new();
            if !expected.normalise.is_empty() {
                step_scores.insert("normalise".to_owned(), score_normalise(expected, audit));
            }
            (
                step_scores,
                score_end_to_end(expected, audit),
                score_needs_review_rate(expected, outcome, audit),
            )
        }
        Payload::Extraction(extraction) => {
            let mut step_scores = BTreeMap::new();
            if !expected.obligations.is_empty() {
                step_scores.insert(
                    "obligations".to_owned(),
                    score_obligations(expected, extraction),
                );
            }
            (
                step_scores,
                score_extraction_end_to_end(expected, extraction),
                score_extraction_review_rate(outcome),
            )
        }
        // The comparison typology is scored on its *extraction* (#356).
        // The diff itself is deterministic Rust over those terms
        // (#350), so a bed for it would score `rust_decimal` and a
        // `BTreeMap` — and a bed that cannot disagree measures nothing.
        Payload::Comparison(comparison) => {
            let mut step_scores = BTreeMap::new();
            if !expected.policy_terms.is_empty() {
                step_scores.insert(
                    "policy-terms".to_owned(),
                    score_policy_terms(expected, outcome, comparison),
                );
            }
            (
                step_scores,
                score_comparison_end_to_end(expected, outcome, comparison),
                score_extraction_review_rate(outcome),
            )
        }
    };

    let containment = containment_metrics(&items, &outcome.claim_traces);
    FixtureResult {
        fixture: fixture.to_owned(),
        step_scores,
        items,
        containment,
        end_to_end,
        needs_review_rate,
        retries,
        perf,
        // Stability is a property of a *set* of runs, so it cannot be
        // known here: one run scored once. `--runs` fills it in from
        // the repeats (#83).
        stability: None,
    }
}

fn containment_metrics(
    items: &[ScoredItem],
    traces: &[crate::claim_trace::ClaimTrace],
) -> ContainmentMetrics {
    use crate::claim_trace::{CheckOutcome, Guardrail, TerminalDisposition};

    let mut metrics = ContainmentMetrics {
        candidates: traces.len(),
        ..ContainmentMetrics::default()
    };
    for trace in traces {
        match trace.terminal {
            TerminalDisposition::Accepted => metrics.accepted += 1,
            TerminalDisposition::NeedsReview => metrics.needs_review += 1,
            TerminalDisposition::Rejected => metrics.rejected += 1,
            TerminalDisposition::Deduplicated => metrics.deduplicated += 1,
            TerminalDisposition::AbsentAfterRetry => metrics.absent_after_retry += 1,
            TerminalDisposition::PipelineIntroducedError => metrics.pipeline_introduced += 1,
        }
        for check in &trace.checks {
            let boundary = metrics.by_guardrail.entry(check.guardrail).or_default();
            match check.outcome {
                CheckOutcome::Passed => boundary.passed += 1,
                CheckOutcome::Failed => boundary.failed += 1,
                CheckOutcome::NotApplicable => {}
                // A changed value is the stage's act, not the
                // candidate crossing or failing a boundary — counting
                // it as `failed` here would book a pipeline fault as
                // containment of a model error, the exact collapse
                // #470 exists to prevent. The trace-level
                // `pipeline_introduced` column carries the tally.
                CheckOutcome::ChangedValue => {}
                // A warning contains nothing — the candidate went
                // through (#460 rule 2). Booking it as `failed` would
                // claim a catch that never happened; the trace carries
                // the caution for anything that wants to read it.
                CheckOutcome::Warned => {}
            }
        }
    }

    let by_id: BTreeMap<&str, &crate::claim_trace::ClaimTrace> = traces
        .iter()
        .map(|trace| (trace.id.as_str(), trace))
        .collect();
    for item in items {
        let linked: Vec<&crate::claim_trace::ClaimTrace> = item
            .trace_ids
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect();
        if scored_needs_review(&item.decision) {
            metrics.contained += 1;
            let mut guards: BTreeSet<Guardrail> = linked
                .iter()
                .flat_map(|trace| &trace.checks)
                .filter(|check| check.outcome == CheckOutcome::Failed)
                .map(|check| check.guardrail)
                .collect();
            // Honest low confidence reaches review without failing a
            // semantic check; review routing is itself the boundary.
            if guards.is_empty() {
                guards.insert(Guardrail::ReviewRouting);
            }
            for guardrail in guards {
                metrics.by_guardrail.entry(guardrail).or_default().contained += 1;
            }
        } else if scored_assertion_is_wrong(&item.decision) {
            metrics.escaped += 1;
            let guards: BTreeSet<Guardrail> = linked
                .iter()
                .flat_map(|trace| &trace.checks)
                .filter(|check| check.outcome == CheckOutcome::Passed)
                .map(|check| check.guardrail)
                .collect();
            for guardrail in guards {
                metrics.by_guardrail.entry(guardrail).or_default().escaped += 1;
            }
        }
    }
    metrics
}

fn scored_needs_review(decision: &ScoredDecision) -> bool {
    matches!(
        decision,
        ScoredDecision::Classification {
            actual: ClassificationOutcome::NeedsReview { .. },
            ..
        } | ScoredDecision::Extraction {
            actual: ExtractionOutcome::NeedsReview { .. },
            ..
        }
    )
}

pub(super) fn scored_assertion_is_wrong(decision: &ScoredDecision) -> bool {
    match decision {
        ScoredDecision::Classification {
            expected,
            actual: ClassificationOutcome::Classified { classification },
            ..
        } => expected != classification,
        ScoredDecision::Classification {
            actual: ClassificationOutcome::NeedsReview { .. },
            ..
        } => false,
        ScoredDecision::Extraction {
            expected, actual, ..
        } => match actual {
            ExtractionOutcome::Found { extracted } => match expected {
                Some(want) => !want.same_assertion_as(extracted),
                None => true,
            },
            ExtractionOutcome::Absent => expected.is_some(),
            ExtractionOutcome::NeedsReview { .. } => false,
        },
    }
}

/// One decision per passage of a comparison (#356), read the way the
/// obligations lens reads a letter: a passage routed to review is
/// neither a hit nor a miss, a term quoted from the passage is a find,
/// and anything else is the run having read the passage and asserted
/// nothing from it.
///
/// A term is matched to a passage by **both** the document it came from
/// and the passage the run read it out of. Document alone would pair the
/// two years' identically-worded lines; the passage alone would let last
/// year's reading satisfy this year's expectation, which is the failure
/// that turns a rise into a cut.
///
/// # Not by the quote (#457)
///
/// This used to ask whether the expectation's segment *contained* the
/// term's quote, and take the first term that did. A quote is evidence
/// that a value is on the page; it was never an identifier of where.
/// The renewal bed's commercial schedules state `Excess` under three
/// cover sections, the model quoted the bare label, and the label is
/// verbatim in all three — so sections two and three were both scored
/// against section one's reading, and a run that read every value
/// correctly was recorded as confidently wrong sixteen times.
///
/// The run already knows which passage each term came from: the model is
/// asked one passage at a time, and [`crate::terms::Term::segment`]
/// carries it. Re-deriving it here was the defect (#361), and any join
/// that infers the source from the claim's own text can be broken by a
/// weaker claim. So a term whose passage this is not does not join, and
/// the honest outcome for an expectation nothing was read from is
/// `Absent`.
fn comparison_items(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    comparison: &crate::run::ComparisonOutcome,
    provenance: &ItemProvenance<'_>,
) -> Vec<ScoredItem> {
    let mut items: Vec<ScoredItem> = expected
        .policy_terms
        .iter()
        .map(|want| {
            let document = role_index(outcome, &want.role);
            let from_this_passage = |term: &crate::terms::Term| {
                Some(term.document) == document && same_passage(&term.segment, &want.segment)
            };
            let joined = comparison.terms.iter().find(|t| from_this_passage(t));
            // A referral expectation is also satisfied by Rust's own
            // refusal (#461): a reading `diff_terms` declined to compare
            // surfaced to a person just as a model refusal does, and the
            // run records which — joined structurally, `(term, basis)`
            // from `not_compared` to the segment the run's own term
            // carries, never re-derived from quote text (#457).
            let rust_referred = if want.review {
                comparison.not_compared.iter().find(|refused| {
                    comparison.terms.iter().any(|term| {
                        from_this_passage(term)
                            && term.term == refused.term
                            && term.basis == refused.basis
                    })
                })
            } else {
                None
            };
            let actual = if let Some(review) = outcome
                .needs_review
                .iter()
                .find(|item| same_passage(&item.subject, &want.segment))
            {
                ExtractionOutcome::NeedsReview {
                    reason: review.reason.clone(),
                }
            } else if let Some(refused) = rust_referred {
                ExtractionOutcome::NeedsReview {
                    reason: crate::comparison_report::not_compared_reason(
                        &refused.term,
                        refused.readings,
                        &refused.why,
                    ),
                }
            } else if let Some(found) = joined {
                ExtractionOutcome::Found {
                    extracted: Extracted::Term(ExpectedTerm {
                        term: found.term.clone(),
                        basis: found.basis.clone(),
                        value: found.value.clone(),
                        quote: found.quote.clone(),
                    }),
                }
            } else {
                ExtractionOutcome::Absent
            };

            // The declared evidence questions, answered for the claim
            // as asserted (#430). Only an asserted claim has evidence
            // to examine.
            let evidence = match (&actual, joined) {
                (ExtractionOutcome::Found { .. }, Some(found)) => super::evidence::term_evidence(
                    provenance.evidence,
                    want,
                    found,
                    provenance.sources,
                ),
                _ => BTreeMap::new(),
            };

            let exchanges = provenance
                .exchanges
                .iter()
                .filter(|exchange| {
                    exchange
                        .items
                        .iter()
                        .any(|(_, source)| same_passage(source, &want.segment))
                })
                .map(|exchange| exchange.exchange.clone())
                .collect();

            ScoredItem {
                id: format!("{}/{}/{}", provenance.pack, expected.fixture_id, want.id),
                item_id: want.id.clone(),
                pack: provenance.pack.to_owned(),
                pack_version: provenance.pack_version.to_owned(),
                prompt_version: provenance.prompt_version.to_owned(),
                fixture: fixture.to_owned(),
                fixture_id: expected.fixture_id.clone(),
                strata: want.strata.clone(),
                raw_input: want.segment.clone(),
                // One passage of one document is one decision (#310).
                // The role is in the key because the two documents of a
                // renewal repeat each other's wording by construction —
                // pooling them would count one answer as two trials and
                // narrow every interval a ceiling is judged on.
                decision_key: format!("{}|{}", want.role, passage_key(&want.segment)),
                decision: ScoredDecision::Extraction {
                    expected: want.expect.clone().map(Extracted::Term),
                    expected_review: want.review,
                    unauthored_negative: false,
                    actual,
                },
                evidence,
                trace_ids: passage_trace_ids(outcome, &want.segment),
                confidence: declared_confidence(outcome, &want.segment),
                exchanges,
            }
        })
        .collect();

    // A passage the model read and answered with nothing is a decision
    // too (#429), whichever shape the lens extracts.
    items.extend(answered_nothing_items(
        fixture,
        expected,
        outcome,
        "terms",
        |source| {
            expected
                .policy_terms
                .iter()
                .any(|want| same_passage(source, &want.segment))
        },
        provenance,
    ));
    items
}

/// Which document a role was supplied as, by position in the run's
/// bound inputs (#332) — the same index `Segment::document` counts in.
/// `None` for a role this run never bound, which makes every
/// expectation against it a miss rather than a silent pass.
fn role_index(outcome: &RunOutcome, role: &str) -> Option<usize> {
    outcome.inputs.iter().position(|input| input.role == role)
}

/// Is this quote in that passage? Whitespace-insensitive, as the run's
/// own verification is (`run::quote_is_in`): a PDF's line breaks belong
/// to the page, not the sentence.
pub(crate) fn same_passage_contains(passage: &str, quote: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    !quote.trim().is_empty() && squash(passage).contains(&squash(quote))
}

/// Did the run read each named value the documents state? Joined on the
/// whole term — which value, on what basis, out of which document — all
/// exact, because the value is copied verbatim and an exact join is
/// what makes a misread currency symbol a wrong answer rather than a
/// rounding one.
fn score_policy_terms(
    expected: &Expected,
    outcome: &RunOutcome,
    comparison: &crate::run::ComparisonOutcome,
) -> StepScore {
    let want = term_keys_expected(expected, outcome);
    let got = term_keys_read(expected, outcome, comparison);
    let want_refs: Vec<(&str, &str)> = want.iter().map(|k| (k.as_str(), "")).collect();
    let got_refs: Vec<(&str, &str)> = got.iter().map(|k| (k.as_str(), "")).collect();
    scored(&want_refs, &got_refs, Tolerance::Exact)
}

/// The Comparison end result, F1 as the others are: a value missed and
/// a value invented both cost, because both are a number a person
/// either never sees or acts on wrongly.
fn score_comparison_end_to_end(
    expected: &Expected,
    outcome: &RunOutcome,
    comparison: &crate::run::ComparisonOutcome,
) -> f32 {
    let want = term_keys_expected(expected, outcome);
    let got = term_keys_read(expected, outcome, comparison);
    set_f1(&borrowed(&want), &borrowed(&got))
}

fn term_keys_expected(expected: &Expected, outcome: &RunOutcome) -> Vec<String> {
    expected
        .policy_terms
        .iter()
        .filter_map(|want| {
            // An expected referral's key is the referral itself (#445):
            // the correct reading routes this passage to a person, and
            // the read side mirrors the key from `needs_review`.
            if want.review {
                return Some(format!("review|{}", passage_key(&want.segment)));
            }
            let term = want.expect.as_ref()?;
            Some(term_key(
                role_index(outcome, &want.role),
                &term.term,
                &term.basis,
                &term.value,
            ))
        })
        .collect()
}

fn term_keys_read(
    expected: &Expected,
    outcome: &RunOutcome,
    comparison: &crate::run::ComparisonOutcome,
) -> Vec<String> {
    let mut keys: Vec<String> = comparison
        .terms
        .iter()
        .map(|term| term_key(Some(term.document), &term.term, &term.basis, &term.value))
        .collect();
    // A review-routed passage the bed expected to be referred was read
    // correctly (#445); its key joins the expected side's. The model's
    // refusal and Rust's are different facts kept in different lists
    // (`needs_review` and `not_compared`), and either satisfies a
    // referral expectation (#461) — the Rust side joined structurally,
    // `(term, basis)` to the segment the run's own term carries, never
    // re-derived from quote text (#457).
    for want in expected.policy_terms.iter().filter(|want| want.review) {
        let document = role_index(outcome, &want.role);
        let surfaced = outcome
            .needs_review
            .iter()
            .any(|item| same_passage(&item.subject, &want.segment))
            || comparison.not_compared.iter().any(|refused| {
                comparison.terms.iter().any(|term| {
                    Some(term.document) == document
                        && same_passage(&term.segment, &want.segment)
                        && term.term == refused.term
                        && term.basis == refused.basis
                })
            });
        if surfaced {
            keys.push(format!("review|{}", passage_key(&want.segment)));
        }
    }
    keys
}

/// One term's identity for scoring: which document, which term, on what
/// basis, saying what. The document is in the key because a renewal
/// diff's whole content is which year a value came out of.
fn term_key(document: Option<usize>, term: &str, basis: &str, value: &str) -> String {
    let which = match document {
        Some(index) => index.to_string(),
        None => "unbound".to_owned(),
    };
    format!("{which}|{term}|{basis}|{value}")
}

/// Did the run extract each obligation the document carries? Joined on
/// [`super::ObligationIdentity`] (#554) — the same kind of ask, on the
/// same party, by the same day arrived at the same way; in the letter's
/// own words where no day resolved. The harm lens and the evidence
/// `support` dimension read the same identity, so a found obligation is
/// a correct one and a supported one, and a wrong one is none of the
/// three.
fn score_obligations(expected: &Expected, extraction: &crate::run::ExtractionOutcome) -> StepScore {
    let want: Vec<(String, String)> = expected
        .obligations
        .iter()
        .filter_map(|o| o.expect.as_ref())
        .map(|o| (o.identity().key(), String::new()))
        .collect();
    let got: Vec<(String, String)> = extraction
        .obligations
        .iter()
        .map(|o| (o.identity().key(), String::new()))
        .collect();
    let want_refs: Vec<(&str, &str)> = want.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let got_refs: Vec<(&str, &str)> = got.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    scored(&want_refs, &got_refs, Tolerance::Exact)
}

/// The Extraction end result, F1 as the Audit's is: a missed obligation
/// and an invented one both cost, because both are a deadline a person
/// either loses or chases for nothing.
fn score_extraction_end_to_end(
    expected: &Expected,
    extraction: &crate::run::ExtractionOutcome,
) -> f32 {
    let want: Vec<String> = expected
        .obligations
        .iter()
        .filter_map(|o| o.expect.as_ref())
        .map(|o| o.identity().key())
        .collect();
    let got: Vec<String> = extraction
        .obligations
        .iter()
        .map(|o| o.identity().key())
        .collect();
    set_f1(&borrowed(&want), &borrowed(&got))
}

/// How much of the document ended up in front of a person, counted in
/// segments — a passage is what a person actually has to read. The
/// denominator is every segment the run read (`input.rows` is the
/// typology's own unit), and the numerator is the ones routed to
/// review **plus the low-confidence findings**: a reading kept at low
/// confidence lands in "check these yourself", which is exactly the
/// same ask of a person as a review item. The mutation harness found
/// the undercount (#426): forcing every confidence low changed what a
/// person must read and this measure saw nothing move.
fn score_extraction_review_rate(outcome: &RunOutcome) -> f32 {
    if outcome.input.rows == 0 {
        return 0.0;
    }
    let low_confidence = match &outcome.payload {
        Payload::Extraction(extraction) => extraction
            .obligations
            .iter()
            .filter(|obligation| obligation.confidence == "low")
            .count(),
        Payload::Comparison(comparison) => comparison
            .terms
            .iter()
            .filter(|term| term.confidence == "low")
            .count(),
        Payload::Audit(_) => 0,
    };
    (outcome.needs_review.len() + low_confidence) as f32 / outcome.input.rows as f32
}

/// Did the model turn each raw merchant into the name the fixture asks
/// for? Joined on the raw string the statement carried, which every
/// answered group keeps hold of all the way to the outcome.
fn score_normalise(expected: &Expected, outcome: &AuditOutcome) -> StepScore {
    // One real-world merchant is one naming decision, however many
    // processor descriptors it wore (#253): the pipeline groups the
    // variants and answers once, so scoring per descriptor would
    // charge that one answer several times over — or, joined on raw
    // equality, mark the sibling descriptors as unanswered. Both sides
    // key on the canonical name.
    let mut seen = BTreeSet::new();
    let want: Vec<(&str, &str)> = expected
        .normalise
        .iter()
        .filter(|e| seen.insert(e.name.to_ascii_lowercase()))
        .map(|e| (e.name.as_str(), e.name.as_str()))
        .collect();
    let got: Vec<(&str, &str)> = named_merchants(outcome)
        .into_iter()
        .map(|(raw, name)| (join_key(expected, raw), name))
        .collect();
    scored(&want, &got, tolerance(expected, "normalise"))
}

fn classification_items(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    provenance: &ItemProvenance<'_>,
) -> Vec<ScoredItem> {
    // Per-item records are the classification metric's (#237); an
    // extraction run has none until a pack declares an item metric for
    // obligations — absent, not zero.
    let Payload::Audit(audit) = &outcome.payload else {
        return Vec::new();
    };
    expected
        .classify
        .iter()
        .map(|want| {
            let raw = want
                .raw
                .as_deref()
                .unwrap_or_else(|| raw_merchant_for(expected, &want.name));
            // The pipeline's outcome is the decision being scored
            // (#253): kind derived from cadence, category from the
            // model. The raw exchange is a fallback for review-routed
            // items, where a retained proposal is worth inspecting.
            let pipeline = run_classification(expected, audit, raw);
            let review = outcome
                .needs_review
                .iter()
                .find(|item| same_merchant(expected, raw, &item.subject));
            // The branch the pipeline took, kept whatever the outcome:
            // a review-routed item still had a kind derived, and "which
            // decision surfaced this" is as much a question as "which
            // decision got it wrong" (#272).
            let kind_from = pipeline.as_ref().map(|(_, _, path)| *path);
            let actual = match (review, pipeline) {
                // Review-routed, but keep whatever proposal exists for
                // diagnosis: the raw exchange's if one touched this
                // item, else the pipeline's own low-confidence finding.
                (Some(review), pipeline) => ClassificationOutcome::NeedsReview {
                    proposed: captured_classification(provenance.exchanges, raw)
                        .or(pipeline.map(|(classification, _, _)| classification)),
                    reason: review.reason.clone(),
                },
                // Low confidence is what the screen shows in "check
                // these yourself": surfaced for a person, asserted by
                // nobody. Scored as an assertion it would make the
                // no-model floor read confidently wrong about every
                // merchant — the exact opposite of what it does.
                (None, Some((classification, confidence, _))) if confidence == LOW_CONFIDENCE => {
                    ClassificationOutcome::NeedsReview {
                        proposed: Some(classification),
                        reason: "Kettle wasn't sure, so this one is shown in \
                                 \"check these yourself\"."
                            .to_owned(),
                    }
                }
                (None, Some((classification, _, _))) => {
                    ClassificationOutcome::Classified { classification }
                }
                (None, None) => ClassificationOutcome::NeedsReview {
                    proposed: None,
                    reason: "Kettle produced no classification for this scored item.".to_owned(),
                },
            };
            let exchanges = provenance
                .exchanges
                .iter()
                .filter(|exchange| {
                    exchange.items.iter().any(|(_, source)| {
                        source.eq_ignore_ascii_case(raw)
                            || canonical(expected, raw)
                                .is_some_and(|name| name.eq_ignore_ascii_case(source))
                    })
                })
                .map(|exchange| exchange.exchange.clone())
                .collect();

            ScoredItem {
                id: format!("{}/{}/{}", provenance.pack, expected.fixture_id, want.id),
                item_id: want.id.clone(),
                pack: provenance.pack.to_owned(),
                pack_version: provenance.pack_version.to_owned(),
                prompt_version: provenance.prompt_version.to_owned(),
                fixture: fixture.to_owned(),
                fixture_id: expected.fixture_id.clone(),
                strata: want.strata.clone(),
                raw_input: raw.to_owned(),
                // What was decided is *this merchant*, not this
                // descriptor (#310). `STRIPE* BACKBLAZE` and
                // `STRIPE* BACKBLAZE CARD 4821` are one piece of
                // merchant knowledge, and the 7B was wrong about both
                // forms — counting them twice would let a ceiling clear
                // on the same mistake made twice.
                decision_key: join_key(expected, raw).to_owned(),
                decision: ScoredDecision::Classification {
                    expected: Classification {
                        kind: want.kind.clone(),
                        category: want.category.clone(),
                    },
                    actual,
                    kind_from,
                },
                evidence: BTreeMap::new(),
                trace_ids: merchant_trace_ids(expected, outcome, raw),
                // The split #271 was waiting for, and it turned out to
                // be one question rather than two levels: the model
                // answers `category` and nothing else (#253), because
                // `kind` moved into Rust when the Stage 3 bed showed 92
                // of 96 confident-wrong answers sitting in the cadence
                // gap. So the declared level is a claim about the
                // category, taken from the `Classification` trace and
                // never from the `Normalisation` one beside it — a
                // name-recognition confidence is a third question
                // again, and borrowing it is the defect, not the fix.
                confidence: declared_category_confidence(expected, outcome, raw),
                exchanges,
            }
        })
        .collect()
}

/// One scored record per authored extraction decision (#279).
///
/// The actual outcome is read per *passage*, because that is the unit
/// the model was asked about and the unit a person reads: a segment
/// routed to review is neither a hit nor a miss, an obligation whose
/// evidence includes the passage is a find, and anything else is the
/// run having read the passage and asserted nothing.
fn extraction_items(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    extraction: &crate::run::ExtractionOutcome,
    provenance: &ItemProvenance<'_>,
) -> Vec<ScoredItem> {
    let mut items: Vec<ScoredItem> = expected
        .obligations
        .iter()
        .map(|want| {
            let from_this_passage = |o: &crate::run::Obligation| {
                o.evidence
                    .iter()
                    .any(|segment| same_passage(&segment.text, &want.segment))
            };
            let joined = extraction.obligations.iter().find(|o| from_this_passage(o));
            let actual = if let Some(review) = outcome
                .needs_review
                .iter()
                .find(|item| same_passage(&item.subject, &want.segment))
            {
                ExtractionOutcome::NeedsReview {
                    reason: review.reason.clone(),
                }
            } else if let Some(found) = joined {
                ExtractionOutcome::Found {
                    // The letter typology's payload, named as one of
                    // the shapes the lens can carry (#351) rather than
                    // as the only one.
                    extracted: Extracted::Obligation(ExpectedObligation::from(found)),
                }
            } else {
                ExtractionOutcome::Absent
            };

            // The declared evidence questions, answered for the claim
            // as asserted (#430). Only an asserted claim has evidence
            // to examine; an absent or review-routed decision leaves
            // every dimension unanswered rather than vacuously passed.
            let evidence = match (&actual, joined) {
                (ExtractionOutcome::Found { .. }, Some(found)) => {
                    super::evidence::obligation_evidence(
                        provenance.evidence,
                        want,
                        found,
                        provenance.sources,
                    )
                }
                _ => BTreeMap::new(),
            };

            let exchanges = provenance
                .exchanges
                .iter()
                .filter(|exchange| {
                    exchange
                        .items
                        .iter()
                        .any(|(_, source)| same_passage(source, &want.segment))
                })
                .map(|exchange| exchange.exchange.clone())
                .collect();

            ScoredItem {
                id: format!("{}/{}/{}", provenance.pack, expected.fixture_id, want.id),
                item_id: want.id.clone(),
                pack: provenance.pack.to_owned(),
                pack_version: provenance.pack_version.to_owned(),
                prompt_version: provenance.prompt_version.to_owned(),
                fixture: fixture.to_owned(),
                fixture_id: expected.fixture_id.clone(),
                strata: want.strata.clone(),
                raw_input: want.segment.clone(),
                // One passage is one decision, however many letters
                // carry it (#310). The letter bed repeats phrasings by
                // construction: 415 no-obligation rows are 23 distinct
                // sentences, each answered identically every time.
                decision_key: passage_key(&want.segment),
                decision: ScoredDecision::Extraction {
                    expected: want.expect.clone().map(Extracted::Obligation),
                    expected_review: false,
                    unauthored_negative: false,
                    actual,
                },
                evidence,
                trace_ids: passage_trace_ids(outcome, &want.segment),
                confidence: declared_confidence(outcome, &want.segment),
                exchanges,
            }
        })
        .collect();

    // An assertion about a passage the bed never authored is an
    // invention by construction — the bed is synthetic, so nothing
    // unauthored is ever legitimately assertable — and it must remain
    // representable rather than vanishing between the raw answer and
    // the metrics (#442, #425). The mutation harness found 1,750 of
    // these invisible on the letter bed while a real run would have
    // carried every one into the report.
    for found in &extraction.obligations {
        let authored = expected.obligations.iter().any(|want| {
            found
                .evidence
                .iter()
                .any(|segment| same_passage(&segment.text, &want.segment))
        });
        if authored {
            continue;
        }
        let passage = found
            .evidence
            .first()
            .map(|segment| segment.text.clone())
            .unwrap_or_default();
        let key = passage_key(&passage);
        let item_id = format!("unauthored-{}", &blake3::hash(key.as_bytes()).to_hex()[..8]);
        items.push(ScoredItem {
            id: format!("{}/{}/{}", provenance.pack, expected.fixture_id, item_id),
            item_id,
            pack: provenance.pack.to_owned(),
            pack_version: provenance.pack_version.to_owned(),
            prompt_version: provenance.prompt_version.to_owned(),
            fixture: fixture.to_owned(),
            fixture_id: expected.fixture_id.clone(),
            // Tagged into every gated stratum, so the pooled ceilings
            // read the invention the way they read authored decisions.
            strata: provenance.gated_strata.to_vec(),
            raw_input: passage.clone(),
            decision_key: key,
            decision: ScoredDecision::Extraction {
                expected: None,
                expected_review: false,
                unauthored_negative: false,
                actual: ExtractionOutcome::Found {
                    extracted: Extracted::Obligation(ExpectedObligation::from(found)),
                },
            },
            evidence: BTreeMap::new(),
            trace_ids: passage_trace_ids(outcome, &passage),
            confidence: declared_confidence(outcome, &passage),
            exchanges: Vec::new(),
        });
    }

    // A passage the model read and answered with nothing is a decision
    // too (#429).
    items.extend(answered_nothing_items(
        fixture,
        expected,
        outcome,
        "obligations",
        |source| {
            expected
                .obligations
                .iter()
                .any(|want| same_passage(source, &want.segment))
        },
        provenance,
    ));
    items
}

/// The confidence declared on the answer(s) that read this passage
/// (#429), read from the run's own claim ledger — where each
/// decision-kind claim holds the validated answer beside the segment
/// it answered. That is the structural pairing #457 requires: the run
/// recorded which answer belongs to which passage, so nothing here is
/// re-derived from a claim's own text.
///
/// Several exchanges may have answered text-identical passages; at
/// temperature 0 they are the same question asked again (#310). If
/// every one declared the same level, that level is the decision's and
/// the first exchange in ledger order names it — nothing about the
/// confidence is guessed, because all of them carried it. Levels that
/// disagree cannot be traced to the answer that produced *this*
/// decision, so they are recorded untraceable rather than resolved by
/// picking one (#271).
///
/// `None` when no exchange declared one: the floor, a batch that
/// failed validation twice, or a record scored before answers carried
/// confidence at all. Absent means "not recorded", never a level.
fn declared_confidence(outcome: &RunOutcome, passage: &str) -> Option<super::DeclaredConfidence> {
    let declared: Vec<(&str, &str)> = outcome
        .claim_traces
        .iter()
        .filter(|trace| {
            trace.kind == crate::claim_trace::ClaimKind::Decision
                && same_passage(&trace.source, passage)
        })
        .filter_map(|trace| {
            trace
                .candidate
                .get("confidence")
                .and_then(serde_json::Value::as_str)
                .filter(|level| !level.is_empty())
                .map(|level| (level, trace.id.as_str()))
        })
        .collect();
    let (level, trace_id) = *declared.first()?;
    if declared.iter().all(|(seen, _)| *seen == level) {
        Some(super::DeclaredConfidence::Declared {
            level: level.to_owned(),
            trace_id: trace_id.to_owned(),
        })
    } else {
        Some(super::DeclaredConfidence::Untraceable {
            levels: declared
                .iter()
                .map(|(seen, _)| (*seen).to_owned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }
}

/// Scored records for the passages the model read and answered with
/// nothing, where the bed authored no expectation at all (#429).
///
/// Until #429 these decisions existed only in raw exchange text — 83%
/// of the letter run's answered passages are in this shape — so the
/// dominant decision the model makes was invisible to every scored
/// record, and the confidence it was made at could not be calibrated.
/// On a synthetic bed an unauthored passage genuinely asks nothing
/// (the same construction argument as #442's inventions), so each is a
/// correct negative: expected nothing, asserted nothing.
///
/// They carry no strata and are excluded from the extraction metrics —
/// see `unauthored_negative` on the decision — because every declared
/// gate was sized on authored decisions, and a record-shape change
/// must not move a score.
fn answered_nothing_items(
    fixture: &str,
    expected: &Expected,
    outcome: &RunOutcome,
    answers_key: &str,
    authored: impl Fn(&str) -> bool,
    provenance: &ItemProvenance<'_>,
) -> Vec<ScoredItem> {
    let mut seen = BTreeSet::new();
    outcome
        .claim_traces
        .iter()
        .filter(|trace| {
            trace.kind == crate::claim_trace::ClaimKind::Decision
                && trace.terminal == crate::claim_trace::TerminalDisposition::Accepted
                && trace
                    .candidate
                    .get(answers_key)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(Vec::is_empty)
                && !authored(&trace.source)
                && seen.insert(passage_key(&trace.source))
        })
        .map(|trace| {
            let key = passage_key(&trace.source);
            let item_id = format!(
                "answered-nothing-{}",
                &blake3::hash(key.as_bytes()).to_hex()[..8]
            );
            let exchanges = provenance
                .exchanges
                .iter()
                .filter(|exchange| {
                    exchange
                        .items
                        .iter()
                        .any(|(_, source)| same_passage(source, &trace.source))
                })
                .map(|exchange| exchange.exchange.clone())
                .collect();
            ScoredItem {
                id: format!("{}/{}/{}", provenance.pack, expected.fixture_id, item_id),
                item_id,
                pack: provenance.pack.to_owned(),
                pack_version: provenance.pack_version.to_owned(),
                prompt_version: provenance.prompt_version.to_owned(),
                fixture: fixture.to_owned(),
                fixture_id: expected.fixture_id.clone(),
                strata: Vec::new(),
                raw_input: trace.source.clone(),
                decision_key: key,
                decision: ScoredDecision::Extraction {
                    expected: None,
                    expected_review: false,
                    unauthored_negative: true,
                    actual: ExtractionOutcome::Absent,
                },
                evidence: BTreeMap::new(),
                trace_ids: passage_trace_ids(outcome, &trace.source),
                confidence: declared_confidence(outcome, &trace.source),
                exchanges,
            }
        })
        .collect()
}

fn passage_trace_ids(outcome: &RunOutcome, passage: &str) -> Vec<String> {
    outcome
        .claim_traces
        .iter()
        .filter(|trace| same_passage(&trace.source, passage))
        .map(|trace| trace.id.clone())
        .collect()
}

/// The confidence the classify answer declared about **this merchant's
/// category** (#429), read from the run's own claim ledger.
///
/// The `Classification` filter is the per-question split. The ledger
/// also carries a `Normalisation` trace for the same merchant, with its
/// own confidence about whether the name was recognised; that is a
/// different question, and a calibration table built by taking whatever
/// confidence was nearest would be measuring the wrong one (#271).
///
/// Several traces answering one merchant with different levels is
/// untraceable rather than resolved by preference, exactly as the
/// passage-shaped path treats it.
fn declared_category_confidence(
    expected: &Expected,
    outcome: &RunOutcome,
    raw: &str,
) -> Option<super::DeclaredConfidence> {
    let declared: Vec<(&str, &str)> = outcome
        .claim_traces
        .iter()
        .filter(|trace| {
            trace.kind == crate::claim_trace::ClaimKind::Classification
                && same_merchant(expected, raw, &trace.source)
        })
        .filter_map(|trace| {
            trace
                .candidate
                .get("confidence")
                .and_then(serde_json::Value::as_str)
                .filter(|level| !level.is_empty())
                .map(|level| (level, trace.id.as_str()))
        })
        .collect();
    let (level, trace_id) = *declared.first()?;
    if declared.iter().all(|(seen, _)| *seen == level) {
        Some(super::DeclaredConfidence::Declared {
            level: level.to_owned(),
            trace_id: trace_id.to_owned(),
        })
    } else {
        Some(super::DeclaredConfidence::Untraceable {
            levels: declared
                .iter()
                .map(|(seen, _)| (*seen).to_owned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }
}

fn merchant_trace_ids(expected: &Expected, outcome: &RunOutcome, raw: &str) -> Vec<String> {
    outcome
        .claim_traces
        .iter()
        .filter(|trace| {
            matches!(
                trace.kind,
                crate::claim_trace::ClaimKind::Normalisation
                    | crate::claim_trace::ClaimKind::Classification
            ) && same_merchant(expected, raw, &trace.source)
        })
        .map(|trace| trace.id.clone())
        .collect()
}

/// Whether two renderings of a passage are the same passage. Whitespace
/// is normalised because a segment's text is reflowed from the page,
/// and an expectation is typed by a person.
/// A passage's identity for grouping decisions: whitespace and case
/// folded, the same comparison [`same_passage`] makes.
fn passage_key(passage: &str) -> String {
    passage
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn same_passage(left: &str, right: &str) -> bool {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    words(left).eq_ignore_ascii_case(&words(right))
}

fn captured_classification(exchanges: &[CapturedExchange], raw: &str) -> Option<Classification> {
    exchanges.iter().rev().find_map(|exchange| {
        let id = exchange
            .items
            .iter()
            .find(|(_, source)| source.eq_ignore_ascii_case(raw))
            .map(|(id, _)| *id)?;
        let response: serde_json::Value = serde_json::from_str(&exchange.exchange.response).ok()?;
        response["results"]
            .as_array()?
            .iter()
            .find(|result| result["id"].as_u64() == Some(id as u64))
            .and_then(|result| {
                Some(Classification {
                    // Kind stopped being the model's answer in #253; a
                    // review-routed item never reached the derivation,
                    // so its proposal honestly has no kind.
                    kind: result["kind"].as_str().unwrap_or("unknown").to_owned(),
                    category: result["category"].as_str()?.to_owned(),
                })
            })
    })
}

fn run_classification(
    expected: &Expected,
    outcome: &AuditOutcome,
    raw: &str,
) -> Option<(Classification, String, KindFrom)> {
    outcome
        .findings
        .iter()
        .chain(&outcome.income)
        .find(|item| same_merchant(expected, raw, &item.raw_merchant))
        .map(|item| {
            (
                Classification {
                    kind: item.kind.clone(),
                    category: item.category.clone(),
                },
                item.confidence.clone(),
                item.kind_from,
            )
        })
        .or_else(|| {
            outcome
                .other
                .iter()
                .find(|item| same_merchant(expected, raw, &item.raw_merchant))
                .map(|item| {
                    (
                        Classification {
                            kind: item.kind.clone(),
                            category: item.category.clone(),
                        },
                        item.confidence.clone(),
                        item.kind_from,
                    )
                })
        })
}

/// The score that matters: did the run find the series the fixture says
/// are there, and no others? F1, so a missed subscription and an
/// invented one both cost — the two ways a report can be wrong while
/// looking finished.
///
/// A member is the **raw statement merchant** and its cadence, never
/// the model's normalised name. `recurring` is deterministic Rust
/// (CLAUDE.md), so this number must not move when the model has a bad
/// day: a merchant it misnames is `normalise`'s mark to lose, and
/// recurrence detection found the same series from the same
/// transactions either way. Keyed on the model's name instead, one bad
/// answer would be counted twice and the table would blame the wrong
/// half of the system.
///
/// The fixture's own `normalise` block is what ties the merchant names
/// its `recurring` expectations use back to the statement — which is
/// why [`Expected::validate`] insists every expected series has one.
///
/// Cadence is part of membership: a monthly series found as quarterly
/// annualises three times out, and that is a Rust mistake worth
/// failing over.
///
/// Only spending: `income` is not a subscription and is never totalled
/// as one (see [`crate::run::AuditOutcome::income`]).
fn score_end_to_end(expected: &Expected, outcome: &AuditOutcome) -> f32 {
    let want: Vec<String> = expected
        .recurring
        .iter()
        .map(|r| series_key(&r.merchant.to_ascii_lowercase(), &r.period))
        .collect();
    let got: Vec<String> = outcome
        .findings
        .iter()
        .map(|f| {
            series_key(
                &join_key(expected, &f.raw_merchant).to_ascii_lowercase(),
                f.period.as_wire(),
            )
        })
        .collect();

    set_f1(&borrowed(&want), &borrowed(&got))
}

/// The raw statement string a fixture's expected merchant name came
/// from. Falls back to the name itself, which `validate` has already
/// made unreachable for a well-formed fixture.
fn raw_merchant_for<'a>(expected: &'a Expected, merchant: &'a str) -> &'a str {
    expected
        .normalise
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(merchant))
        .map(|e| e.raw.as_str())
        .unwrap_or(merchant)
}

/// How much of the statement ended up in front of a person: merchants
/// Kettle couldn't answer for, plus the ones it answered with low
/// confidence and showed for checking rather than trusted. Both are
/// someone's work, which is what the rate is measuring. It is a cost,
/// not a quality judgement, and does not contribute to the verdict.
///
/// Counted in merchants, not transactions or findings. A merchant is
/// what a person actually has to look at. Findings would be the wrong
/// denominator twice over: two concurrent subscriptions to one merchant
/// are two findings but one decision, and a parked merchant produces no
/// finding at all.
///
/// Merchants are counted by the raw string the statement carried, which
/// every route out of a run keeps hold of — including the ones that
/// never got a name.
///
/// A statement with no merchants at all scores 0.0: nothing went to
/// anybody.
fn score_needs_review_rate(expected: &Expected, outcome: &RunOutcome, audit: &AuditOutcome) -> f32 {
    let mut everyone: BTreeSet<&str> = BTreeSet::new();
    let mut theirs: BTreeSet<&str> = BTreeSet::new();

    for (raw, confidence) in audit
        .findings
        .iter()
        .chain(&audit.income)
        .map(|f| (f.raw_merchant.as_str(), f.confidence.as_str()))
        .chain(
            audit
                .other
                .iter()
                .map(|s| (s.raw_merchant.as_str(), s.confidence.as_str())),
        )
    {
        // Three descriptors of one merchant are one decision (#253).
        let key = join_key(expected, raw);
        everyone.insert(key);
        if confidence == LOW_CONFIDENCE {
            theirs.insert(key);
        }
    }
    for item in &outcome.needs_review {
        let key = join_key(expected, &item.subject);
        everyone.insert(key);
        theirs.insert(key);
    }

    if everyone.is_empty() {
        return 0.0;
    }
    theirs.len() as f32 / everyone.len() as f32
}

/// The expected merchant name a raw statement string belongs to, from
/// the fixture's own normalise table — the one place that knows every
/// descriptor of every planted merchant.
///
/// This is the join every scorer goes through (#253, scoring v5). A
/// merchant paid through several processors appears on the statement
/// under several raw strings, the pipeline's grouping keeps one of
/// them as its representative, and which one is an implementation
/// detail — so joining on raw equality scored 555 of the Stage 3
/// bed's items as "no classification produced" when the pipeline had
/// classified every one of them under a sibling descriptor. Raw
/// strings stay in the records for people; identity is the name.
fn canonical<'a>(expected: &'a Expected, raw: &str) -> Option<&'a str> {
    expected
        .normalise
        .iter()
        .find(|entry| entry.raw.eq_ignore_ascii_case(raw))
        .map(|entry| entry.name.as_str())
}

/// The join key for one outcome entry: its canonical name where the
/// fixture knows the raw string, the raw string itself where it does
/// not (a fixture without a normalise table still joins exactly).
fn join_key<'a>(expected: &'a Expected, raw: &'a str) -> &'a str {
    canonical(expected, raw).unwrap_or(raw)
}

/// Do an expected item's raw string and an outcome entry's raw string
/// name the same real-world merchant?
fn same_merchant(expected: &Expected, wanted_raw: &str, outcome_raw: &str) -> bool {
    outcome_raw.eq_ignore_ascii_case(wanted_raw)
        || matches!(
            (canonical(expected, wanted_raw), canonical(expected, outcome_raw)),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b)
        )
}

/// One series' identity for set comparison: who, and how often.
fn series_key(merchant: &str, period: &str) -> String {
    format!("{merchant}|{period}")
}

fn borrowed(owned: &[String]) -> Vec<&str> {
    owned.iter().map(String::as_str).collect()
}

/// Every merchant the run put a name to, raw string first.
fn named_merchants(outcome: &AuditOutcome) -> Vec<(&str, &str)> {
    outcome
        .findings
        .iter()
        .chain(&outcome.income)
        .map(|f| (f.raw_merchant.as_str(), f.merchant.as_str()))
        .chain(
            outcome
                .other
                .iter()
                .map(|s| (s.raw_merchant.as_str(), s.merchant.as_str())),
        )
        .collect()
}

/// The tolerance a fixture sets for a step. Absent means exact: scoring
/// must never invent a forgiveness nobody wrote down (see
/// [`crate::scoring`]).
fn tolerance(expected: &Expected, step: &str) -> Tolerance {
    expected
        .tolerances
        .get(step)
        .and_then(|spelling| spelling.parse().ok())
        .unwrap_or(Tolerance::Exact)
}

/// A score kept beside the counts that produced it, so the table can say
/// "44 of 50" rather than only 0.88.
fn scored(want: &[(&str, &str)], got: &[(&str, &str)], tolerance: Tolerance) -> StepScore {
    let score = keyed_accuracy(want, got, tolerance);
    StepScore {
        score,
        expected: want.len(),
        correct: (score * want.len() as f32).round() as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim_trace::{
        CheckOutcome, ClaimCheck, ClaimTrace, Guardrail, TerminalDisposition,
    };
    use chrono::NaiveDate;

    /// #554, structurally: three parts of the scorer asked "is the
    /// wording part of the claim" and gave three answers. The pooled
    /// join ignored a dated deadline's words and the anchor (#287);
    /// `same_assertion_as` took the deadline verbatim and the anchor by
    /// its date (#452); `support` took both verbatim. So an obligation
    /// could be *found*, *confident-wrong* and *unsupported* at once —
    /// which the re-authored exam bed reported 12 times on 21 August,
    /// and what `support` reported 110 times per run on the bed that
    /// passes. One definition, three consumers — and each case says
    /// what the one answer must be, so the three agreeing on the wrong
    /// answer is not a pass.
    #[test]
    fn found_confident_wrong_and_support_give_one_answer_on_wording() {
        use super::super::evidence::{
            obligation_evidence, DimensionOutcome, EvidenceDeclaration, EvidenceDimension,
        };
        use super::super::{ExpectedObligation, Extracted};
        use crate::claim::Kind;
        use crate::timeline::Resolved;

        let date = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a date");
        let expected = |deadline: &str, anchor: &str, due: Option<&str>| ExpectedObligation {
            kind: "payment".to_owned(),
            party: "Elmswood Lettings".to_owned(),
            deadline: deadline.to_owned(),
            anchor: anchor.to_owned(),
            amount: "no amount".to_owned(),
            due: due.map(date),
        };
        let found =
            |party: &str, deadline: &str, anchor: &str, due: Option<&str>| crate::run::Obligation {
                kind: "payment".to_owned(),
                party: party.to_owned(),
                ask: "Pay the arrears".to_owned(),
                deadline: deadline.to_owned(),
                anchor: anchor.to_owned(),
                amount: "no amount".to_owned(),
                confidence: "high".to_owned(),
                due: due.map(|d| Resolved {
                    date: date(d),
                    kind: Kind::WorkedOut,
                }),
                evidence: Vec::new(),
                dated_by: None,
                priced_by: None,
                amount_from: None,
                deadline_from: None,
                disputed: Vec::new(),
            };
        let declared = BTreeMap::from([(
            EvidenceDimension::Support,
            EvidenceDeclaration {
                reason: "test".to_owned(),
                date: date("2026-08-22"),
            },
        )]);

        let cases: Vec<(&str, bool, ExpectedObligation, crate::run::Obligation)> = vec![
            (
                "the anchor worded as the deadline, same day",
                true,
                expected(
                    "within 28 days",
                    "the date of this letter",
                    Some("2026-03-31"),
                ),
                found(
                    "Elmswood Lettings",
                    "within 28 days",
                    "within 28 days",
                    Some("2026-03-31"),
                ),
            ),
            (
                "the deadline carrying its own anchor, same day",
                true,
                expected("within 45 days", "23 August 2026", Some("2026-10-07")),
                found(
                    "Elmswood Lettings",
                    "within 45 days of 23 August 2026",
                    "",
                    Some("2026-10-07"),
                ),
            ),
            (
                "the party as the merchant join reads it",
                true,
                expected(
                    "within 28 days",
                    "the date of this letter",
                    Some("2026-03-31"),
                ),
                found(
                    "ELMSWOOD LETTINGS",
                    "within 28 days",
                    "the date of this letter",
                    Some("2026-03-31"),
                ),
            ),
            (
                "a different day",
                false,
                expected(
                    "within 21 days",
                    "the date of this letter",
                    Some("2026-03-24"),
                ),
                found(
                    "Elmswood Lettings",
                    "within 22 days",
                    "the date of this letter",
                    Some("2026-03-25"),
                ),
            ),
            (
                "the right day, computed by the model instead of read",
                false,
                expected("within 45 days", "23 August 2026", Some("2026-10-07")),
                found(
                    "Elmswood Lettings",
                    "by 7 October 2026",
                    "",
                    Some("2026-10-07"),
                ),
            ),
            (
                "the right day, fused from a table row the passage only points at (#544)",
                false,
                expected("the date shown beside it", "", Some("2026-03-06")),
                found(
                    "Elmswood Lettings",
                    "by 6 March 2026",
                    "",
                    Some("2026-03-06"),
                ),
            ),
            (
                "undated, the same words",
                true,
                expected("at your earliest convenience", "", None),
                found(
                    "Elmswood Lettings",
                    "at your earliest convenience",
                    "",
                    None,
                ),
            ),
            (
                "undated, different words",
                false,
                expected("at your earliest convenience", "", None),
                found("Elmswood Lettings", "when you can", "", None),
            ),
            (
                "undated, the anchor naming a different date (#452)",
                false,
                expected("as soon as possible", "the hearing on 1 June 2026", None),
                found(
                    "Elmswood Lettings",
                    "as soon as possible",
                    "the hearing on 8 June 2026",
                    None,
                ),
            ),
            (
                "dated on one side only",
                false,
                expected(
                    "within 28 days",
                    "the date of this letter",
                    Some("2026-03-31"),
                ),
                found("Elmswood Lettings", "within 28 days", "", None),
            ),
        ];

        for (case, answer, want, got) in cases {
            let found_it = want.identity() == got.identity();
            let same = Extracted::Obligation(want.clone())
                .same_assertion_as(&Extracted::Obligation(ExpectedObligation::from(&got)));
            let expectation = ObligationExpectation {
                id: "one".to_owned(),
                strata: Vec::new(),
                segment: "Please pay.".to_owned(),
                expect: Some(want),
                evidence: None,
            };
            let supported = matches!(
                obligation_evidence(&declared, &expectation, &got, &[])
                    .get(&EvidenceDimension::Support),
                Some(DimensionOutcome::Pass)
            );
            assert!(
                found_it == answer && same == answer && supported == answer,
                "{case}: expected {answer}; found {found_it}, same assertion {same}, supported \
                 {supported} — one answer was asked for"
            );
        }
    }

    /// #470: an error the pipeline introduced has its own column, and
    /// only that column — booking it as `accepted` would hide it, and
    /// booking it as containment would credit the layer for the fault
    /// it caused (#432's scorecard depends on this staying apart).
    #[test]
    fn a_pipeline_introduced_error_is_counted_apart_from_acceptance_and_containment() {
        let mut trace = ClaimTrace {
            id: "claim-000001".to_owned(),
            parent_id: None,
            pack: "app.kttl.letter".to_owned(),
            step: "Reading what the letter asks".to_owned(),
            batch: 1,
            item: 0,
            candidate_index: 0,
            kind: crate::claim_trace::ClaimKind::Obligation,
            source: "Please pay £120.00 within 30 days of 22 May 2026.".to_owned(),
            candidate: serde_json::json!({"deadline": "within 30 days of 22 May 2026"}),
            attempts: Vec::new(),
            checks: vec![ClaimCheck {
                guardrail: Guardrail::Schema,
                outcome: CheckOutcome::Passed,
                detail: None,
            }],
            terminal: TerminalDisposition::Accepted,
            outputs: Vec::new(),
        };
        trace.record_derivation(
            "timeline::resolve_deadline",
            "due 2026-06-21, worked out",
            "due 2026-05-22, read and verified",
        );

        let metrics = containment_metrics(&[], std::slice::from_ref(&trace));

        assert_eq!(metrics.pipeline_introduced, 1);
        assert_eq!(metrics.accepted, 0);
        assert_eq!(metrics.contained, 0);
        // The stage's act is not a candidate failing a boundary: the
        // derivation guardrail must not book a `failed` the tables
        // would read as containment.
        assert!(metrics
            .by_guardrail
            .get(&Guardrail::DeterministicDerivation)
            .is_none_or(|boundary| boundary.failed == 0));
    }
}
