//! The real [`Evaluator`] (#25): spawn a llama-server on the weights
//! being measured, run the pack's fixtures through it, hand back a
//! scored report.
//!
//! Everything above this — flags, repeats, the baseline check, the
//! table, the exit code — was finished in #38 against a faked
//! evaluator. This file is the only thing that needed a model, and it
//! adds no judgement of its own: the scoring is
//! [`runner::eval::fixture`] and the verdict is the pack's thresholds.
//!
//! One sidecar per request, not one per eval. A repeat is meant to be a
//! stability check, and a server that has already answered the same
//! questions is not the same starting position as a cold one — sharing
//! it would quietly measure something other than what was asked for.

use super::{EvalRequest, Evaluator};
use runner::eval::fixture::{EvalSelection, FixtureEvaluator};
use runner::eval::{EvalReport, MachineInfo, ModelInfo, SidecarInfo};
use runner::exec::Endpoint;
use runner::packs::load_pack;
use runner::packs::Pack;
use runner::run::Answers;
use runner::sidecar::{Sidecar, SidecarRuntime, DEFAULT_CONTEXT};
use std::path::PathBuf;
use std::time::Duration;

/// How long to wait for the weights to load before giving up. Large
/// models on a cold page cache are slow to start, and an eval that
/// abandoned a 7B at thirty seconds would report "could not run" for a
/// model that was merely still reading.
const MODEL_LOAD_TIMEOUT: Duration = Duration::from_secs(300);

// CONTRACT (#251): the context an eval runs at now lives beside the
// flag that carries it, as `runner::sidecar::DEFAULT_CONTEXT`. Two
// copies is what let the reported context and the running one differ.

pub struct SidecarEvaluator {
    /// The llama-server binary to run the weights with.
    pub sidecar_binary: PathBuf,
    /// Where the sidecar's own output is captured, so a model that
    /// refuses to load can be diagnosed after the fact.
    pub log_dir: PathBuf,
    /// Where each fixture's model exchanges are kept, so a score can be
    /// read back as answers rather than only as a number.
    pub runs_dir: PathBuf,
    /// Where already-scored fixtures are cached, so an interrupted
    /// measurement can be picked up rather than lost (#282). `None`
    /// measures everything.
    pub resume_dir: Option<PathBuf>,
    pub machine: MachineInfo,
}

impl SidecarEvaluator {
    /// Which llama-server is about to answer (#74), and what it loaded
    /// on (#490).
    ///
    /// A binary that won't say is recorded as `"unknown"` rather than
    /// stopping the eval or quietly dropping the field. The point of
    /// this is attribution, and "unknown" is a fact someone can see in
    /// the report — where a missing field looks like an older harness
    /// and a failed eval would mean a future llama-server changing its
    /// `--version` wording could stop the harness dead.
    ///
    /// The device stays `Option` where the version does not, because
    /// the two absences read differently downstream: an unknown version
    /// is still a version slot filled in, while an absent device means
    /// "not recorded" — the honest claim for a startup output that
    /// never said, and the same claim every pre-#490 baseline makes.
    fn sidecar_info(&self, device: Option<String>) -> SidecarInfo {
        SidecarInfo {
            version: runner::sidecar::version(&self.sidecar_binary)
                .unwrap_or_else(|_| "unknown".to_owned()),
            file: self
                .sidecar_binary
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".to_owned()),
            device,
        }
    }
}

impl Evaluator for SidecarEvaluator {
    fn evaluate(&self, request: &EvalRequest) -> Result<EvalReport, String> {
        // A replay is answered entirely from an earlier run's record:
        // no sidecar is spawned, no weights are loaded, nothing is
        // asked. The model in the report is the recording's, because
        // that is whose answers these are (#288).
        if let Some(root) = request.replay {
            let pack = runner::packs::load_pack(&request.pack_dir)
                .map_err(|e| format!("{} couldn't be loaded: {e}", request.pack))?;
            let recording = runner::eval::replay::Recording::from_run_dirs(root)?;
            // Whose answers these are, taken from the recording itself
            // (#303). `--replay` names no model on the command line —
            // there is nothing to load — so before run directories
            // recorded it, this was always `None` and every replayed
            // report was labelled "without a model". `baseline::compare`
            // joins on the model, so such a report could never be
            // compared against a live one, which defeated the case
            // replay was built for.
            let model = recording.model().cloned();
            let evaluator = FixtureEvaluator {
                answers: Answers::FromModel(runner::exec::Endpoint::replaying(recording)),
                model,
                machine: self.machine.clone(),
                sidecar: None,
                peak_rss: None,
                fixtures_dir: request.fixture_dir.map(PathBuf::from),
                runs_dir: None,
                resume_dir: None,
            };
            return run_selected(&evaluator, &pack, request.selection);
        }

        let pack = load_pack(&request.pack_dir)
            .map_err(|e| format!("{} couldn't be loaded: {e}", request.pack))?;

        // The floor (#73): nothing to spawn, nothing to load, nothing to
        // wait for. A FixtureEvaluator with Answers::WithoutModel and no
        // ModelInfo, the machine as below, no sidecar recorded — and no
        // log file, because there is no server output to capture.
        let Some(spec) = request.model else {
            let evaluator = FixtureEvaluator {
                answers: Answers::WithoutModel,
                model: None,
                machine: self.machine.clone(),
                sidecar: None,
                peak_rss: None,
                fixtures_dir: request.fixture_dir.map(PathBuf::from),
                runs_dir: Some(self.runs_dir.join(format!("run{}", request.run))),
                resume_dir: self.resume_dir.clone(),
            };
            return run_selected(&evaluator, &pack, request.selection);
        };

        if !spec.path.exists() {
            return Err(format!("there's no model at {}", spec.path.display()));
        }

        std::fs::create_dir_all(&self.log_dir)
            .map_err(|e| format!("couldn't make {}: {e}", self.log_dir.display()))?;
        let log = self.log_dir.join(format!(
            "eval-{}-{}-run{}.log",
            request.pack, spec.file, request.run,
        ));

        // One value, used three times: what the child is told, what the
        // report's model entry claims, and the runtime policy the
        // baseline comparison guards (#251, #232) are all the same
        // expression.
        let runtime = SidecarRuntime {
            context: spec.context.unwrap_or(DEFAULT_CONTEXT),
            ..SidecarRuntime::default()
        };
        let policy = runner::eval::RuntimePolicy::effective(&runtime);
        let mut sidecar = Sidecar::spawn(&self.sidecar_binary, &spec.path, &log, runtime)
            .map_err(|e| format!("couldn't start {}: {e}", self.sidecar_binary.display()))?;
        sidecar.wait_until_ready(MODEL_LOAD_TIMEOUT).map_err(|e| {
            format!(
                "{} never became ready ({e}) — see {}",
                spec.file,
                log.display()
            )
        })?;

        // The spec's own params/quant are used rather than re-read from
        // the file name: a models.toml may have pinned them precisely
        // because the name doesn't say.
        let evaluator = FixtureEvaluator {
            answers: Answers::FromModel(Endpoint::local(sidecar.port())),
            model: Some(ModelInfo {
                file: spec.file.clone(),
                params: spec.params.clone(),
                quant: spec.quant.clone(),
                context: runtime.context,
            }),
            machine: self.machine.clone(),
            // Asked once ready, so the device named is the one the
            // model finished loading on (#490).
            sidecar: Some(self.sidecar_info(sidecar.device())),
            peak_rss: Some(sidecar.peak_rss()),
            fixtures_dir: request.fixture_dir.map(PathBuf::from),
            runs_dir: Some(self.runs_dir.join(format!("run{}", request.run))),
            resume_dir: self.resume_dir.clone(),
        };
        let mut report = run_selected(&evaluator, &pack, request.selection)?;
        // The policy the report claims is the policy the sidecar above
        // was spawned with — one local, never re-derived (#232).
        report.runtime = Some(policy);
        Ok(report)
        // The sidecar is killed on drop (runner::sidecar), so a failed
        // eval never leaves a model resident.
    }
}

/// One place that turns the requested selection into the evaluator
/// call, so the three routes above (replay, floor, sidecar) cannot
/// drift apart on which slice they run.
fn run_selected(
    evaluator: &FixtureEvaluator,
    pack: &Pack,
    selection: EvalSelection,
) -> Result<EvalReport, String> {
    match selection {
        EvalSelection::Development => evaluator.evaluate(pack),
        EvalSelection::Exam => evaluator.evaluate_exam(pack),
        EvalSelection::Audition => evaluator.evaluate_audition(pack),
    }
}
