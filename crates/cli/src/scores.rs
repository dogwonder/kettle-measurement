//! `kettle scores` — the public, build-time projection of eval reports
//! (#511).
//!
//! The website never owns a score. It asks this command to read the
//! committed baselines through the same Rust shapes that wrote them,
//! then receives a smaller document containing only the figures and
//! provenance a public score card can support.

use crate::eval::baseline;
use runner::claim_trace::Guardrail;
use runner::eval::{
    CalibrationReport, EvalMetric, EvalReport, MachineInfo, ModelInfo, RuntimePolicy, SidecarInfo,
    Verdict,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Broken = 2,
}

#[derive(Debug)]
pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

#[derive(Debug, Serialize)]
struct PublicScores {
    schema: &'static str,
    current_scoring_version: u32,
    measurements: Vec<PublicScore>,
}

#[derive(Debug, Serialize)]
struct PublicScore {
    state: MeasurementState,
    state_reason: Option<String>,
    source: String,
    recorded_at: String,
    pack: String,
    pack_version: String,
    eval_set: runner::eval::fixture::EvalSet,
    scoring_version: u32,
    bed: Option<String>,
    policy: PublicPolicy,
    machine: MachineInfo,
    sidecar: Option<SidecarInfo>,
    runtime: Option<RuntimePolicy>,
    verdict: Verdict,
    scores: PublicScoreValues,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum MeasurementState {
    Current,
    Historical,
    Incomplete,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicPolicy {
    Model {
        model: ModelInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    DeterministicFloor,
}

#[derive(Debug, Serialize)]
struct PublicScoreValues {
    end_to_end: f32,
    review_rate: f32,
    candidates: usize,
    contained: usize,
    escaped: usize,
    pipeline_introduced: usize,
    wall_ms: u64,
    peak_rss_mb: u64,
    tokens_per_second: f32,
    guardrails: BTreeMap<Guardrail, PublicGuardrail>,
    confidence: BTreeMap<EvalMetric, CalibrationReport>,
}

#[derive(Debug, Default, Serialize)]
struct PublicGuardrail {
    failed: usize,
    contained: usize,
    escaped: usize,
}

#[derive(Debug, Deserialize)]
struct ModelManifestEntry {
    file_name: String,
    sha256: String,
}

pub fn run_json(root: &Path, current_scoring_version: u32) -> Outcome {
    match project(root, current_scoring_version) {
        Ok(scores) => Outcome {
            text: serde_json::to_string_pretty(&scores).expect("public scores serialise") + "\n",
            code: ExitCode::Ok,
        },
        Err(message) => Outcome {
            text: format!("{message}\n"),
            code: ExitCode::Broken,
        },
    }
}

pub fn run(root: &Path, current_scoring_version: u32) -> Outcome {
    let projected = match project(root, current_scoring_version) {
        Ok(scores) => scores,
        Err(message) => {
            return Outcome {
                text: format!("{message}\n"),
                code: ExitCode::Broken,
            }
        }
    };
    let current = projected
        .measurements
        .iter()
        .filter(|row| matches!(row.state, MeasurementState::Current))
        .count();
    let historical = projected.measurements.len() - current;
    Outcome {
        text: format!(
            "{current} current score-card row(s); {historical} historical or incomplete row(s).\n"
        ),
        code: ExitCode::Ok,
    }
}

fn project(root: &Path, current_scoring_version: u32) -> Result<PublicScores, String> {
    let digests = model_digests(root)?;
    let mut measurements = Vec::new();
    for path in baseline_paths(root)? {
        let baseline = baseline::read(&path)?;
        let scoring_version = baseline.scoring_version.ok_or_else(|| {
            format!(
                "The baseline {} predates scoring versions, so its figures cannot be published.",
                path.display()
            )
        })?;
        let recorded_at = baseline.recorded_at.ok_or_else(|| {
            format!(
                "The baseline {} has no measurement date, so its figures cannot be published.",
                path.display()
            )
        })?;
        let source = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for report in baseline.reports {
            measurements.push(public_score(
                report,
                scoring_version,
                current_scoring_version,
                &recorded_at.to_rfc3339(),
                &source,
                &digests,
            ));
        }
    }
    measurements.sort_by(|a, b| {
        a.pack
            .cmp(&b.pack)
            .then(a.scoring_version.cmp(&b.scoring_version).reverse())
            .then(a.recorded_at.cmp(&b.recorded_at).reverse())
            .then(policy_name(&a.policy).cmp(policy_name(&b.policy)))
    });
    Ok(PublicScores {
        schema: "kettle/public-scores@0",
        current_scoring_version,
        measurements,
    })
}

/// What a committed eval report may be called, and why there are two.
///
/// `baseline` is a report something is guarded against. `measured` is a
/// report nobody adopted — the 14 August subscription run under scoring
/// 14 failed, so no baseline came from it, and while this list held one
/// prefix that failure could not be published at all. The public page
/// then showed the pack's July record as merely "historical", which
/// reads as evidence gone stale rather than a current measurement that
/// went badly.
///
/// A report is published because it exists. Adoption is a separate
/// question and belongs in the file's own words, not in its name
/// deciding whether anyone may see it.
const EVIDENCE_PREFIXES: [&str; 2] = ["baseline", "measured"];

fn baseline_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let evals = root.join("evals");
    let entries = match std::fs::read_dir(&evals) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read {}: {e}", evals.display())),
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        EVIDENCE_PREFIXES
                            .iter()
                            .any(|prefix| name.starts_with(prefix))
                    })
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn model_digests(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = root.join("app/src-tauri/models.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read the model manifest {}: {e}", path.display()))?;
    let entries: Vec<ModelManifestEntry> = serde_json::from_str(&text).map_err(|e| {
        format!(
            "Could not make sense of the model manifest {}: {e}",
            path.display()
        )
    })?;
    Ok(entries
        .into_iter()
        .map(|entry| (entry.file_name, entry.sha256))
        .collect())
}

fn public_score(
    report: EvalReport,
    scoring_version: u32,
    current_scoring_version: u32,
    recorded_at: &str,
    source: &str,
    digests: &BTreeMap<String, String>,
) -> PublicScore {
    let policy = match &report.model {
        Some(model) => PublicPolicy::Model {
            model: model.clone(),
            sha256: digests.get(&model.file).cloned(),
        },
        None => PublicPolicy::DeterministicFloor,
    };
    let (state, state_reason) =
        measurement_state(&report, &policy, scoring_version, current_scoring_version);
    PublicScore {
        state,
        state_reason,
        source: source.to_owned(),
        recorded_at: recorded_at.to_owned(),
        pack: report.pack.clone(),
        pack_version: report.pack_version.clone(),
        eval_set: report.eval_set,
        scoring_version,
        bed: report.bed.clone(),
        policy,
        machine: report.machine.clone(),
        sidecar: report.sidecar.clone(),
        runtime: report.runtime.clone(),
        verdict: report.verdict,
        scores: score_values(&report),
    }
}

fn measurement_state(
    report: &EvalReport,
    policy: &PublicPolicy,
    scoring_version: u32,
    current_scoring_version: u32,
) -> (MeasurementState, Option<String>) {
    if scoring_version != current_scoring_version {
        return (
            MeasurementState::Historical,
            Some(format!(
                "Scoring version {scoring_version} is not the current version {current_scoring_version}."
            )),
        );
    }
    if report.bed.is_none() {
        return (
            MeasurementState::Incomplete,
            Some("The measurement does not identify the bed it scored.".to_owned()),
        );
    }
    if matches!(policy, PublicPolicy::Model { sha256: None, .. }) {
        return (
            MeasurementState::Incomplete,
            Some("The model file has no digest in the pinned model manifest.".to_owned()),
        );
    }
    if report.model.is_some()
        && (report.runtime.is_none()
            || report.sidecar.is_none()
            || report
                .sidecar
                .as_ref()
                .is_some_and(|sidecar| sidecar.device.is_none()))
    {
        return (
            MeasurementState::Incomplete,
            Some("The measurement does not carry its complete runtime, sidecar and device provenance.".to_owned()),
        );
    }
    (MeasurementState::Current, None)
}

fn score_values(report: &EvalReport) -> PublicScoreValues {
    let has_fixtures = !report.fixtures.is_empty();
    let end_to_end = report
        .fixtures
        .iter()
        .map(|fixture| fixture.end_to_end)
        .reduce(f32::min)
        .unwrap_or(0.0);
    let review_rate = report
        .fixtures
        .iter()
        .map(|fixture| fixture.needs_review_rate)
        .reduce(f32::max)
        .unwrap_or(1.0);
    let mut candidates = 0;
    let mut contained = 0;
    let mut escaped = 0;
    let mut pipeline_introduced = 0;
    let mut wall_ms = 0;
    let mut peak_rss_mb = 0;
    let mut tokens_per_second = f32::MAX;
    let mut guardrails: BTreeMap<Guardrail, PublicGuardrail> = BTreeMap::new();
    for fixture in &report.fixtures {
        candidates += fixture.containment.candidates;
        contained += fixture.containment.contained;
        escaped += fixture.containment.escaped;
        pipeline_introduced += fixture.containment.pipeline_introduced;
        wall_ms = wall_ms.max(fixture.perf.wall_ms);
        peak_rss_mb = peak_rss_mb.max(fixture.perf.peak_rss_mb);
        if fixture.perf.tokens_per_second > 0.0 {
            tokens_per_second = tokens_per_second.min(fixture.perf.tokens_per_second);
        }
        for (guardrail, boundary) in &fixture.containment.by_guardrail {
            let row = guardrails.entry(*guardrail).or_default();
            row.failed += boundary.failed;
            row.contained += boundary.contained;
            row.escaped += boundary.escaped;
        }
    }
    let confidence = report
        .metrics
        .iter()
        .map(|(metric, report)| {
            let calibration = match report {
                runner::eval::MetricReport::Classification(metrics) => &metrics.calibration,
                runner::eval::MetricReport::Extraction(metrics) => &metrics.calibration,
            };
            (*metric, calibration.clone())
        })
        .collect();
    PublicScoreValues {
        end_to_end: if has_fixtures { end_to_end } else { 0.0 },
        review_rate: if has_fixtures { review_rate } else { 1.0 },
        candidates,
        contained,
        escaped,
        pipeline_introduced,
        wall_ms,
        peak_rss_mb,
        tokens_per_second: if tokens_per_second == f32::MAX {
            0.0
        } else {
            tokens_per_second
        },
        guardrails,
        confidence,
    }
}

fn policy_name(policy: &PublicPolicy) -> &str {
    match policy {
        PublicPolicy::Model { model, .. } => &model.file,
        PublicPolicy::DeterministicFloor => "",
    }
}
