//! `kettle eval` — "is this model good enough for this pack?", asked
//! from a terminal (#38, brief §6).
//!
//! ```text
//! kettle eval app.kttl.subscription-audit --model qwen2.5-3b-q4.gguf
//! kettle eval app.kttl.subscription-audit --models models.toml --runs 3
//! kettle eval --all --models models.toml --baseline evals/baseline.json
//! ```
//!
//! # The seam
//!
//! Everything here is decided without a model in the room: which packs
//! and models to measure, how many times, what the table says, whether
//! the numbers moved against a baseline, and what the process exits
//! with. The measuring itself — loading a pack, running its fixtures
//! through the runner and scoring them — is #25, and sits behind
//! [`Evaluator`]: one call, one pack, one model, one repeat, one
//! [`EvalReport`] back.
//!
//! That split is deliberate. CI runs no model and touches no network
//! (CLAUDE.md), so the half of the harness CI *can* guard has to be
//! reachable without one. The tests drive this module through canned
//! reports; #25 supplies the real implementation and changes nothing
//! here.
//!
//! The verdict is the evaluator's. It is computed from the pack's
//! thresholds by [`runner::eval::FixtureResult::verdict`], and this
//! module reads [`EvalReport::verdict`] without second-guessing it —
//! re-deriving it would mean loading and interpreting pack manifests
//! twice, in two places that could disagree.
//!
//! # Exit codes
//!
//! - `0` — everything ran, and nothing got worse.
//! - `1` — a regression against `--baseline`. The prompt-editing safety
//!   net (CLAUDE.md): this is the code CI keys on.
//! - `2` — the eval could not be run at all: no such pack, unreadable
//!   `models.toml`, a missing baseline file, a model that would not
//!   load. Separated from `1` on purpose, because "the prompt got worse"
//!   and "the harness broke" want different reactions from whoever reads
//!   the build.
//!
//! A `FAIL` verdict with no `--baseline` exits `0`. Evaluating a model
//! that turns out to be too small is a measurement, not an error — the
//! table says `FAIL` and that is the answer you asked for.

pub mod baseline;
pub mod machine;
pub mod models;
pub mod sidecar_evaluator;
pub mod table;
pub mod tiers;

use chrono::{DateTime, Utc};
use runner::eval::fixture::EvalSelection;
use runner::eval::{EvalReport, FixtureResult, Spread, Stability};
use std::path::{Path, PathBuf};

pub use models::ModelSpec;

// ---------------------------------------------------------------------------
// Where a measurement's artefacts live (#293)
//
// One line decides all three: is this thing expensive or irreplaceable,
// or can running again produce it? `target/` is build output — `cargo
// clean` is entitled to delete it, and in a worktree under
// `/private/tmp` the OS may take it without asking. Anything on the
// wrong side of that line is a loss waiting for a routine command.
//
// These are functions rather than a hard-coded literal in `main` so the
// answer is one somewhere a test can read. It could not be, and the
// defect sat unnoticed through the whole of #288's work.

/// Where a run keeps its exchanges — the recording a replay reads.
///
/// Since replay (#288/#289) this is the artefact that turns a
/// 115-minute measurement into a 29-second rescore, and re-recording it
/// costs those 115 minutes again. It defaulted to `target/eval-runs`
/// until #293: the most expensive thing the harness produces, living in
/// the one directory licensed to delete it, while `evals/` — durable,
/// version-controlled — held only the baselines that explain it.
///
/// Gitignored: one 355-fixture recording is 8.1MB, and recordings are
/// not what version control is for.
///
/// There is no flag to override it. #293 assumed a `--runs-dir` and so
/// does a doc comment in `replay.rs`, but no such argument has ever
/// existed — worth knowing before promising it to anybody. Nothing
/// needs it while the default is somewhere durable, which is what
/// wanting it was about.
pub fn default_runs_dir() -> PathBuf {
    PathBuf::from("evals/runs")
}

/// Where an interrupted eval keeps what it has already measured.
///
/// The same class of artefact as a recording and, before #293, in the
/// same place: losing it costs the hours the eval had already spent.
/// #293 did not ask for this one — it is moved because leaving it would
/// be knowingly leaving the trap half-set.
pub fn default_resume_dir() -> PathBuf {
    PathBuf::from("evals/resume")
}

/// Where the model server's own output goes.
///
/// This one stays in `target/`, deliberately. It is stderr from a
/// process, regenerable by running again, and it explains nothing a
/// report cannot say for itself. Keeping the distinction sharp is what
/// makes the rule above meaningful: if everything moved, "expensive or
/// irreplaceable" would have stopped being the reason and become a
/// habit.
pub fn default_log_dir() -> PathBuf {
    PathBuf::from("target/eval-logs")
}

/// What the process should exit with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Everything ran, and nothing got worse.
    Ok,
    /// Something scored worse than the baseline said it did.
    Regression,
    /// The eval could not be run at all.
    CouldNotRun,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        match self {
            ExitCode::Ok => 0,
            ExitCode::Regression => 1,
            ExitCode::CouldNotRun => 2,
        }
    }
}

/// What to print, and what to exit with. Mirrors `doctor`: the command
/// builds its whole answer as text, and `main` decides how to leave.
#[derive(Debug)]
pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

impl Outcome {
    fn problem(message: impl Into<String>) -> Outcome {
        Outcome {
            text: format!("{}\n", message.into()),
            code: ExitCode::CouldNotRun,
        }
    }
}

// ---------------------------------------------------------------------------
// The seam

/// One pack, one model, one repeat — the question #25 answers.
#[derive(Debug)]
pub struct EvalRequest<'a> {
    /// The pack's id, e.g. "app.kttl.subscription-audit".
    pub pack: &'a str,
    /// Where that pack lives, e.g. `packs/app.kttl.subscription-audit`.
    pub pack_dir: PathBuf,
    /// The weights to measure — `None` measures the deterministic
    /// floor instead (#73): the same fixtures, the same scorers, no
    /// model. The evaluator answers with the pass-through semantics of
    /// [`runner::run::Answers::WithoutModel`].
    pub model: Option<&'a ModelSpec>,
    /// Where the fixtures to score against live. `None` means the pack's
    /// own `fixtures/` directory — `--fixture-dir` points it at real
    /// statements instead, which are gitignored and never come into the
    /// repo (CLAUDE.md, data rules).
    pub fixture_dir: Option<&'a Path>,
    /// Which fixture selection to run: development, the sealed exam
    /// set, or the audition subset (#539). One field rather than flags
    /// that can disagree (#334's reasoning).
    pub selection: EvalSelection,
    /// Which repeat this is, 1-based, and how many were asked for.
    /// Present so a real evaluator can label its per-run directory; the
    /// answer for a given repeat should not depend on it.
    pub run: u32,
    pub runs: u32,
    /// Answer from a recording under this directory instead of asking a
    /// model (#288). Verifying a scoring change against answers that
    /// cannot have altered.
    pub replay: Option<&'a Path>,
}

/// Measures one model against one pack. The half of the eval harness
/// that needs a model — supplied by #25, faked in tests.
pub trait Evaluator {
    /// `Err` is a plain-language sentence explaining why this
    /// combination could not be measured at all. It stops the command:
    /// a table with a hole in it is worse than no table.
    fn evaluate(&self, request: &EvalRequest) -> Result<EvalReport, String>;
}

/// The evaluator the shipped binary has until #25 lands: it explains
/// itself and stops. Every flag, the baseline comparison and the table
/// are finished and tested; only the measuring is missing.
pub struct NotWiredUpYet;

impl Evaluator for NotWiredUpYet {
    fn evaluate(&self, request: &EvalRequest) -> Result<EvalReport, String> {
        Err(format!(
            "measuring {} against {} needs the fixture-scoring loop, which isn't wired up \
             yet (#25). Everything around it — the flags, the baseline check and the table \
             — is ready and waiting for it.",
            request
                .model
                .map(|model| model.file.as_str())
                .unwrap_or("the no-model floor"),
            request.pack,
        ))
    }
}

// ---------------------------------------------------------------------------
// Options

/// The flags of `kettle eval`.
#[derive(Debug, clap::Args)]
pub struct Options {
    /// The pack's id, e.g. app.kttl.subscription-audit
    pub pack: Option<String>,
    /// Evaluate every pack in the packs directory
    #[arg(long, conflicts_with = "pack")]
    pub all: bool,
    /// Directory holding task packs
    #[arg(long, default_value = "packs")]
    pub packs_dir: PathBuf,
    /// The model (.gguf) file to measure
    #[arg(long)]
    pub model: Option<PathBuf>,
    /// A models.toml listing several models to measure in turn
    #[arg(long, conflicts_with = "model")]
    pub models: Option<PathBuf>,
    /// Measure the deterministic floor: the same pipeline with no model
    /// at all. Every tier's margin is read against this (#73)
    #[arg(long, conflicts_with_all = ["model", "models"])]
    pub no_model: bool,
    /// How many times to repeat each measurement. Answers at
    /// temperature 0 should not move, so repeats are a stability check
    #[arg(long, default_value_t = 1)]
    pub runs: u32,
    /// Compare against a recorded baseline, and exit non-zero if
    /// anything got worse
    #[arg(long)]
    pub baseline: Option<PathBuf>,
    /// Write what this eval measured to a file, to compare against later.
    /// Refused with `--replay` by `run`, not by clap, so the reason
    /// reaches the terminal
    #[arg(long)]
    pub write_baseline: Option<PathBuf>,
    /// Record what this eval measured in each pack's tiers.json, the file
    /// the model-manager screen quotes. Merges with what is already there.
    /// Refused with `--replay` by `run`, as `--write-baseline` is
    #[arg(long)]
    pub write_tiers: bool,
    /// Score against statements in this directory instead of the pack's
    /// own fixtures
    #[arg(long)]
    pub fixture_dir: Option<PathBuf>,
    /// Measure on a llama-server other than the shipped pin — the lab
    /// build (#539), vendored by `scripts/vendor-sidecar.sh --lab`.
    ///
    /// A lab-build number is a fact about that candidate on that
    /// runtime and nothing more: #74 couples the shipped pin to its
    /// scores, so this result is **not comparable** with a committed
    /// baseline, must not fill `tiers.json`, and cannot back an
    /// assurance claim. The report records which llama-server answered
    /// (`SidecarInfo`), so the recording says so itself rather than
    /// relying on whoever reads it remembering (#303).
    #[arg(long)]
    pub sidecar_binary: Option<PathBuf>,
    /// Run the sealed exam fixtures for a pack-version-bump measurement
    #[arg(long)]
    pub exam: bool,
    /// Run the audition subset (#539): the committed go/no-go bed a
    /// candidate model runs before earning a full bed run. A scheduling
    /// measurement, never a tier claim.
    #[arg(long, conflicts_with = "exam")]
    pub audition: bool,
    /// Reuse fixtures already scored under identical conditions, so an
    /// interrupted measurement is picked up rather than lost (#282).
    ///
    /// Refused with `--runs`: repeats are a stability check, and a
    /// repeat that reused another repeat's answers would measure
    /// nothing and report perfect stability.
    #[arg(long, conflicts_with = "runs")]
    pub resume: bool,
    /// Re-score against answers an earlier run recorded, instead of
    /// asking a model again (#288).
    ///
    /// For verifying a *scoring* change: the model's answers cannot
    /// have altered, so replaying them is the same measurement. A
    /// prompt change is refused — the request is the key, so a
    /// question the recording never heard stops the run.
    #[arg(long, conflicts_with_all = ["resume", "runs", "no_model"])]
    pub replay: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Running

/// Measure every requested pack against every requested model, then
/// report.
///
/// Order is pack by pack (alphabetically), model by model (in the order
/// `models.toml` lists them), repeat by repeat — so a long run reads in
/// the order the table prints, and a run stopped halfway has finished
/// whole packs.
pub fn run(options: &Options, evaluator: &dyn Evaluator, now: DateTime<Utc>) -> Outcome {
    if options.replay.is_some() && (options.write_baseline.is_some() || options.write_tiers) {
        return Outcome::problem(concat!(
            "A replay can re-score recorded answers, but it cannot mint a baseline or a tier: ",
            "a baseline says when and on what it was recorded, and a tier names the machine, ",
            "and the recording does not carry all of the original run provenance ",
            "for either. Stamping this machine and this moment onto somebody else's answers ",
            "would be an invention. Re-run for real to write one, or fix the projection ",
            "so a recording carries its provenance."
        ));
    }
    // A replay needs no weights: the answers are already recorded, and
    // naming a model would claim one had been asked (#288).
    let models = if options.no_model || options.replay.is_some() {
        Vec::new()
    } else {
        match models::resolve(options.model.as_deref(), options.models.as_deref()) {
            Ok(models) => models,
            Err(problem) => return Outcome::problem(problem),
        }
    };
    // What to measure: each model named, or the floor once (#73). One
    // list so the loop below cannot treat the floor as a special case.
    let subjects: Vec<Option<&ModelSpec>> = if options.no_model || options.replay.is_some() {
        vec![None]
    } else {
        models.iter().map(Some).collect()
    };
    let packs = match resolve_packs(options) {
        Ok(packs) => packs,
        Err(problem) => return Outcome::problem(problem),
    };
    if options.runs == 0 {
        return Outcome::problem("--runs 0 would measure nothing — ask for at least one run.");
    }

    // Read the baseline before spending an hour on a model, so a typo in
    // the path costs a second rather than the whole eval — and check its
    // provenance here too (#84), for the same reason: a baseline that
    // cannot honestly be compared should cost a second, not an eval.
    let mut staleness = None;
    let recorded = match options.baseline.as_deref() {
        Some(path) => match baseline::read(path) {
            Ok(recorded) => match recorded.check(path, now) {
                Ok(note) => {
                    staleness = note;
                    Some(recorded.reports)
                }
                Err(problem) => return Outcome::problem(problem),
            },
            Err(problem) => return Outcome::problem(problem),
        },
        None => None,
    };

    let mut text = String::new();
    let mut measured: Vec<EvalReport> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut tiers_written: Vec<String> = Vec::new();

    for pack in &packs {
        let mut for_this_pack: Vec<EvalReport> = Vec::new();
        for model in &subjects {
            let mut repeats = Vec::new();
            for run in 1..=options.runs {
                let request = EvalRequest {
                    pack,
                    pack_dir: options.packs_dir.join(pack),
                    model: *model,
                    fixture_dir: options.fixture_dir.as_deref(),
                    selection: if options.exam {
                        EvalSelection::Exam
                    } else if options.audition {
                        EvalSelection::Audition
                    } else {
                        EvalSelection::Development
                    },
                    replay: options.replay.as_deref(),
                    run,
                    runs: options.runs,
                };
                match evaluator.evaluate(&request) {
                    Ok(report) => repeats.push(report),
                    Err(problem) => return Outcome::problem(format!("Could not run: {problem}")),
                }
            }
            let (report, wobble) = worst_run(repeats);
            if let Some(wobble) = wobble {
                notes.push(wobble);
            }
            for_this_pack.push(report);
        }
        text.push_str(&table::render(
            pack,
            &for_this_pack.iter().collect::<Vec<_>>(),
            options.runs,
        ));
        text.push_str(&baseline::compare_builds(&for_this_pack));
        text.push('\n');

        // Pack by pack, as each finishes: an eval stopped halfway has
        // recorded whole packs, which is the same promise the loop order
        // already makes about the table.
        if options.write_tiers {
            match tiers::write(&options.packs_dir, pack, &for_this_pack, options.runs, now) {
                Ok(path) => {
                    tiers_written.push(format!("Recorded these tiers in {}.", path.display()))
                }
                Err(problem) => return Outcome::problem(problem),
            }
        }

        measured.extend(for_this_pack);
    }

    for note in &notes {
        text.push_str(note);
        text.push('\n');
    }
    for line in &tiers_written {
        text.push_str(line);
        text.push('\n');
    }

    if let Some(path) = options.write_baseline.as_deref() {
        match baseline::write(path, &measured, now) {
            Ok(()) => text.push_str(&format!("Recorded this eval in {}.\n", path.display())),
            Err(problem) => return Outcome::problem(problem),
        }
    }

    let mut code = ExitCode::Ok;
    if let Some(recorded) = recorded {
        let comparison = baseline::compare(&recorded, &measured);
        text.push('\n');
        text.push_str(&comparison.report());
        if let Some(staleness) = staleness {
            text.push_str(&format!("\n{staleness}\n"));
        }
        // A refusal outranks a regression: "worse" is a claim about the
        // model, and a refused comparison has nothing to say about it
        // (#320).
        if comparison.is_refused() {
            code = ExitCode::CouldNotRun;
        } else if comparison.is_regression() {
            code = ExitCode::Regression;
        }
    }

    Outcome { text, code }
}

/// Which packs to measure: the one named, or every pack in the packs
/// directory.
///
/// A pack is a directory with a `pack.json` in it. This is only the
/// "does it exist" check — reading the manifest is the evaluator's job,
/// and a pack that fails to load says so there rather than here.
fn resolve_packs(options: &Options) -> Result<Vec<String>, String> {
    if options.all {
        let mut found = Vec::new();
        let entries = std::fs::read_dir(&options.packs_dir)
            .map_err(|e| format!("Could not read {}: {e}", options.packs_dir.display()))?;
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.join("pack.json").is_file() {
                if let Some(id) = path.file_name().and_then(|name| name.to_str()) {
                    found.push(id.to_owned());
                }
            }
        }
        found.sort();
        if found.is_empty() {
            return Err(format!(
                "No packs in {} — nothing to evaluate.",
                options.packs_dir.display()
            ));
        }
        return Ok(found);
    }

    match options.pack.as_deref() {
        Some(pack) => {
            if options.packs_dir.join(pack).join("pack.json").is_file() {
                Ok(vec![pack.to_owned()])
            } else {
                Err(format!(
                    "No pack called {pack} in {} — try `kettle packs list`.",
                    options.packs_dir.display()
                ))
            }
        }
        None => Err("Name a pack to evaluate, or pass --all for every pack.".to_owned()),
    }
}

// ---------------------------------------------------------------------------
// Repeats

/// The run to report out of a set of repeats, and a warning if they
/// disagreed.
///
/// `--runs 3` exists to confirm stability: grammar-constrained answers
/// at temperature 0 should not move, so any spread is itself the finding
/// (brief §6). The run reported is the **worst** one, carrying a
/// [`Stability`] block per fixture that records what the repeats
/// disagreed about (#83).
///
/// Worst rather than mean because a mean hides exactly the signal the
/// flag exists to surface: 0.95/0.95/0.95 and 1.00/1.00/0.85 average the
/// same, and only one of them is a model you would recommend. Worst
/// rather than last because "whichever finished most recently" is not a
/// property of the model.
fn worst_run(mut repeats: Vec<EvalReport>) -> (EvalReport, Option<String>) {
    let stability = stability_of(&repeats);
    let wobble = describe_wobble(&repeats, &stability);
    // Worst verdict first; between equal verdicts the lowest end
    // result, and between equal end results the lowest step score.
    //
    // That last tie-break matters more than it looks. app.kttl.subscription-audit
    // bars normalise at 0.85, so 0.71 and 0.88 can sit under the same
    // PASS with the same end-to-end number, and reporting whichever
    // repeat happened to be measured first would average the difference
    // away one run later — exactly what `--runs` exists to prevent.
    repeats.sort_by(|a, b| {
        b.verdict
            .cmp(&a.verdict)
            .then(mean_end_to_end(a).total_cmp(&mean_end_to_end(b)))
            .then(lowest_step_score(a).total_cmp(&lowest_step_score(b)))
    });
    let mut report = repeats
        .into_iter()
        .next()
        .expect("at least one run — --runs 0 is refused above");

    // Fixtures are the same set in the same order in every repeat —
    // they come from one pack read once — so position is the join.
    for (fixture, stability) in report.fixtures.iter_mut().zip(stability) {
        fixture.stability = stability;
    }
    (report, wobble)
}

/// What the repeats disagreed about, fixture by fixture, positionally.
///
/// `None` throughout for a single run: one run cannot disagree with
/// itself, and a spread of one number beside every score would read as
/// a stability claim nobody made.
fn stability_of(repeats: &[EvalReport]) -> Vec<Option<Stability>> {
    let Some(first) = repeats.first() else {
        return Vec::new();
    };
    if repeats.len() < 2 {
        return vec![None; first.fixtures.len()];
    }

    first
        .fixtures
        .iter()
        .enumerate()
        .map(|(index, fixture)| {
            let at = |index: usize| {
                repeats
                    .iter()
                    .filter_map(move |run| run.fixtures.get(index))
            };
            Some(Stability {
                runs: repeats.len() as u32,
                // Every step the first run scored, including the ones
                // that held: "we checked and it did not move" is the
                // finding `--runs` was asked for.
                steps: fixture
                    .step_scores
                    .keys()
                    .map(|step| {
                        let spread = Spread::across(
                            at(index)
                                .filter_map(|f| f.step_scores.get(step))
                                .map(|s| s.score),
                        );
                        (step.clone(), spread)
                    })
                    .collect(),
                end_to_end: Spread::across(at(index).map(|f| f.end_to_end)),
                needs_review_rate: Spread::across(at(index).map(|f| f.needs_review_rate)),
                record_digests: at(index).map(record_digest).collect(),
            })
        })
        .collect()
}

/// Everything a repeat recorded about one fixture, as a digest.
///
/// The spreads beside this one are the quantities somebody remembered
/// to watch, and the harm ceiling was not among them (#533):
/// `confident_wrong` comes from `items`, which no spread covers. So
/// this hashes the whole record instead — scores, items, containment,
/// end-to-end, review rate — and the set of digests answers "did the
/// repeats agree?" totally rather than for a list of fields that has to
/// be maintained.
///
/// Two things are excluded, and both would produce a false alarm:
/// `perf`, because two repeats taking different wall-clock times is the
/// machine being a machine; and `stability` itself, which is not yet
/// populated here and would be self-referential if it were.
fn record_digest(fixture: &FixtureResult) -> String {
    let comparable = FixtureResult {
        perf: None,
        stability: None,
        ..fixture.clone()
    };
    let json = serde_json::to_vec(&comparable).unwrap_or_default();
    format!("blake3:{}", blake3::hash(&json).to_hex())
}

/// The lowest single step score anywhere in a report — how badly its
/// worst moment went. `1.0` when nothing was scored, so a report with no
/// steps never sorts as worse than one that has them.
fn lowest_step_score(report: &EvalReport) -> f32 {
    report
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.step_scores.values())
        .map(|scored| scored.score)
        .fold(1.0, f32::min)
}

fn mean_end_to_end(report: &EvalReport) -> f32 {
    if report.fixtures.is_empty() {
        return 0.0;
    }
    report
        .fixtures
        .iter()
        .map(|fixture| fixture.end_to_end)
        .sum::<f32>()
        / report.fixtures.len() as f32
}

/// A plain-language warning if the repeats disagreed about anything a
/// score is made of.
///
/// Reads the [`Stability`] blocks rather than recomputing the spreads,
/// so the sentence under the table and the numbers in the JSON can
/// never disagree about what moved.
///
/// Timings are left out: those move with the weather. The verdict is
/// not, because a run set that cannot agree whether it passed is the
/// starkest form of the finding.
fn describe_wobble(repeats: &[EvalReport], stability: &[Option<Stability>]) -> Option<String> {
    let first = repeats.first()?;
    if repeats.len() < 2 {
        return None;
    }

    let mut moved: Vec<String> = Vec::new();
    if repeats.iter().any(|run| run.verdict != first.verdict) {
        moved.push("the verdict".to_owned());
    }
    for (fixture, stability) in first.fixtures.iter().zip(stability) {
        let Some(stability) = stability else { continue };
        for (step, spread) in &stability.steps {
            if spread.moved() {
                moved.push(format!(
                    "{step} on {} {}",
                    fixture.fixture,
                    spread.describe()
                ));
            }
        }
        if stability.end_to_end.moved() {
            moved.push(format!(
                "the end result on {} {}",
                fixture.fixture,
                stability.end_to_end.describe(),
            ));
        }
        if stability.needs_review_rate.moved() {
            moved.push(format!(
                "the share sent for review on {} {}",
                fixture.fixture,
                stability.needs_review_rate.describe(),
            ));
        }
    }

    if moved.is_empty() {
        return None;
    }
    Some(format!(
        "Warning: {} gave different answers each time it was asked — {}. \
         At temperature 0 the answers should not move at all, so treat this as a \
         fault to chase, not a rounding error. The row above is the worst run.",
        first.model_name(),
        moved.join("; "),
    ))
}
