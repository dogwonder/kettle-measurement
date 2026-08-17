mod doctor;
mod plan;
mod table;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

/// Kettle — runs read-only task packs against your files, locally.
#[derive(Parser)]
#[command(name = "kettle", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the pieces Kettle needs are in place
    Doctor {
        /// Path to the llama-server binary
        #[arg(long, default_value_os_t = default_sidecar_binary())]
        sidecar_binary: PathBuf,
        /// Directory holding model (.gguf) files
        #[arg(long, default_value = "models")]
        models_dir: PathBuf,
        /// Directory holding vendored sidecar libraries
        #[arg(long, default_value = "sidecars")]
        sidecars_dir: PathBuf,
    },
    /// List the task packs Kettle can run, and what each may do
    Packs {
        #[command(subcommand)]
        command: PacksCommand,
    },
    /// Run a pack against your files
    Run {
        /// The pack's id, e.g. app.kttl.subscription-audit
        pack: String,
        /// The files to read
        inputs: Vec<PathBuf>,
        /// Directory holding task packs
        #[arg(long, default_value = "packs")]
        packs_dir: PathBuf,
        /// Show what would happen — the steps, how much asking, whether
        /// the answer is already known — without starting the model
        #[arg(long)]
        dry_run: bool,
    },
    /// Measure how well a model does a pack's job, against its fixtures
    Eval(cli::eval::Options),
    /// Read a statement file and show the transactions Kettle found
    Parse {
        /// The statement to read (CSV recommended; PDF is best effort)
        file: PathBuf,
        /// Directory holding vendored sidecar libraries
        #[arg(long, default_value = "sidecars")]
        sidecars_dir: PathBuf,
    },
    /// Prove a pack's safeguards catch known harms, without a model
    Mutate {
        /// The pack's id, e.g. app.kttl.letter-to-actions
        pack: String,
        /// Directory holding task packs
        #[arg(long, default_value = "packs")]
        packs_dir: PathBuf,
    },
    /// Regenerate a pack's generated eval bed from its committed spec
    Bed {
        /// The pack whose bed to regenerate
        #[arg(long, default_value = "packs/app.kttl.subscription-audit")]
        pack_dir: PathBuf,
        /// Say what would change and write nothing
        #[arg(long)]
        check: bool,
    },
    /// Show what each product-level assurance claim stands on today
    Claims {
        /// The repository root, holding assurance/claims.json
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Render the validated registry as machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Project committed eval reports into public model score cards
    Scores {
        /// The repository root, holding evals/ and the model manifest
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Render the validated projection as machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate the public measurement tree from the declared boundary
    Project {
        /// The repository root to project from
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Write the projected tree here
        #[arg(long, default_value_os_t = cli::project::default_out_dir())]
        out: PathBuf,
        /// Say what would be projected and write nothing
        #[arg(long)]
        check: bool,
    },
    /// Render a saved results file through a report template
    Render {
        /// A completed run's results.json
        results: PathBuf,
        /// The pack's report.html.tera
        template: PathBuf,
        /// Write the report here instead of standard output
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PacksCommand {
    /// One line per pack: id, name, version, inputs, capabilities
    List {
        /// Directory holding task packs
        #[arg(long, default_value = "packs")]
        packs_dir: PathBuf,
        /// Emit the public projection the website builds from (#478).
        /// A pack that will not load is a refusal here, not a skipped
        /// line: a public page that quietly drops one misdescribes the
        /// product.
        #[arg(long)]
        json: bool,
    },
}

/// Vendored binary for the current platform, per sidecars/README.md.
fn default_sidecar_binary() -> PathBuf {
    runner::sidecar::binary_in(Path::new("sidecars"))
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Doctor {
            sidecar_binary,
            models_dir,
            sidecars_dir,
        }) => {
            let report = doctor::run_checks(&sidecar_binary, &models_dir, &sidecars_dir);
            for line in &report.lines {
                println!("{line}");
            }
            if report.all_ok {
                println!("\nAll present and correct.");
            } else {
                println!("\nSomething needs sorting before Kettle can run — see above.");
                std::process::exit(1);
            }
        }
        Some(Command::Packs {
            command: PacksCommand::List { packs_dir, json },
        }) => {
            if json {
                let outcome = cli::packs::run_json(&packs_dir);
                match outcome.code {
                    cli::packs::ExitCode::Ok => println!("{}", outcome.text),
                    cli::packs::ExitCode::Broken => {
                        eprintln!("{}", outcome.text);
                        std::process::exit(cli::packs::ExitCode::Broken as i32);
                    }
                }
            } else {
                let packs = load_all_packs(&packs_dir);
                print!("{}", plan::list_packs(&packs));
            }
        }
        Some(Command::Run {
            pack,
            inputs,
            packs_dir,
            dry_run,
        }) => {
            if !dry_run {
                println!(
                    "Running a pack for real isn't wired up yet — try `kettle run {pack} \
                     ... --dry-run` to see what it would do."
                );
                return;
            }

            let dir = packs_dir.join(&pack);
            let loaded = match runner::packs::load_pack(&dir) {
                Ok(loaded) => loaded,
                Err(e) => {
                    eprintln!("Could not load pack {pack}: {e}");
                    std::process::exit(1);
                }
            };

            // No model has been chosen yet (that arrives with the real
            // run in #46) — the recommended tier stands in for it here,
            // so the cache lookup at least reflects the pack asking.
            let model_id = &loaded.manifest.model.recommended_tier;
            let cached = runner::cache::cache_key(
                &loaded.manifest.id,
                &loaded.manifest.version,
                &inputs,
                model_id,
            )
            .ok()
            .and_then(|key| {
                let hit = runner::cache::Cache::new("cache").lookup(&key).is_some();
                hit.then_some(key)
            });

            print!("{}", plan::dry_run(&loaded, &inputs, cached.as_ref()));
        }
        Some(Command::Eval(options)) => {
            // Every flag, the baseline check and the table are finished;
            // the measuring itself is #25, which fills the seam.
            let evaluator = cli::eval::sidecar_evaluator::SidecarEvaluator {
                sidecar_binary: default_sidecar_binary(),
                // Where each of these lives, and why, is #293's rule:
                // `cli::eval`'s three functions, so the answer sits
                // somewhere a test can read it.
                log_dir: cli::eval::default_log_dir(),
                runs_dir: cli::eval::default_runs_dir(),
                resume_dir: options.resume.then(cli::eval::default_resume_dir),
                machine: cli::eval::machine::detect(),
            };
            let outcome = cli::eval::run(&options, &evaluator, now());
            print!("{}", outcome.text);
            if outcome.code != cli::eval::ExitCode::Ok {
                std::process::exit(outcome.code.as_i32());
            }
        }
        Some(Command::Parse { file, sidecars_dir }) => {
            match runner::parse::parse_input_file(&file, Some(&sidecars_dir)) {
                Ok(parsed) => print!("{}", table::render(&parsed)),
                Err(e) => {
                    eprintln!("Could not read {}: {e}", file.display());
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Mutate { pack, packs_dir }) => {
            let outcome = cli::mutate::run(&cli::mutate::Options {
                pack,
                packs_dir,
                pack_dir: None,
            });
            println!("{}", outcome.text);
            if outcome.code != cli::mutate::ExitCode::Ok {
                std::process::exit(outcome.code as i32);
            }
        }
        Some(Command::Bed { pack_dir, check }) => {
            let outcome = cli::bed::run(&pack_dir, check);
            print!("{}", outcome.text);
            if !outcome.ok {
                std::process::exit(1);
            }
        }
        Some(Command::Claims { root, json }) => {
            let outcome = if json {
                cli::claims::run_json(&root, runner::eval::SCORING_VERSION, now().date_naive())
            } else {
                cli::claims::run(&root, runner::eval::SCORING_VERSION, now().date_naive())
            };
            print!("{}", outcome.text);
            if outcome.code != cli::claims::ExitCode::Ok {
                std::process::exit(outcome.code as i32);
            }
        }
        Some(Command::Scores { root, json }) => {
            let outcome = if json {
                cli::scores::run_json(&root, runner::eval::SCORING_VERSION)
            } else {
                cli::scores::run(&root, runner::eval::SCORING_VERSION)
            };
            print!("{}", outcome.text);
            if outcome.code != cli::scores::ExitCode::Ok {
                std::process::exit(outcome.code as i32);
            }
        }
        Some(Command::Project { root, out, check }) => {
            // The tracked listing is read here, at the edge, and passed
            // in — so the selection itself is a pure function a test can
            // hold still, and the one place that shells out to git is
            // the one place a reader has to check.
            let tracked = match cli::project::tracked_files(&root) {
                Ok(tracked) => tracked,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(cli::project::ExitCode::Broken as i32);
                }
            };
            let revision = match cli::project::revision(&root) {
                Ok(revision) => revision,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(cli::project::ExitCode::Broken as i32);
                }
            };
            let provenance = cli::project::Provenance {
                revision,
                generated: now().date_naive(),
            };
            let outcome = cli::project::run(
                &root,
                &tracked,
                (!check).then_some(out.as_path()),
                &provenance,
            );
            print!("{}", outcome.text);
            if outcome.code != cli::project::ExitCode::Ok {
                std::process::exit(outcome.code as i32);
            }
        }
        Some(Command::Render {
            results,
            template,
            output,
        }) => match output {
            Some(output) => match cli::render::write_report(&results, &template, &output) {
                Ok(()) => println!("Wrote {}", output.display()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            },
            None => match cli::render::report_from_files(&results, &template) {
                Ok(rendered) => print!("{rendered}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            },
        },
        None => println!("Nothing to run yet — try `kettle --help` to see the available commands."),
    }
}

/// Every pack in `packs_dir`, in directory order. A pack that fails to
/// load is reported and skipped rather than stopping the whole listing
/// — one broken pack shouldn't hide the ones that work.
fn load_all_packs(packs_dir: &PathBuf) -> Vec<runner::packs::Pack> {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(packs_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(e) => {
            eprintln!("Could not read {}: {e}", packs_dir.display());
            return Vec::new();
        }
    };
    entries.sort();

    let mut packs = Vec::new();
    for dir in entries {
        match runner::packs::load_pack(&dir) {
            Ok(pack) => packs.push(pack),
            Err(e) => eprintln!("Skipping {}: {e}", dir.display()),
        }
    }
    packs
}

/// The clock, read once at the edge (#84).
///
/// Everything downstream takes the time as an argument, so a test can
/// pin it and a pure function never reaches for a clock mid-calculation
/// — the same posture `today` has throughout the runner. `chrono` is
/// built here without its `clock` feature, so the reading comes from
/// `std` and is converted.
fn now() -> chrono::DateTime<chrono::Utc> {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is set after 1970");
    chrono::DateTime::from_timestamp(since_epoch.as_secs() as i64, 0)
        .expect("a timestamp the system clock can produce is in range")
}
