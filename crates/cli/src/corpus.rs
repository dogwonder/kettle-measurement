//! Bounded corpus diagnostics; never a pack ceiling or a tier promotion.
use runner::eval::corpus_execution::{Bindings, Evaluation, Report};
use runner::eval::{corpus::Corpus, replay::Recording, resume, ModelInfo};
use runner::exec::Endpoint;
use runner::run::Answers;
use runner::sidecar::{Sidecar, SidecarRuntime, DEFAULT_CONTEXT};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, clap::Args)]
pub struct Options {
    /// Authored corpus slice; every case is reported
    #[arg(long, default_value = "evals/corpus/slice-01.json")]
    pub corpus: PathBuf,
    /// Inventory snapshot that linked diagnostic cases must match
    #[arg(long, default_value = "evals/capabilities")]
    pub inventory_dir: PathBuf,
    #[arg(long, default_value = "packs/app.kttl.letter-to-actions")]
    pub pack_dir: PathBuf,
    /// New durable directory for report, source snapshot and exchanges
    #[arg(long)]
    pub out: PathBuf,
    /// JSON mapping case ids to {"inputs":{"role":"file or list"}}
    #[arg(long)]
    pub bindings: Option<PathBuf>,
    /// Explicit challenge attempt: consume this frozen lifecycle before execution
    #[arg(long)]
    pub challenge_record: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["replay", "mock_port", "no_model"])]
    pub model: Option<PathBuf>,
    /// Exact recorded answers from a previous corpus output directory
    #[arg(long, conflicts_with_all = ["model", "mock_port", "no_model"])]
    pub replay: Option<PathBuf>,
    /// Controlled loopback test endpoint; never labelled a model measurement
    #[arg(long, conflicts_with_all = ["model", "replay", "no_model"])]
    pub mock_port: Option<u16>,
    /// Run the pipeline's deterministic floor
    #[arg(long, conflicts_with_all = ["model", "replay", "mock_port"])]
    pub no_model: bool,
    #[arg(long, default_value = "sidecars")]
    pub sidecars_dir: PathBuf,
    #[arg(long)]
    pub sidecar_binary: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_CONTEXT)]
    pub context: u32,
}

pub fn run(options: &Options) -> Result<Report, String> {
    let sources = usize::from(options.model.is_some())
        + usize::from(options.replay.is_some())
        + usize::from(options.mock_port.is_some())
        + usize::from(options.no_model);
    if sources != 1 {
        return Err("choose one of --model, --replay, --mock-port or --no-model".into());
    }
    if options.out.exists() {
        return Err("use a new --out directory; existing evidence is never replaced".into());
    }
    let text = std::fs::read_to_string(&options.corpus).map_err(|e| e.to_string())?;
    let corpus = if options.challenge_record.is_some() {
        runner::eval::challenge::validate(&text)?
    } else {
        Corpus::parse(&text)?
    };
    corpus.validate_inventory(&options.inventory_dir)?;
    let pack = runner::packs::load_pack(&options.pack_dir).map_err(|e| e.to_string())?;
    let mut bindings = Bindings::new();
    if let Some(path) = &options.bindings {
        // Reuse the fixture filename/list contract and preserve manifest order.
        let declared: BTreeMap<String, runner::eval::fixture::Expected> =
            serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        for (case, expected) in declared {
            for role in expected.inputs.keys() {
                if !pack.manifest.inputs.iter().any(|i| &i.role == role) {
                    return Err(format!("{case}: unknown input role {role}"));
                }
            }
            let mut files = Vec::new();
            for input in &pack.manifest.inputs {
                if let Some(paths) = expected.inputs.get(&input.role) {
                    files.extend(paths.iter().map(|file| {
                        (
                            input.role.clone(),
                            path.parent()
                                .unwrap_or_else(|| std::path::Path::new("."))
                                .join(file),
                        )
                    }));
                }
            }
            bindings.insert(case, files);
        }
    }
    let mut model = None;
    let machine = crate::eval::machine::detect();
    let mut generation_machine = None;
    let mut sidecar_info = None;
    let mut runtime_identity = None;
    let mut sidecar_guard = None;
    let mut challenge_attempt = None;
    if let Some(record) = &options.challenge_record {
        if options.replay.is_none() {
            let source = if options.model.is_some() {
                "model"
            } else if options.mock_port.is_some() {
                "controlled-endpoint"
            } else {
                "deterministic-floor"
            };
            challenge_attempt = Some(runner::eval::challenge::Attempt::start(
                &text,
                record,
                &options.out,
                source,
            )?);
        }
    }
    let answers = if let Some(root) = &options.replay {
        let recording = Recording::from_run_dirs(root)?;
        if recording.compatibility().legacy_prompt_only_requests != 0 {
            return Err("corpus diagnostics require exact recordings; legacy prompt-only compatibility is insufficient".into());
        }
        model = recording.model().cloned();
        let previous: Report = serde_json::from_str(
            &std::fs::read_to_string(root.join("report.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if previous.schema != runner::eval::corpus_execution::SCHEMA || previous.model != model {
            return Err("recording and corpus report identities disagree".into());
        }
        match (&options.challenge_record, &previous.challenge) {
            (Some(path), Some(record)) => {
                if previous.corpus_digest
                    != format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex())
                {
                    return Err("replay challenge corpus identity differs from its report".into());
                }
                challenge_attempt = Some(runner::eval::challenge::Attempt::replay(
                    &text, path, record,
                )?);
            }
            (None, None) => {}
            _ => return Err(
                "challenge replay requires its original lifecycle and explicit --challenge-record"
                    .into(),
            ),
        }
        runtime_identity = previous.runtime;
        generation_machine = previous.generation_machine;
        sidecar_info = previous.sidecar;
        Answers::FromModel(Endpoint::replaying(recording))
    } else if let Some(port) = options.mock_port {
        Answers::FromModel(Endpoint::local(port))
    } else if let Some(path) = &options.model {
        let spec = crate::eval::models::resolve(Some(path), None)?.remove(0);
        let binary = options
            .sidecar_binary
            .clone()
            .unwrap_or_else(|| runner::sidecar::binary_in(&options.sidecars_dir));
        let runtime = SidecarRuntime {
            context: options.context,
            ..SidecarRuntime::default()
        };
        let identity = resume::RuntimeIdentity::for_sidecar(&binary, &runtime)?;
        model = Some(ModelInfo {
            weights_digest: Some(resume::file_identity(&spec.path)?),
            file: spec.file,
            params: spec.params,
            quant: spec.quant,
            context: runtime.context,
        });
        let log_dir = crate::eval::default_log_dir();
        std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
        let mut sidecar = Sidecar::spawn(
            &binary,
            &spec.path,
            &log_dir.join(format!("corpus-{}.log", std::process::id())),
            runtime,
        )
        .map_err(|e| e.to_string())?;
        sidecar
            .wait_until_ready(Duration::from_secs(300))
            .map_err(|e| e.to_string())?;
        generation_machine = Some(machine.clone());
        sidecar_info = Some(runner::eval::SidecarInfo {
            version: runner::sidecar::version(&binary).unwrap_or_else(|_| "unknown".into()),
            file: binary
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            device: sidecar.device(),
        });
        let endpoint = Endpoint::local(sidecar.port()).with_runtime_identity(identity.clone());
        runtime_identity = Some(identity);
        sidecar_guard = Some(sidecar);
        Answers::FromModel(endpoint)
    } else {
        Answers::WithoutModel
    };
    let evaluation = Evaluation {
        pack: &pack,
        corpus_text: &text,
        bindings: &bindings,
        answers: &answers,
        model,
        machine,
        generation_machine,
        sidecar: sidecar_info,
        runtime: runtime_identity,
        output: &options.out,
        pdfium_dir: Some(&options.sidecars_dir),
    };
    let report = match challenge_attempt {
        Some(attempt) => evaluation.evaluate_challenge(attempt),
        None => evaluation.evaluate(),
    };
    drop(sidecar_guard);
    report
}
