//! Execute the diagnostic corpus through the pack's ordinary pipeline.
//! Gold facts are used only after execution, never in a model request.

use super::corpus::{self, Case, CaseScore, Corpus, Proposal, ProposedAsk, Selection, VerifiedAsk};
use crate::claim_trace::{CheckOutcome, ClaimKind, ClaimTrace, Guardrail};
use crate::document::Segment;
use crate::exec::{BatchItem, GenerationRequest};
use crate::packs::{Pack, PipelineStep};
use crate::reading::Reading;
use crate::run::{Answers, Payload, RunResources, When};
use crate::run_dir::{RunDir, RunLog};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub const SCHEMA: &str = "kettle/corpus-evaluation@1";
pub const SCORING: &str = "corpus-fields-v6";
pub type Bindings = BTreeMap<String, Vec<(String, PathBuf)>>;

pub struct Evaluation<'a> {
    pub pack: &'a Pack,
    pub corpus_text: &'a str,
    /// Optional acquired inputs per case. Omitted cases render as plain text.
    pub bindings: &'a Bindings,
    pub answers: &'a Answers,
    pub model: Option<super::ModelInfo>,
    pub machine: super::MachineInfo,
    pub generation_machine: Option<super::MachineInfo>,
    pub sidecar: Option<super::SidecarInfo>,
    pub runtime: Option<super::resume::RuntimeIdentity>,
    /// A new directory, never an existing recording to replace.
    pub output: &'a Path,
    pub pdfium_dir: Option<&'a Path>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: String,
    pub scoring: String,
    pub answer_source: String,
    pub corpus_digest: String,
    pub slice: String,
    pub pack: String,
    pub pipeline_digest: String,
    pub executable_digest: String,
    pub fields: Vec<corpus::Field>,
    #[serde(default)]
    pub selection: Option<corpus::DiagnosticSelection>,
    /// Consumed challenge lifecycle, retained unchanged during exact replay.
    #[serde(default)]
    pub challenge: Option<serde_json::Value>,
    pub model: Option<super::ModelInfo>,
    pub scoring_machine: super::MachineInfo,
    pub generation_machine: Option<super::MachineInfo>,
    pub sidecar: Option<super::SidecarInfo>,
    pub runtime: Option<super::resume::RuntimeIdentity>,
    pub replay: Option<super::replay::ReplayCompatibility>,
    pub cases: Vec<CaseReport>,
    pub summary: corpus::Summary,
    /// These cases stay in the report but have no fabricated reading score.
    pub unscored_cases: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputIdentity {
    pub role: String,
    pub file: String,
    pub digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaseReport {
    pub case: String,
    #[serde(default)]
    pub coverage: Vec<corpus::InventoryLink>,
    pub inputs: Vec<InputIdentity>,
    pub segments: Vec<Segment>,
    /// Authored passage index -> actual pooled model item ids, in order.
    pub passage_map: BTreeMap<usize, Vec<usize>>,
    pub acquisition_errors: Vec<String>,
    /// Shared segments whose distinct source ask sites are paired by kind.
    #[serde(default)]
    pub kind_matched_segments: std::collections::BTreeSet<usize>,
    #[serde(default)]
    pub attribution_errors: Vec<String>,
    pub execution_error: Option<String>,
    pub raw: Proposal,
    pub verified: Vec<VerifiedAsk>,
    pub diagnostics: Vec<Diagnostic>,
    pub score: Option<CaseScore>,
    pub exchanges: Vec<super::ModelExchange>,
    pub claims: Vec<ClaimTrace>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub trace: String,
    pub reason: String,
}

#[derive(Default)]
struct Capture<'a> {
    disk: Option<&'a RunDir>,
    segments: RefCell<Vec<Segment>>,
    exchanges: RefCell<Vec<super::ModelExchange>>,
}

impl RunLog for Capture<'_> {
    fn document(&self, segments: &[Segment]) {
        self.segments.replace(segments.to_vec());
    }

    fn generation(
        &self,
        step: &str,
        batch: usize,
        items: &[BatchItem],
        request: &GenerationRequest,
        response: &str,
    ) {
        if let Some(disk) = self.disk {
            disk.generation(step, batch, items, request, response);
        }
        self.exchanges.borrow_mut().push(super::ModelExchange {
            generation: Some(request.clone()),
            step: step.into(),
            batch,
            request: request.prompt().unwrap_or_default().into(),
            response: response.into(),
        });
    }

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
        self.exchanges.borrow_mut().push(super::ModelExchange {
            generation: None,
            step: step.into(),
            batch,
            request: request.into(),
            response: response.into(),
        });
    }
}

impl Evaluation<'_> {
    pub fn evaluate(&self) -> Result<Report, String> {
        let corpus = Corpus::parse(self.corpus_text)?;
        self.evaluate_corpus(corpus, None)
    }

    pub fn evaluate_challenge(&self, attempt: super::challenge::Attempt) -> Result<Report, String> {
        let replay =
            matches!(self.answers, Answers::FromModel(e) if e.replay_compatibility().is_some());
        let corpus = attempt.corpus(self.corpus_text, replay)?;
        self.evaluate_corpus(corpus, Some(attempt.record()))
    }

    fn evaluate_corpus(
        &self,
        corpus: Corpus,
        challenge: Option<serde_json::Value>,
    ) -> Result<Report, String> {
        // This adapter judges one closed obligations question, not a mixture
        // of model roles whose candidates cannot share the same denominator.
        let models: Vec<_> = self
            .pack
            .manifest
            .pipeline
            .iter()
            .filter_map(|step| {
                if let PipelineStep::Model { role, .. } = step {
                    Some(role.as_deref())
                } else {
                    None
                }
            })
            .collect();
        if models != [Some("obligations")] || self.pack.manifest.inputs.len() != 1 {
            return Err(
                "corpus diagnostics require one obligations model step and one input role".into(),
            );
        }
        for id in self.bindings.keys() {
            if corpus.case(id).is_none() {
                return Err(format!("input binding names unknown corpus case {id}"));
            }
        }
        let pipeline_digest = super::resume::pipeline_identity(self.pack)?;
        let executable_digest =
            super::resume::file_identity(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        if let Some(parent) = self.output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir(self.output)
            .map_err(|e| format!("use a new output directory {}: {e}", self.output.display()))?;
        std::fs::write(self.output.join("corpus.json"), self.corpus_text)
            .map_err(|e| e.to_string())?;
        if let Some(record) = &challenge {
            std::fs::write(
                self.output.join("challenge-lifecycle.json"),
                serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        let selection = corpus
            .selection
            .as_ref()
            .map(|s| Selection {
                fields: s.fields.clone(),
            })
            .unwrap_or_else(Selection::letter_pack);
        let mut cases = Vec::new();
        for (index, case) in corpus.cases.iter().enumerate() {
            let disk = RunDir::create(self.output, &format!("case-{index:04}"))
                .map_err(|e| e.to_string())?;
            let inputs = match self.bindings.get(&case.id) {
                Some(inputs) => inputs.clone(),
                None => {
                    let path = disk.path.join("source.txt");
                    std::fs::write(&path, case.passages.join("\n\n")).map_err(|e| e.to_string())?;
                    vec![(self.pack.manifest.inputs[0].role.clone(), path)]
                }
            };
            disk.record_model(self.model.as_ref())
                .map_err(|e| e.to_string())?;
            disk.record_inputs(&inputs.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>())
                .map_err(|e| e.to_string())?;
            let identity = inputs
                .iter()
                .map(|(role, path)| {
                    Ok(InputIdentity {
                        role: role.clone(),
                        file: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        digest: super::resume::file_identity(path)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let log = Capture {
                disk: Some(&disk),
                ..Capture::default()
            };
            let bound: Vec<_> = inputs
                .iter()
                .map(|(role, path)| (role.as_str(), path.clone()))
                .collect();
            let result = crate::run::run_pack_bound_with_resources(
                self.pack,
                &bound,
                self.answers,
                RunResources {
                    pdfium_dir: self.pdfium_dir,
                },
                &AtomicBool::new(false),
                &mut |_| {},
                &log,
            );
            let segments = log.segments.into_inner();
            let (mapped, passage_map, acquisition_errors) = map_case(case, &segments);
            let mut report = CaseReport {
                case: case.id.clone(),
                coverage: case.coverage.clone(),
                inputs: identity,
                segments,
                passage_map,
                acquisition_errors,
                kind_matched_segments: Default::default(),
                attribution_errors: Vec::new(),
                execution_error: None,
                raw: Proposal {
                    case: case.id.clone(),
                    asks: Vec::new(),
                },
                verified: Vec::new(),
                diagnostics: Vec::new(),
                score: None,
                exchanges: log.exchanges.into_inner(),
                claims: Vec::new(),
            };
            match result {
                Err(error) => report.execution_error = Some(error.to_string()),
                Ok(outcome) => {
                    report.claims = outcome.claim_traces;
                    (report.raw, report.diagnostics) = proposals(case, &report.claims);
                    match outcome.payload {
                        Payload::Extraction(extraction) => {
                            for obligation in extraction.obligations {
                                let mut shown = VerifiedAsk::from(&obligation);
                                // Ordinals restart per document. Resolve the evidence's
                                // full coordinate, never assume document zero.
                                shown.passage = obligation
                                    .evidence
                                    .first()
                                    .and_then(|e| {
                                        report.segments.iter().position(|s| {
                                            (s.document, s.page, s.ordinal)
                                                == (e.document, e.page, e.ordinal)
                                        })
                                    })
                                    .unwrap_or(usize::MAX);
                                report.verified.push(shown);
                            }
                            if report.acquisition_errors.is_empty() {
                                (report.kind_matched_segments, report.attribution_errors) =
                                    attribution(case, &mapped, &report.raw, &report.verified);
                            }
                            if report.acquisition_errors.is_empty()
                                && report.attribution_errors.is_empty()
                            {
                                report.score = Some(corpus::score_case_with_attribution(
                                    &corpus,
                                    &mapped,
                                    &report.raw,
                                    &report.verified,
                                    &selection,
                                    &report.kind_matched_segments,
                                ));
                            }
                        }
                        _ => {
                            report.execution_error =
                                Some("pipeline did not produce extraction output".into())
                        }
                    }
                }
            }
            // Recording is required here even though RunLog itself is best effort.
            // Read every exchange back so a full disk cannot silently yield a
            // purportedly replayable measurement.
            if report.execution_error.is_none() && !report.exchanges.is_empty() {
                let recorded = super::replay::Recording::from_run_dirs(&disk.path)?;
                for exchange in &report.exchanges {
                    let schema = exchange
                        .generation
                        .as_ref()
                        .and_then(|g| g.payload.pointer("/response_format/json_schema/schema"))
                        .ok_or("corpus diagnostics require exact generation recordings")?;
                    if recorded.answer_for(&exchange.request, schema).as_deref()
                        != Some(&exchange.response)
                    {
                        return Err("recording did not retain the diagnostic exchange".into());
                    }
                }
            }
            disk.write_output(
                "corpus-case.json",
                &serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            cases.push(report);
        }
        let scores: Vec<_> = cases.iter().filter_map(|c| c.score.clone()).collect();
        let report = Report {
            schema: SCHEMA.into(),
            scoring: SCORING.into(),
            corpus_digest: format!(
                "blake3:{}",
                blake3::hash(self.corpus_text.as_bytes()).to_hex()
            ),
            answer_source: match self.answers {
                Answers::WithoutModel => "deterministic-floor",
                Answers::FromModel(e) if e.replay_compatibility().is_some() => "replay",
                Answers::FromModel(_) if self.model.is_some() => "model",
                Answers::FromModel(_) => "controlled-endpoint",
            }
            .into(),
            slice: corpus.slice,
            pack: self.pack.manifest.id.clone(),
            pipeline_digest,
            executable_digest,
            fields: selection.fields.into_iter().collect(),
            selection: corpus.selection.clone(),
            challenge,
            model: self.model.clone(),
            scoring_machine: self.machine.clone(),
            generation_machine: self.generation_machine.clone(),
            sidecar: self.sidecar.clone(),
            runtime: self.runtime.clone(),
            replay: match self.answers {
                Answers::FromModel(endpoint) => endpoint.replay_compatibility(),
                _ => None,
            },
            unscored_cases: cases.len() - scores.len(),
            summary: corpus::summarise(&scores),
            cases,
        };
        std::fs::write(
            self.output.join("report.json"),
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(report)
    }
}

/// Align the complete ordered token stream, independently of reader paragraph
/// boundaries. Offsets are only for attribution after execution, never input
/// repair. Added, lost or reordered words cannot acquire a fabricated score.
fn map_case(case: &Case, segments: &[Segment]) -> (Case, BTreeMap<usize, Vec<usize>>, Vec<String>) {
    fn ranges(parts: &[String]) -> (String, Vec<std::ops::Range<usize>>) {
        let mut text = String::new();
        let mut ranges = Vec::new();
        for part in parts {
            let normalised = part.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() && !normalised.is_empty() {
                text.push(' ');
            }
            let start = text.len();
            text.push_str(&normalised);
            ranges.push(start..text.len());
        }
        (text, ranges)
    }
    let actual: Vec<_> = segments.iter().map(|s| s.text.clone()).collect();
    let (source_words, source_ranges) = ranges(&case.passages);
    let (acquired_words, acquired_ranges) = ranges(&actual);
    let mut mapped = case.clone();
    mapped.passages = actual;
    let mut errors = Vec::new();
    let mut mapping = BTreeMap::new();
    if source_words != acquired_words {
        errors.push("acquisition differs from the complete ordered source: missing, reordered or text outside the authored passages".into());
        return (mapped, mapping, errors);
    }
    for (i, source) in source_ranges.iter().enumerate() {
        let ids = acquired_ranges
            .iter()
            .enumerate()
            .filter(|(_, acquired)| source.start < acquired.end && acquired.start < source.end)
            .map(|(id, _)| id)
            .collect();
        mapping.insert(i, ids);
    }
    // A reading's source occurrence must fit inside one acquired segment.
    // Locate it in the authored passage, not elsewhere in the document where
    // an identical value could be a different event or a distractor.
    let location = |at: usize, words: &str| -> Option<usize> {
        let source = source_ranges.get(at)?;
        let wanted = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if wanted.is_empty() {
            return None;
        }
        let mut found = std::collections::BTreeSet::new();
        for (offset, _) in source_words[source.clone()].match_indices(&wanted) {
            let start = source.start + offset;
            let end = start + wanted.len();
            for (id, range) in acquired_ranges.iter().enumerate() {
                if range.start <= start && end <= range.end {
                    found.insert(id);
                }
            }
        }
        if found.len() == 1 {
            found.first().copied()
        } else {
            None
        }
    };
    for span in &mut mapped.spans {
        span.passage = location(span.passage, &span.text).unwrap_or_else(|| {
            errors.push(format!(
                "fact {} does not map to one acquired segment",
                span.fact
            ));
            usize::MAX
        });
    }
    for ask in &mut mapped.asks {
        if let Some(reading) = &mut ask.deadline_reading {
            reading.at = location(reading.at, &reading.value).unwrap_or_else(|| {
                errors.push(format!(
                    "ask {} reading contract does not map to one acquired segment",
                    ask.id
                ));
                usize::MAX
            });
        }
        if let Some(at) = ask.deadline_at {
            ask.deadline_at = Some(
                ask.deadline_words
                    .as_deref()
                    .filter(|w| !w.is_empty())
                    .and_then(|words| location(at, words))
                    .or_else(|| {
                        mapping
                            .get(&at)
                            .and_then(|ids: &Vec<usize>| match ids.as_slice() {
                                [id] => Some(*id),
                                _ => None,
                            })
                    })
                    .unwrap_or_else(|| {
                        errors.push(format!(
                            "ask {} deadline words do not map to one acquired segment",
                            ask.id
                        ));
                        usize::MAX
                    }),
            );
        }
        ask.passage = match mapping.get(&ask.passage).map(Vec::as_slice) {
            Some([id]) => *id,
            _ => {
                errors.push(format!(
                    "ask {} does not map to one acquired segment",
                    ask.id
                ));
                usize::MAX
            }
        };
    }
    (mapped, mapping, errors)
}

/// A reader can merge a valid ask and a negative site. Only distinct declared
/// kinds provide an independent discriminator on the current wire contract.
/// Never choose a pairing because its money/date happens to score better.
fn attribution(
    case: &Case,
    mapped: &Case,
    raw: &Proposal,
    verified: &[VerifiedAsk],
) -> (std::collections::BTreeSet<usize>, Vec<String>) {
    let mut sites: BTreeMap<usize, Vec<(&corpus::Ask, &corpus::Ask)>> = BTreeMap::new();
    for (source, acquired) in case.asks.iter().zip(&mapped.asks) {
        sites
            .entry(acquired.passage)
            .or_default()
            .push((source, acquired));
    }
    let mut kind_only = std::collections::BTreeSet::new();
    let mut errors = Vec::new();
    for (segment, asks) in sites {
        let original: std::collections::BTreeSet<_> = asks.iter().map(|(a, _)| a.passage).collect();
        if original.len() <= 1 {
            continue;
        }
        kind_only.insert(segment);
        let kinds: std::collections::BTreeSet<_> =
            asks.iter().map(|(a, _)| a.kind.as_str()).collect();
        if kinds.len() != asks.len()
            || asks
                .iter()
                .any(|(a, _)| a.status == corpus::AskStatus::NoObligation && a.kind == "other")
        {
            errors.push(format!(
                "segment {segment}: merged ask sites have overlapping or unspecified kinds"
            ));
            continue;
        }
        for (label, candidates) in [
            (
                "raw",
                raw.asks
                    .iter()
                    .filter(|a| a.passage == segment)
                    .map(|a| a.kind.as_str())
                    .collect::<Vec<_>>(),
            ),
            (
                "verified",
                verified
                    .iter()
                    .filter(|a| a.passage == segment)
                    .map(|a| a.kind.as_str())
                    .collect::<Vec<_>>(),
            ),
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for kind in candidates {
                if !kinds.contains(kind) || !seen.insert(kind) {
                    errors.push(format!("segment {segment}: {label} candidates do not identify distinct declared ask kinds"));
                    break;
                }
            }
        }
    }
    (kind_only, errors)
}

#[derive(Deserialize)]
struct Decision {
    confidence: String,
    obligations: Vec<WireAsk>,
}
#[derive(Deserialize)]
struct WireAsk {
    #[serde(default)]
    ask: Option<String>,
    kind: String,
    party: Reading,
    deadline: Deadline,
    amount: Reading,
}
#[derive(Deserialize)]
struct Deadline {
    at: usize,
    value: String,
    #[serde(default)]
    read: Option<When>,
    #[serde(default)]
    from: Option<Reading>,
}

fn proposals(case: &Case, claims: &[ClaimTrace]) -> (Proposal, Vec<Diagnostic>) {
    let mut proposal = Proposal {
        case: case.id.clone(),
        asks: Vec::new(),
    };
    let mut diagnostics = Vec::new();
    for claim in claims.iter().filter(|c| c.kind == ClaimKind::Decision) {
        let paired = claim.check(Guardrail::Pairing) == Some(CheckOutcome::Passed);
        let schema = claim.check(Guardrail::Schema) == Some(CheckOutcome::Passed);
        if !paired || !schema {
            diagnostics.push(Diagnostic {
                trace: claim.id.clone(),
                reason: "unpaired, malformed or absent decision; inspect candidate and attempts"
                    .into(),
            });
        }
        match serde_json::from_value::<Decision>(claim.candidate.clone()) {
            Ok(decision) => {
                for ask in decision.obligations {
                    proposal.asks.push(ProposedAsk {
                        text: ask.ask,
                        passage: if paired { claim.item } else { usize::MAX },
                        kind: ask.kind,
                        party: ask.party,
                        deadline: Reading::new(ask.deadline.at, ask.deadline.value),
                        read: ask.deadline.read,
                        from: ask.deadline.from,
                        amount: ask.amount,
                        time: None,
                        place: None,
                        reference: None,
                        confidence: decision.confidence.clone(),
                    });
                }
            }
            Err(error) if paired && schema => diagnostics.push(Diagnostic {
                trace: claim.id.clone(),
                reason: format!("unsupported raw answer shape: {error}"),
            }),
            Err(_) => {}
        }
    }
    (proposal, diagnostics)
}
