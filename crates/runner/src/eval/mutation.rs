//! #426: semantic mutation testing over recorded answers.
//!
//! An eval normally waits for a model to make a useful mistake, which
//! proves model behaviour but is an expensive way to prove a safeguard.
//! This harness deliberately alters recorded candidate answers, replays
//! them through the downstream pipeline — never calling a model — and
//! verifies that a guardrail contains the change or the eval verdict
//! fails. It tests the safety case, not line coverage.
//!
//! The original recording is never written to; mutants are applied to
//! in-memory clones. A mutant nobody catches is the finding, and the
//! harness fails naming it.

use super::replay::Recording;
use super::{MachineInfo, ScoredItem, Verdict};
use crate::packs::Pack;
use crate::run::Answers;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Mutations are deterministic and versioned: the same recording and
/// the same version produce the same mutants. Bump when an operator's
/// meaning or site enumeration changes.
pub const MUTATION_VERSION: u32 = 1;

/// One way of deliberately damaging a recorded answer. Each operator
/// names the harm it introduces and the response the system is
/// expected to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperator {
    /// Remove one obligation the recording found — the harm is a
    /// missed deadline, and the eval gate must fail the replay.
    DropObligation,
    /// Add an obligation to a passage that asks nothing — the harm is
    /// an invented demand a person might act on.
    InventObligation,
    /// Change one digit in a recorded deadline phrase — the harm is a
    /// date that reads plausibly and is wrong.
    DeadlineDigit,
    /// Remove one batch answer entirely — the harm is a question that
    /// silently never gets answered. Pairing must route it to a
    /// person, never score it as an honest absence.
    BatchOmit,
    /// Give a term a real quote lifted from another passage — the harm
    /// is evidence that verifies verbatim somewhere while attributing
    /// the claim to words that never carried it.
    WrongDocumentQuote,
    /// Swap the two documents' values for the same term — the harm is
    /// a rise reading as a cut. Each value keeps its own quote, so
    /// only the scoring join can catch it.
    SwapRoles,
    /// Keep a two-figure quote and assert the other figure — the harm
    /// is a wrong value carrying verbatim evidence that contains it.
    WrongValueFromMultiValueQuote,
    /// Answer one question twice — the harm is one passage becoming
    /// two findings. Pairing must reject or deduplicate it.
    BatchDuplicate,
    /// Swap two answers' ids — the harm is each reading attaching to
    /// the other's passage.
    BatchMispair,
    /// Reverse the batch's answers. Pairing is by id, so this declares
    /// no effect: the mutant is killed by invariance, and any item
    /// that moves under it names a defect.
    BatchReorder,
    /// Rewrite a digit-free deadline as a plausible absolute date —
    /// the harm is a date nobody stated entering the timeline.
    InventDate,
    /// Break an enum field's value — the harm is an answer outside the
    /// contract; schema validation must reject it before anything
    /// downstream sees it.
    SchemaBreak,
    /// Read a term the pack does not model — the harm is a value
    /// adopted outside the contract; pack coverage refuses it into
    /// review.
    UnmodelledTerm,
    /// Give a value a form its declared shape cannot hold — the #380
    /// boundary: referred to a person, never a finding.
    ValueShapeBreak,
    /// Force every answered confidence to low — a cost mutation, not a
    /// harm: the answers stay right, and the safeguard on trial is
    /// that the system responds to uncertainty by surfacing. Killed by
    /// the cost shift.
    ForceConfidenceLow,
    /// Force every low confidence to high. False confidence only bites
    /// on wrong answers, which an oracle recording cannot contain by
    /// construction — sites exist only on real model recordings.
    ForceConfidenceHigh,
}

impl MutationOperator {
    /// Every operator, for reports and completeness checks. Append
    /// only, like the beds: order is part of a mutation run's identity.
    pub const ALL: [MutationOperator; 16] = [
        MutationOperator::DropObligation,
        MutationOperator::InventObligation,
        MutationOperator::DeadlineDigit,
        MutationOperator::BatchOmit,
        MutationOperator::WrongDocumentQuote,
        MutationOperator::SwapRoles,
        MutationOperator::WrongValueFromMultiValueQuote,
        MutationOperator::BatchDuplicate,
        MutationOperator::BatchMispair,
        MutationOperator::BatchReorder,
        MutationOperator::InventDate,
        MutationOperator::SchemaBreak,
        MutationOperator::UnmodelledTerm,
        MutationOperator::ValueShapeBreak,
        MutationOperator::ForceConfidenceLow,
        MutationOperator::ForceConfidenceHigh,
    ];

    /// The harm this operator plants, for the report.
    pub fn harm(&self) -> &'static str {
        match self {
            MutationOperator::DropObligation => "a genuine obligation disappears from the report",
            MutationOperator::InventObligation => {
                "an obligation nobody stated appears in the report"
            }
            MutationOperator::DeadlineDigit => "a deadline moves by one plausible digit",
            MutationOperator::BatchOmit => "one question's answer silently disappears",
            MutationOperator::WrongDocumentQuote => {
                "a claim carries a real quote from the wrong passage"
            }
            MutationOperator::SwapRoles => "the two documents' values trade places",
            MutationOperator::WrongValueFromMultiValueQuote => {
                "the wrong figure is read out of a two-figure passage"
            }
            MutationOperator::BatchDuplicate => "one passage answers twice",
            MutationOperator::BatchMispair => "two readings attach to each other's passages",
            MutationOperator::BatchReorder => "the answers arrive in a different order",
            MutationOperator::InventDate => "a deadline nobody dated grows a plausible date",
            MutationOperator::SchemaBreak => "an answer steps outside its schema",
            MutationOperator::UnmodelledTerm => "a term the pack does not model is read anyway",
            MutationOperator::ValueShapeBreak => "a value arrives in a form its shape cannot hold",
            MutationOperator::ForceConfidenceLow => "every reading loses its confidence",
            MutationOperator::ForceConfidenceHigh => "every uncertain reading claims certainty",
        }
    }

    /// Where the system is expected to catch it.
    pub fn expected_containment(&self) -> Containment {
        match self {
            MutationOperator::DropObligation
            | MutationOperator::InventObligation
            | MutationOperator::DeadlineDigit => Containment::Scoring,
            MutationOperator::BatchOmit => {
                Containment::Guardrail(crate::claim_trace::Guardrail::Pairing)
            }
            MutationOperator::WrongDocumentQuote => {
                Containment::Guardrail(crate::claim_trace::Guardrail::Quote)
            }
            MutationOperator::SwapRoles | MutationOperator::WrongValueFromMultiValueQuote => {
                Containment::Scoring
            }
            MutationOperator::BatchDuplicate | MutationOperator::BatchMispair => {
                Containment::Guardrail(crate::claim_trace::Guardrail::Pairing)
            }
            MutationOperator::BatchReorder => Containment::NoEffect,
            MutationOperator::InventDate => Containment::Scoring,
            MutationOperator::SchemaBreak => {
                Containment::Guardrail(crate::claim_trace::Guardrail::Schema)
            }
            MutationOperator::UnmodelledTerm => {
                Containment::Guardrail(crate::claim_trace::Guardrail::PackCoverage)
            }
            MutationOperator::ValueShapeBreak => {
                Containment::Guardrail(crate::claim_trace::Guardrail::ValueShape)
            }
            MutationOperator::ForceConfidenceLow => Containment::CostShift,
            MutationOperator::ForceConfidenceHigh => Containment::Scoring,
        }
    }
}

/// The layer expected to contain a mutant: a runtime guardrail from
/// the claim lifecycle, or the eval's own scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Containment {
    Guardrail(crate::claim_trace::Guardrail),
    Scoring,
    /// The mutation must change nothing observable — an invariance
    /// declaration. Anything that moves under it is the defect.
    NoEffect,
    /// A cost mutation: the answers stay right, and the system must
    /// respond by paying — review rising, decisions surfacing. Killed
    /// by the shift; surviving means the cost was silently absorbed.
    CostShift,
}

/// One mutant: what was changed, where, and what happened to it.
#[derive(Debug, Clone, Serialize)]
pub struct MutationRecord {
    pub operator: MutationOperator,
    /// The request whose recorded answer was altered.
    pub source_digest: String,
    /// Where in the answer the operator applied, as a JSON path.
    pub site: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
    /// Scored items whose decision changed under the mutant.
    pub affected_items: Vec<String>,
    /// Whether the report's verdict moved against the mutant.
    pub verdict_moved: bool,
    /// Whether the affected decisions were routed to a person — the
    /// containment that is a cost, never a verdict failure.
    pub contained_in_review: bool,
    /// Whether the pipeline rejected the mutated answer and demanded a
    /// re-ask. A replay cannot serve a question the recording never
    /// heard, so the demand surfaces as a refusal here; in live
    /// operation the failed re-ask lands in needs-review.
    pub retry_demanded: bool,
    /// Whether the mutated replay paid a visible cost — its fixture's
    /// review rate rose against the baseline's.
    pub cost_shift: bool,
    /// A killed mutant was caught; a surviving one names a gap.
    pub killed: bool,
}

/// Every mutant's fate, plus the provenance to reproduce them.
#[derive(Debug, Serialize)]
pub struct MutationReport {
    pub mutation_version: u32,
    pub scoring_version: u32,
    pub pack: String,
    pub records: Vec<MutationRecord>,
}

impl MutationReport {
    /// The mutants nothing caught. The aggregate is a summary; this
    /// list is the diagnosis, and it must always be shown.
    pub fn survivors(&self) -> Vec<&MutationRecord> {
        self.records
            .iter()
            .filter(|record| !record.killed)
            .collect()
    }
}

/// How many scored decisions a survivor moved — the behaviour that,
/// with the operator, identifies its family. The #466 census's two
/// families are exactly these: a ceiling-tolerated single error moves
/// one item the gate honestly absorbs, and an already-contained claim
/// moves nothing because the baseline had already routed it to review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Moved {
    Nothing,
    OneItem,
}

/// One declared family of expected survivors (#466, built by #484):
/// the gates tolerate these on purpose, at a named count, for a stated
/// reason. A survivor matches by behaviour — operator and what it
/// moved — never by memory of a PR body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFamily {
    pub name: String,
    pub count: usize,
    pub operators: Vec<MutationOperator>,
    pub moved: Moved,
    pub reason: String,
}

impl ExpectedFamily {
    /// Whether this survivor is one of the family's. A survivor that
    /// moved more than one item matches no family: nothing declarable
    /// today behaves that way, so it is always something new.
    pub fn matches(&self, record: &MutationRecord) -> bool {
        self.operators.contains(&record.operator)
            && match self.moved {
                Moved::Nothing => record.affected_items.is_empty(),
                Moved::OneItem => record.affected_items.len() == 1,
            }
    }
}

/// A pack's declared expected survivors: `expected-survivors.json`
/// beside its manifest. Machine-readable so the harness reads it and
/// the assurance registry can cite it. A pack that declares nothing
/// gets the strict reading — every survivor is new.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedSurvivors {
    pub families: Vec<ExpectedFamily>,
    /// Caveats the counts alone would lose — an operator with no sites
    /// is a coverage gap, not a clean sweep. Carried, never acted on.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// One declared family beside what the run actually found of it.
#[derive(Debug)]
pub struct FamilyTally<'a> {
    pub family: &'a ExpectedFamily,
    pub found: usize,
}

impl FamilyTally<'_> {
    pub fn over_count(&self) -> bool {
        self.found > self.family.count
    }
}

/// A report's survivors sorted against the declaration: each declared
/// family's tally, and every survivor no family covers.
#[derive(Debug)]
pub struct Triage<'a> {
    pub declared: Vec<FamilyTally<'a>>,
    pub new: Vec<&'a MutationRecord>,
}

impl Triage<'_> {
    /// The question the exit code answers (#466): did anything survive
    /// that the declaration does not tolerate — an undeclared family,
    /// or a declared family above its count?
    pub fn anything_new(&self) -> bool {
        !self.new.is_empty() || self.declared.iter().any(FamilyTally::over_count)
    }
}

impl ExpectedSurvivors {
    /// Read a pack's declaration from its directory. No file is a
    /// declaration of nothing — the strict reading. A file that exists
    /// but cannot be read is an error, never quietly nothing: treating
    /// it as empty would flip what the exit code means.
    pub fn load(pack_dir: &std::path::Path) -> Result<Self, String> {
        let path = pack_dir.join("expected-survivors.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is not a valid expected-survivors declaration: {e}",
                path.display()
            )
        })
    }

    /// Sort a report's survivors against the declaration. A survivor
    /// matches the first family that covers it; the census families
    /// are disjoint by construction, so order carries no judgement.
    pub fn triage<'a>(&'a self, report: &'a MutationReport) -> Triage<'a> {
        let mut declared: Vec<FamilyTally<'a>> = self
            .families
            .iter()
            .map(|family| FamilyTally { family, found: 0 })
            .collect();
        let mut new = Vec::new();
        for record in report.records.iter().filter(|record| !record.killed) {
            match declared
                .iter_mut()
                .find(|tally| tally.family.matches(record))
            {
                Some(tally) => tally.found += 1,
                None => new.push(record),
            }
        }
        Triage { declared, new }
    }
}

/// Runs mutants against a pack's recording and reports each one's fate.
pub struct MutationHarness {
    pub machine: MachineInfo,
}

impl MutationHarness {
    pub fn run(
        &self,
        pack: &Pack,
        recording: &Recording,
        operators: &[MutationOperator],
    ) -> Result<MutationReport, String> {
        // The sanity gate: the unmutated replay must pass, or every
        // mutant would read as killed by whatever was already broken.
        let baseline = self.replay(pack, recording.clone())?;
        if baseline.verdict != Verdict::Pass {
            let gates: Vec<String> = baseline
                .metrics
                .values()
                .flat_map(|metric| metric.gate_lines())
                .collect();
            let diagnosis: Vec<String> = baseline
                .fixtures
                .iter()
                .map(|fixture| {
                    let items: Vec<String> = fixture
                        .items
                        .iter()
                        .map(|item| {
                            format!(
                                "{}: {}",
                                item.item_id,
                                serde_json::to_string(&item.decision).unwrap_or_default()
                            )
                        })
                        .collect();
                    format!(
                        "{} steps={:?} end_to_end={} review={} | {}",
                        fixture.fixture,
                        fixture.step_scores,
                        fixture.end_to_end,
                        fixture.needs_review_rate,
                        items.join("; ")
                    )
                })
                .collect();
            return Err(format!(
                "the unmutated recording does not pass ({:?}) — fix the recording or the \
                 bed before measuring mutants against it.\ngates:\n{}\n{}",
                baseline.verdict,
                gates.join("\n"),
                diagnosis.join("\n")
            ));
        }
        let baseline_items = decisions_of(&baseline);
        // Which fixture each recorded question belongs to, from the
        // baseline's own exchanges. A mutant replays only its fixture:
        // replaying a hundred untouched fixtures per mutant measures
        // nothing extra, and on a real bed it turns minutes into hours.
        let mut owner: BTreeMap<String, String> = BTreeMap::new();
        for fixture in &baseline.fixtures {
            for item in &fixture.items {
                for exchange in &item.exchanges {
                    owner
                        .entry(super::replay::digest_prompt_only(&exchange.request))
                        .or_insert_with(|| fixture.fixture.clone());
                }
            }
        }
        let thresholds = pack.thresholds();

        let mut records = Vec::new();
        for operator in operators {
            for (digest, body) in recording.entries() {
                let Some(content) = completion_content(body) else {
                    continue;
                };
                let Some(fixture_name) = owner.get(digest) else {
                    continue;
                };
                for site in sites(*operator, &content) {
                    let (mutated, before, after) = apply(*operator, &content, &site);
                    let mut mutant = recording.clone();
                    mutant.replace_answer(digest, mutated.to_string());

                    let fixture_report = match self.replay_one(pack, mutant, fixture_name) {
                        Ok(report) => report,
                        // The pipeline refused the mutated answer and
                        // asked again — a question this recording never
                        // heard. That rejection is containment: the
                        // damaged answer was not accepted, and in live
                        // operation the failed re-ask routes to review.
                        Err(refusal) if refusal.contains("no recorded answer") => {
                            records.push(MutationRecord {
                                operator: *operator,
                                source_digest: digest.clone(),
                                site: site.clone(),
                                before: before.clone(),
                                after: after.clone(),
                                affected_items: Vec::new(),
                                verdict_moved: false,
                                contained_in_review: false,
                                retry_demanded: true,
                                cost_shift: false,
                                killed: true,
                            });
                            continue;
                        }
                        Err(other) => return Err(other),
                    };
                    // Splice the mutated fixture into the baseline and
                    // re-derive bed-level metrics and verdict, so a
                    // single wrong decision meets the same ceilings a
                    // live run would meet.
                    let mut report = baseline.clone();
                    for fixture in report.fixtures.iter_mut() {
                        if &fixture.fixture == fixture_name {
                            if let Some(mutated_fixture) = fixture_report
                                .fixtures
                                .iter()
                                .find(|candidate| &candidate.fixture == fixture_name)
                            {
                                *fixture = mutated_fixture.clone();
                            }
                        }
                    }
                    let all_items: Vec<super::ScoredItem> = report
                        .fixtures
                        .iter()
                        .flat_map(|fixture| fixture.items.iter().cloned())
                        .collect();
                    for (metric, slot) in report.metrics.iter_mut() {
                        *slot = match metric {
                            super::EvalMetric::Classification => {
                                super::MetricReport::Classification(
                                    super::classification_metrics(&all_items)
                                        .with_gates(&pack.manifest.eval_strata),
                                )
                            }
                            super::EvalMetric::Extraction => super::MetricReport::Extraction(
                                super::extraction_metrics(&all_items)
                                    .with_gates(&pack.manifest.eval_strata),
                            ),
                        };
                    }
                    report.verdict = report.overall_verdict(&thresholds);
                    let mutated_items = decisions_of(&report);
                    let affected_items: Vec<String> = baseline_items
                        .iter()
                        .filter(|(id, decision)| mutated_items.get(*id) != Some(decision))
                        .map(|(id, _)| id.clone())
                        .collect();
                    let verdict_moved = report.verdict != baseline.verdict;
                    let contained_in_review = !affected_items.is_empty()
                        && affected_items.iter().all(|id| {
                            mutated_items
                                .get(id)
                                .is_some_and(|decision| decision.contains("needs_review"))
                        });
                    let cost_shift = {
                        let baseline_rate = baseline
                            .fixtures
                            .iter()
                            .find(|fixture| &fixture.fixture == fixture_name)
                            .map(|fixture| fixture.needs_review_rate);
                        let mutated_rate = report
                            .fixtures
                            .iter()
                            .find(|fixture| &fixture.fixture == fixture_name)
                            .map(|fixture| fixture.needs_review_rate);
                        matches!((baseline_rate, mutated_rate), (Some(before), Some(after)) if after > before)
                    };
                    // An invariance operator inverts the question — the
                    // mutant is killed when nothing moved — and a cost
                    // operator is killed by the system visibly paying.
                    let killed = match operator.expected_containment() {
                        Containment::NoEffect => affected_items.is_empty() && !verdict_moved,
                        Containment::CostShift => cost_shift || contained_in_review,
                        _ => verdict_moved || contained_in_review,
                    };
                    records.push(MutationRecord {
                        operator: *operator,
                        source_digest: digest.clone(),
                        site: site.clone(),
                        before: before.clone(),
                        after: after.clone(),
                        affected_items,
                        verdict_moved,
                        contained_in_review,
                        retry_demanded: false,
                        cost_shift,
                        killed,
                    });
                }
            }
        }
        Ok(MutationReport {
            mutation_version: MUTATION_VERSION,
            scoring_version: super::SCORING_VERSION,
            pack: pack.manifest.id.clone(),
            records,
        })
    }

    /// Replay a recording through the full pipeline — structurally no
    /// model: the endpoint answers from the recording or refuses.
    fn replay(&self, pack: &Pack, recording: Recording) -> Result<super::EvalReport, String> {
        self.evaluator(recording).evaluate(pack)
    }

    /// Replay one fixture only — the mutant's own.
    fn replay_one(
        &self,
        pack: &Pack,
        recording: Recording,
        fixture: &str,
    ) -> Result<super::EvalReport, String> {
        self.evaluator(recording).evaluate_only(
            pack,
            super::fixture::EvalSelection::Development,
            fixture,
        )
    }

    fn evaluator(&self, recording: Recording) -> super::fixture::FixtureEvaluator {
        super::fixture::FixtureEvaluator {
            model: recording.model().cloned(),
            answers: Answers::FromModel(crate::exec::Endpoint::replaying(recording)),
            machine: self.machine.clone(),
            sidecar: None,
            peak_rss: None,
            fixtures_dir: None,
            runs_dir: None,
            resume_dir: None,
            pdfium_dir: None,
        }
    }
}

/// Every scored decision in a report, keyed by item id, serialised so
/// two replays can be diffed without caring which payload shape the
/// decision carries.
fn decisions_of(report: &super::EvalReport) -> BTreeMap<String, String> {
    report
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.items.iter())
        .map(|item: &ScoredItem| {
            (
                item.id.clone(),
                serde_json::to_string(&item.decision).unwrap_or_default(),
            )
        })
        .collect()
}

/// The answer JSON inside a recorded body. A run directory records the
/// model's parsed answer directly; a body inserted by tests may still
/// be a completion envelope, so both shapes are read.
fn completion_content(body: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("results").is_some() {
        return Some(value);
    }
    let content = value["choices"][0]["message"]["content"].as_str()?;
    serde_json::from_str(content).ok()
}

/// Every place in one answer where an operator can apply. Deterministic:
/// results in array order, claims in array order.
fn sites(operator: MutationOperator, content: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    match operator {
        MutationOperator::DropObligation => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                if let Some(obligations) = result["obligations"].as_array() {
                    for obligation_index in 0..obligations.len() {
                        found.push(format!(
                            "results[{result_index}].obligations[{obligation_index}]"
                        ));
                    }
                }
            }
        }
        MutationOperator::InventObligation => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                if result["obligations"].as_array().is_some_and(Vec::is_empty) {
                    found.push(format!("results[{result_index}].obligations"));
                }
            }
        }
        MutationOperator::DeadlineDigit => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                if let Some(obligations) = result["obligations"].as_array() {
                    for (obligation_index, obligation) in obligations.iter().enumerate() {
                        let deadline = obligation["deadline"].as_str().unwrap_or_default();
                        if deadline.chars().any(|c| c.is_ascii_digit()) {
                            found.push(format!(
                                "results[{result_index}].obligations[{obligation_index}].deadline"
                            ));
                        }
                    }
                }
            }
        }
        MutationOperator::BatchOmit => {
            for result_index in 0..results_of(content).len() {
                found.push(format!("results[{result_index}]"));
            }
        }
        MutationOperator::WrongDocumentQuote => {
            let results = results_of(content);
            for (result_index, result) in results.iter().enumerate() {
                if let Some(terms) = result["terms"].as_array() {
                    for (term_index, term) in terms.iter().enumerate() {
                        if borrowed_quote(&results, result_index, term).is_some() {
                            found.push(format!("results[{result_index}].terms[{term_index}]"));
                        }
                    }
                }
            }
        }
        MutationOperator::SwapRoles => {
            // One site per unordered pair of same-term, same-basis,
            // different-value claims — the two documents' readings.
            let all = term_sites(content);
            for (first, second) in pairs(&all) {
                let a = &content["results"][first.0]["terms"][first.1];
                let b = &content["results"][second.0]["terms"][second.1];
                if a["term"] == b["term"] && a["basis"] == b["basis"] && a["value"] != b["value"] {
                    found.push(format!(
                        "results[{}].terms[{}]<->results[{}].terms[{}]",
                        first.0, first.1, second.0, second.1
                    ));
                }
            }
        }
        MutationOperator::WrongValueFromMultiValueQuote => {
            for (result_index, term_index) in term_sites(content) {
                let term = &content["results"][result_index]["terms"][term_index];
                if other_money_token(term).is_some() {
                    found.push(format!("results[{result_index}].terms[{term_index}]"));
                }
            }
        }
        MutationOperator::BatchDuplicate => {
            for result_index in 0..results_of(content).len() {
                found.push(format!("results[{result_index}]"));
            }
        }
        MutationOperator::BatchMispair => {
            // Adjacent pairs trade ids; one site per neighbouring pair.
            let count = results_of(content).len();
            for result_index in 1..count {
                found.push(format!(
                    "results[{}]<->results[{result_index}]",
                    result_index - 1
                ));
            }
        }
        MutationOperator::BatchReorder => {
            if results_of(content).len() > 1 {
                found.push("results".to_owned());
            }
        }
        MutationOperator::InventDate => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                if let Some(obligations) = result["obligations"].as_array() {
                    for (obligation_index, obligation) in obligations.iter().enumerate() {
                        let deadline = obligation["deadline"].as_str().unwrap_or_default();
                        if !deadline.chars().any(|c| c.is_ascii_digit()) {
                            found.push(format!(
                                "results[{result_index}].obligations[{obligation_index}].deadline"
                            ));
                        }
                    }
                }
            }
        }
        MutationOperator::SchemaBreak => {
            // One site per claim-bearing result: the first enum field
            // of its first claim steps outside the contract.
            for (result_index, result) in results_of(content).iter().enumerate() {
                let has_claim = result["obligations"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
                    || result["terms"].as_array().is_some_and(|a| !a.is_empty());
                if has_claim {
                    found.push(format!("results[{result_index}]"));
                }
            }
        }
        MutationOperator::UnmodelledTerm => {
            for (result_index, term_index) in term_sites(content) {
                found.push(format!("results[{result_index}].terms[{term_index}]"));
            }
        }
        MutationOperator::ValueShapeBreak => {
            for (result_index, term_index) in term_sites(content) {
                found.push(format!("results[{result_index}].terms[{term_index}]"));
            }
        }
        MutationOperator::ForceConfidenceLow => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                let has_claim = result["obligations"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
                    || result["terms"].as_array().is_some_and(|a| !a.is_empty());
                if has_claim && result["confidence"] != "low" {
                    found.push(format!("results[{result_index}].confidence"));
                }
            }
        }
        MutationOperator::ForceConfidenceHigh => {
            for (result_index, result) in results_of(content).iter().enumerate() {
                if result["confidence"] == "low" {
                    found.push(format!("results[{result_index}].confidence"));
                }
            }
        }
    }
    found
}

/// Every (result, term) coordinate in an answer, in order.
fn term_sites(content: &serde_json::Value) -> Vec<(usize, usize)> {
    let mut sites = Vec::new();
    for (result_index, result) in results_of(content).iter().enumerate() {
        if let Some(terms) = result["terms"].as_array() {
            for term_index in 0..terms.len() {
                sites.push((result_index, term_index));
            }
        }
    }
    sites
}

/// Every unordered pair, first before second, in order.
fn pairs(sites: &[(usize, usize)]) -> Vec<((usize, usize), (usize, usize))> {
    let mut all = Vec::new();
    for (index, first) in sites.iter().enumerate() {
        for second in &sites[index + 1..] {
            all.push((*first, *second));
        }
    }
    all
}

/// A money-looking token in the quote other than the claim's own value
/// — the figure a wrong pick would assert. First in reading order, so
/// the same quote always picks the same wrong figure.
fn other_money_token(term: &serde_json::Value) -> Option<String> {
    let quote = term["quote"].as_str().unwrap_or_default();
    let own = term["value"].as_str().unwrap_or_default();
    let mut token = String::new();
    let mut tokens = Vec::new();
    for c in quote.chars() {
        if c == '£' || c.is_ascii_digit() || (c == '.' && !token.is_empty()) {
            token.push(c);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
        .into_iter()
        .filter(|t| t.starts_with('£'))
        .map(|t| t.trim_end_matches('.').to_owned())
        .find(|t| t != own)
}

/// A quote from a different passage's term, for the borrower at
/// `result_index` — the first one in order that differs, so the same
/// recording always borrows the same words.
fn borrowed_quote(
    results: &[serde_json::Value],
    result_index: usize,
    term: &serde_json::Value,
) -> Option<String> {
    let own = term["quote"].as_str().unwrap_or_default();
    results.iter().enumerate().find_map(|(other_index, other)| {
        if other_index == result_index {
            return None;
        }
        other["terms"].as_array().and_then(|terms| {
            terms.iter().find_map(|candidate| {
                let quote = candidate["quote"].as_str()?;
                (quote != own).then(|| quote.to_owned())
            })
        })
    })
}

/// Apply one operator at one site. Returns the mutated answer and the
/// before/after values at the site.
fn apply(
    operator: MutationOperator,
    content: &serde_json::Value,
    site: &str,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let mut mutated = content.clone();
    match operator {
        MutationOperator::DropObligation => {
            let (result_index, obligation_index) = parse_site(site);
            let before = content["results"][result_index]["obligations"][obligation_index].clone();
            if let Some(obligations) =
                mutated["results"][result_index]["obligations"].as_array_mut()
            {
                obligations.remove(obligation_index);
            }
            (mutated, before, serde_json::Value::Null)
        }
        MutationOperator::InventObligation => {
            let (result_index, _) = parse_site(site);
            let invented = serde_json::json!({
                "kind": "payment",
                "party": "An unnamed party",
                "ask": "Pay an amount this passage never asked for",
                "deadline": "within 7 days",
                "anchor": "the date of this letter",
                "amount": "no amount"
            });
            if let Some(obligations) =
                mutated["results"][result_index]["obligations"].as_array_mut()
            {
                obligations.push(invented.clone());
            }
            (mutated, serde_json::Value::Null, invented)
        }
        MutationOperator::DeadlineDigit => {
            let (result_index, obligation_index) = parse_site(site);
            let before = content["results"][result_index]["obligations"][obligation_index]
                ["deadline"]
                .clone();
            let moved = move_one_digit(before.as_str().unwrap_or_default());
            let after = serde_json::Value::String(moved);
            mutated["results"][result_index]["obligations"][obligation_index]["deadline"] =
                after.clone();
            (mutated, before, after)
        }
        MutationOperator::BatchOmit => {
            let (result_index, _) = parse_site(site);
            let before = content["results"][result_index].clone();
            if let Some(results) = mutated["results"].as_array_mut() {
                results.remove(result_index);
            }
            (mutated, before, serde_json::Value::Null)
        }
        MutationOperator::WrongDocumentQuote => {
            let (result_index, term_index) = parse_site(site);
            let before = content["results"][result_index]["terms"][term_index]["quote"].clone();
            let borrowed = borrowed_quote(
                &results_of(content),
                result_index,
                &content["results"][result_index]["terms"][term_index],
            )
            .unwrap_or_default();
            let after = serde_json::Value::String(borrowed);
            mutated["results"][result_index]["terms"][term_index]["quote"] = after.clone();
            (mutated, before, after)
        }
        MutationOperator::SwapRoles => {
            let mut numbers = site
                .split(['[', ']', '<', '-', '>', '.'])
                .filter_map(|part| part.parse::<usize>().ok());
            let (ra, ta, rb, tb) = (
                numbers.next().unwrap_or_default(),
                numbers.next().unwrap_or_default(),
                numbers.next().unwrap_or_default(),
                numbers.next().unwrap_or_default(),
            );
            let a = content["results"][ra]["terms"][ta]["value"].clone();
            let b = content["results"][rb]["terms"][tb]["value"].clone();
            mutated["results"][ra]["terms"][ta]["value"] = b.clone();
            mutated["results"][rb]["terms"][tb]["value"] = a.clone();
            let before = serde_json::json!([a, b]);
            let after = serde_json::json!([b, a]);
            (mutated, before, after)
        }
        MutationOperator::WrongValueFromMultiValueQuote => {
            let (result_index, term_index) = parse_site(site);
            let before = content["results"][result_index]["terms"][term_index]["value"].clone();
            let wrong = other_money_token(&content["results"][result_index]["terms"][term_index])
                .unwrap_or_default();
            let after = serde_json::Value::String(wrong);
            mutated["results"][result_index]["terms"][term_index]["value"] = after.clone();
            (mutated, before, after)
        }
        MutationOperator::BatchDuplicate => {
            let (result_index, _) = parse_site(site);
            let before = content["results"][result_index].clone();
            if let Some(results) = mutated["results"].as_array_mut() {
                results.insert(result_index + 1, before.clone());
            }
            (
                mutated,
                before.clone(),
                serde_json::json!([before.clone(), before]),
            )
        }
        MutationOperator::BatchMispair => {
            let (first, second) = parse_site(site);
            let a = content["results"][first]["id"].clone();
            let b = content["results"][second]["id"].clone();
            mutated["results"][first]["id"] = b.clone();
            mutated["results"][second]["id"] = a.clone();
            (
                mutated,
                serde_json::json!([a, b]),
                serde_json::json!([b, a]),
            )
        }
        MutationOperator::BatchReorder => {
            if let Some(results) = mutated["results"].as_array_mut() {
                results.reverse();
            }
            (
                mutated,
                serde_json::Value::String("original order".to_owned()),
                serde_json::Value::String("reversed".to_owned()),
            )
        }
        MutationOperator::InventDate => {
            let (result_index, obligation_index) = parse_site(site);
            let before = content["results"][result_index]["obligations"][obligation_index]
                ["deadline"]
                .clone();
            let after = serde_json::Value::String("by 15 June 2026".to_owned());
            mutated["results"][result_index]["obligations"][obligation_index]["deadline"] =
                after.clone();
            (mutated, before, after)
        }
        MutationOperator::SchemaBreak => {
            let (result_index, _) = parse_site(site);
            let field = if content["results"][result_index]["obligations"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                ("obligations", "kind")
            } else {
                ("terms", "term")
            };
            let before = content["results"][result_index][field.0][0][field.1].clone();
            let after = serde_json::Value::String("outside-the-contract".to_owned());
            mutated["results"][result_index][field.0][0][field.1] = after.clone();
            (mutated, before, after)
        }
        MutationOperator::UnmodelledTerm => {
            let (result_index, term_index) = parse_site(site);
            let before = content["results"][result_index]["terms"][term_index]["term"].clone();
            let after = serde_json::Value::String("other".to_owned());
            mutated["results"][result_index]["terms"][term_index]["term"] = after.clone();
            (mutated, before, after)
        }
        MutationOperator::ValueShapeBreak => {
            let (result_index, term_index) = parse_site(site);
            let before = content["results"][result_index]["terms"][term_index]["value"].clone();
            let after = serde_json::Value::String("five hundred pounds or so".to_owned());
            mutated["results"][result_index]["terms"][term_index]["value"] = after.clone();
            (mutated, before, after)
        }
        MutationOperator::ForceConfidenceLow | MutationOperator::ForceConfidenceHigh => {
            let (result_index, _) = parse_site(site);
            let before = content["results"][result_index]["confidence"].clone();
            let target = if operator == MutationOperator::ForceConfidenceLow {
                "low"
            } else {
                "high"
            };
            let after = serde_json::Value::String(target.to_owned());
            mutated["results"][result_index]["confidence"] = after.clone();
            (mutated, before, after)
        }
    }
}

/// The first digit in the phrase, moved by one — 4 becomes 5, 9 wraps
/// to 8 so the result is always a different valid digit. Deterministic
/// by construction: same phrase, same mutant.
fn move_one_digit(phrase: &str) -> String {
    let mut moved = String::with_capacity(phrase.len());
    let mut done = false;
    for c in phrase.chars() {
        if !done && c.is_ascii_digit() {
            let digit = c.to_digit(10).unwrap_or(0);
            let new = if digit == 9 { 8 } else { digit + 1 };
            moved.push(char::from_digit(new, 10).unwrap_or(c));
            done = true;
        } else {
            moved.push(c);
        }
    }
    moved
}

fn results_of(content: &serde_json::Value) -> Vec<serde_json::Value> {
    content["results"].as_array().cloned().unwrap_or_default()
}

/// "results[i].<claims>[j]" back into (i, j).
fn parse_site(site: &str) -> (usize, usize) {
    let mut numbers = site
        .split(['[', ']'])
        .filter_map(|part| part.parse::<usize>().ok());
    (
        numbers.next().unwrap_or_default(),
        numbers.next().unwrap_or_default(),
    )
}
