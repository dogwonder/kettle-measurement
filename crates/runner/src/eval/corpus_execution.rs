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
pub const SCORING: &str = "corpus-fields-v4";
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
                                report.score = Some(corpus::score_case(
                                    &corpus,
                                    &mapped,
                                    &report.raw,
                                    &report.verified,
                                    &selection,
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

/// Align whole authored passages against ordered runs of acquired segments.
/// The reader can split a sign-off; source order disambiguates its repeated
/// organisation name from the letterhead. More than one complete alignment,
/// missing prose, or a fact spanning segments stays an acquisition limitation.
fn map_case(case: &Case, segments: &[Segment]) -> (Case, BTreeMap<usize, Vec<usize>>, Vec<String>) {
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let actual: Vec<_> = segments.iter().map(|s| squash(&s.text)).collect();
    let candidates: Vec<Vec<(usize, usize)>> = case
        .passages
        .iter()
        .map(|p| {
            let wanted = squash(p);
            let mut ranges = Vec::new();
            for start in 0..actual.len() {
                let mut joined = String::new();
                for (end, part) in actual.iter().enumerate().skip(start) {
                    if end > start {
                        joined.push(' ');
                    }
                    joined.push_str(part);
                    if joined == wanted {
                        ranges.push((start, end + 1));
                    }
                    if joined.len() >= wanted.len() {
                        break;
                    }
                }
            }
            ranges
        })
        .collect();
    // Count complete ordered alignments, capped at two. This avoids greedy
    // matching of repeated passages and exponential search on repeated prose.
    let mut ways = vec![vec![0usize; actual.len() + 1]; candidates.len() + 1];
    ways[candidates.len()].fill(1);
    for i in (0..candidates.len()).rev() {
        for position in 0..=actual.len() {
            ways[i][position] = candidates[i]
                .iter()
                .filter(|(start, _)| *start >= position)
                .fold(0usize, |count, (_, end)| (count + ways[i + 1][*end]).min(2));
        }
    }
    let mut mapping = BTreeMap::new();
    let mut errors = Vec::new();
    if ways[0][0] != 1 {
        errors.push(format!(
            "source passages have {} complete ordered acquisition mappings (2 means multiple)",
            ways[0][0]
        ));
        if ways[0][0] == 0 && squash(&case.passages.join(" ")) == actual.join(" ") {
            errors.push(
                "reader merged authored passage boundaries; per-ask attribution is unavailable"
                    .into(),
            );
        }
    } else {
        let mut position = 0;
        for (i, ranges) in candidates.iter().enumerate() {
            let &(start, end) = ranges
                .iter()
                .find(|(start, end)| *start >= position && ways[i + 1][*end] > 0)
                .expect("one complete alignment");
            mapping.insert(i, (start..end).collect::<Vec<_>>());
            position = end;
        }
        let mapped_segments: std::collections::BTreeSet<_> =
            mapping.values().flatten().copied().collect();
        if mapped_segments.len() != segments.len() {
            errors.push("acquisition contains text outside the authored passages; adjudicate extra rendered prose before scoring".into());
        }
    }
    let mut mapped = case.clone();
    mapped.passages = segments.iter().map(|s| s.text.clone()).collect();
    for span in &mut mapped.spans {
        let found: Vec<_> = mapping
            .get(&span.passage)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| actual[*id].contains(&squash(&span.text)))
            .collect();
        span.passage = if let [id] = found.as_slice() {
            *id
        } else {
            errors.push(format!(
                "fact {} does not map to one acquired segment",
                span.fact
            ));
            usize::MAX
        };
    }
    for ask in &mut mapped.asks {
        if let Some(at) = ask.deadline_at {
            let found: Vec<_> = mapping
                .get(&at)
                .into_iter()
                .flatten()
                .copied()
                .filter(|id| {
                    ask.deadline_words
                        .as_ref()
                        .is_none_or(|words| actual[*id].contains(&squash(words)))
                })
                .collect();
            ask.deadline_at = Some(if let [id] = found.as_slice() {
                *id
            } else {
                errors.push(format!(
                    "ask {} deadline words do not map to one acquired segment",
                    ask.id
                ));
                usize::MAX
            });
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

#[derive(Deserialize)]
struct Decision {
    confidence: String,
    obligations: Vec<WireAsk>,
}
#[derive(Deserialize)]
struct WireAsk {
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
