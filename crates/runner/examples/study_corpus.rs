//! #431: turn the ten study statements into ten *genuine* reports.
//!
//! Not authored reports. Decided 25 August 2026: a report Kettle would
//! produce is a different artefact from one Kettle did produce, and any
//! error the pipeline makes on its own tells the study more than a
//! corpus with none. So these run the real pipeline — the same
//! `run_pack` the desktop app calls, the same pack, the same model —
//! and whatever comes out is the corpus.
//!
//! The consequence has to be carried, not forgotten: a "clean" report
//! is only clean once somebody has read it against its statement. A
//! participant who catches a natural error in a clean control would
//! otherwise be scored as a false alarm while being right, which
//! inverts the measure the clean pair exists to give. The audit is the
//! next step after this run, and its findings are recorded rather than
//! assumed away.
//!
//! One sidecar for all ten, because ten model loads would be ten times
//! the wait for nothing.
//!
//! Usage:
//!   cargo run -p runner --features pdf --example study_corpus -- \
//!     --model <path.gguf> [--out fixtures/study]

use runner::aggregate::build_report;
use runner::exec::Endpoint;
use runner::results::{DateRange, InputInfo, ModelInfo, PackInfo, RunInfo};
use runner::run::{run_pack, Answers, Progress, RunOutcome};
use runner::run_dir::NoLog;
use runner::sidecar::{binary_in, Sidecar, SidecarRuntime};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const PACK: &str = "app.kttl.subscription-audit";

fn blake3_of(path: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_reader(std::fs::File::open(path).expect("open statement"))
        .expect("read statement");
    hasher.finalize().to_hex().to_string()
}

fn main() {
    let mut argv = std::env::args().skip(1);
    let (mut model, mut out) = (None::<PathBuf>, PathBuf::from("fixtures/study"));
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--model" => model = argv.next().map(PathBuf::from),
            "--out" => out = argv.next().map(PathBuf::from).unwrap_or(out),
            other => panic!("unknown flag {other}"),
        }
    }
    let model = model.expect("--model");

    let pack = runner::packs::load_pack(Path::new("packs").join(PACK).as_path())
        .unwrap_or_else(|e| panic!("could not load {PACK}: {e}"));

    let mut statements: Vec<PathBuf> = std::fs::read_dir(&out)
        .expect("study directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().map(|e| e == "csv").unwrap_or(false)
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with("statement-"))
                    .unwrap_or(false)
        })
        .collect();
    statements.sort();
    println!("{} statements", statements.len());

    let log = std::env::temp_dir().join("study-corpus-sidecar.log");
    let mut sidecar = Sidecar::spawn(
        &binary_in(Path::new("sidecars")),
        &model,
        &log,
        SidecarRuntime::default(),
    )
    .expect("spawn sidecar");
    sidecar
        .wait_until_ready(Duration::from_secs(600))
        .expect("sidecar ready");
    println!(
        "sidecar ready on port {} ({})",
        sidecar.port(),
        sidecar.device().unwrap_or_else(|| "device unknown".into())
    );

    let answers = Answers::FromModel(Endpoint::local(sidecar.port()));
    let cancel = AtomicBool::new(false);
    let model_file = model
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    for path in &statements {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let started = chrono::Utc::now();
        let clock = Instant::now();
        let mut last = String::new();
        let outcome: RunOutcome = match run_pack(
            &pack,
            std::slice::from_ref(path),
            &answers,
            &cancel,
            &mut |progress: Progress| {
                if progress.step != last {
                    last = progress.step.to_owned();
                    print!("\r{stem}: {last}                    ");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            },
            &NoLog,
        ) {
            Ok(outcome) => outcome,
            Err(e) => {
                println!("\n{stem}: run failed ({e:?})");
                continue;
            }
        };

        let run = RunInfo {
            id: stem.replace("statement", "study"),
            pack: PackInfo {
                id: pack.manifest.id.clone(),
                version: pack.manifest.version.clone(),
                title: pack.manifest.name.clone(),
            },
            input: InputInfo {
                file: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                rows: outcome.input.rows,
                period: outcome
                    .input
                    .period
                    .map(|(from, to)| DateRange { from, to })
                    .expect("a statement with no dates would have failed the run"),
                // The same hash the results cache is keyed on, of a file
                // anybody reading the corpus can hash themselves.
                hash: format!("blake3:{}", blake3_of(path)),
            },
            model: ModelInfo {
                tier: "study".to_owned(),
                id: model_file.clone(),
            },
            started: started.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            finished: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            currency: "GBP".to_owned(),
        };

        match build_report(&outcome, run) {
            Ok(report) => {
                let target = out.join(format!("{}.json", stem.replace("statement", "report")));
                std::fs::write(
                    &target,
                    serde_json::to_string_pretty(&report).expect("serialise report"),
                )
                .expect("write report");
                println!(
                    "\r{stem}: {} findings, {} needs-review, {} warnings, {:.0}s        ",
                    report.recurring.len(),
                    report.needs_review.len(),
                    outcome.warnings.len(),
                    clock.elapsed().as_secs_f64(),
                );
            }
            Err(e) => println!("\r{stem}: could not build a report ({e})        "),
        }
    }
}
