//! The baseline: what this pack and this model scored last time, and
//! whether anything has got worse since (#38).
//!
//! CLAUDE.md calls this the prompt-editing safety net — until it exists,
//! changing a prompt means re-running the fixtures by hand and hoping
//! you remember the old numbers. So the comparison has to be strict
//! enough to be worth trusting.
//!
//! # What counts as a regression
//!
//! Anything that got worse:
//!
//! - the **verdict** worsened — `Pass` to `Marginal`, `Marginal` to
//!   `Fail`, `Pass` to `Fail`;
//! - a **step score** dropped;
//! - the **end result** dropped;
//! - a pack-and-model pair, or a fixture within one, that the baseline
//!   measured and this eval did not. Coverage quietly disappearing is
//!   how a safety net stops catching things.
//!
//! Deliberately **not** regressions: needs-review rate and resource
//! telemetry. Review is a tracked cost and its movement is printed as a
//! note; it is not a quality failure or verdict gate. Timings, peak
//! memory and token rate move with the machine, the weather and what else
//! is running. They stay in individual run receipts for diagnosis and
//! same-sitting comparisons; the durable baseline omits them rather than
//! giving bare scalars the shape of comparable evidence. Retries remain:
//! they describe the answers and validation path, not resource use.
//! Nor is a *new* pack, model or fixture a regression: measuring more
//! than last time is not doing worse than last time.
//!
//! # Why no tolerance band
//!
//! A drop of any size fails. Evals run at temperature 0 against a
//! grammar, so the same model on the same fixture should give the same
//! answer every time — a score that moves has a cause, and the cause is
//! what this file exists to surface. A band would only ever be a
//! judgement about how much silent drift is acceptable, and the honest
//! answer is none.
//!
//! [`FLOAT_NOISE`] is not that band. Scores are `f32` fractions of small
//! integer counts (44 of 50) that make a round trip through JSON text;
//! it is there so bit-level round-trip noise cannot be reported as a
//! finding. It is far smaller than a single answer's worth of any
//! plausible fixture.

use chrono::{DateTime, SecondsFormat, Utc};
use runner::eval::{
    paired_classification_comparison, EvalMetric, EvalReport, FixtureResult, ScoredItem,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What the numbers in a baseline mean (#84).
///
/// Lives with the scoring it describes ([`runner::eval::SCORING_VERSION`])
/// and is re-exported here because the baseline is what most people meet
/// it through. `tiers.json` stamps it on every entry too, so it could not
/// stay owned by this module.
pub use runner::eval::SCORING_VERSION;

/// How old a baseline gets before the comparison says so.
///
/// Not a failure — a baseline is only stale in the sense that a lot has
/// probably happened since, and the numbers themselves are as valid as
/// the day they were recorded. It is a fact worth putting in front of
/// someone reading a clean result, which is exactly when nobody thinks
/// to check the date.
pub const STALE_AFTER_DAYS: i64 = 30;

/// The most two scores can differ by and still be the same score.
///
/// The same idea as [`runner::eval::SCORE_NOISE`] and deliberately the
/// same number: "did this move at all" has one answer, whether it is
/// asked of two runs of the same eval (#83) or of an eval and its
/// baseline.
pub const FLOAT_NOISE: f32 = runner::eval::SCORE_NOISE;

/// The file `--baseline` reads and `--write-baseline` writes.
///
/// An object rather than a bare array so the format can gain a field
/// — a schema version, the date, the machine — without every existing
/// baseline becoming unreadable. #84 is the first time that mattered.
///
/// Both provenance fields are optional on the way *in* and always
/// written on the way *out*: a file from before they existed has to
/// parse, so that it can be refused with a sentence explaining what to
/// do rather than a serde error about a missing field.
#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    /// Which [`SCORING_VERSION`] produced these numbers. `None` means a
    /// harness from before the field existed, which is the same
    /// situation as a mismatch: nothing says what the numbers mean.
    #[serde(default)]
    pub scoring_version: Option<u32>,
    /// When it was recorded. `None` for the same reason.
    #[serde(default)]
    pub recorded_at: Option<DateTime<Utc>>,
    pub reports: Vec<EvalReport>,
}

impl Baseline {
    /// Whether this baseline can be compared against numbers scored
    /// today, and anything worth saying if it can.
    ///
    /// `Err` refuses the comparison outright. A safety net that reports
    /// confident nonsense is worse than one that reports nothing, so a
    /// baseline whose numbers mean something other than today's numbers
    /// stops the command rather than producing a table of differences
    /// that are artefacts of the harness.
    ///
    /// `Ok(Some(..))` is a remark to print alongside a comparison that
    /// is going ahead.
    pub fn check(&self, path: &Path, now: DateTime<Utc>) -> Result<Option<String>, String> {
        if self.scoring_version != Some(SCORING_VERSION) {
            let recorded = match self.scoring_version {
                Some(version) => format!("scoring version {version}"),
                None => "a harness from before scoring was versioned".to_owned(),
            };
            return Err(format!(
                "The baseline {} was recorded by {}, and this harness scores at \
                 version {SCORING_VERSION} — the numbers no longer mean the same \
                 thing, so comparing them would report movement that is only the \
                 harness changing underneath. Re-record it with --write-baseline {}, \
                 having checked the new numbers are ones you would sign off.",
                path.display(),
                recorded,
                path.display(),
            ));
        }

        let Some(recorded_at) = self.recorded_at else {
            return Ok(None);
        };
        let days = (now - recorded_at).num_days();
        Ok((days >= STALE_AFTER_DAYS).then(|| {
            format!(
                "Note: this baseline is {days} days old — recorded {}. Nothing is \
                 wrong with it, but a lot may have happened since, and \"nothing got \
                 worse\" is only as reassuring as the thing it is measured against.",
                recorded_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            )
        }))
    }
}

/// Render reports as the baseline file's JSON, stamped with when they
/// were measured and what the numbers mean.
///
/// `now` is passed in rather than read here: a timestamp taken in the
/// middle of a pure function is a thing no test can pin down. It comes
/// from the CLI edge, the same way `today` reaches the rest of the
/// runner.
pub fn to_json(reports: &[EvalReport], now: DateTime<Utc>) -> String {
    // Resource telemetry is sitting-local and never durable evidence
    // (#220): the receipt keeps its `perf` block, the baseline does not.
    // Dropped here, on the way out, so this is the only place that
    // knows a projection differs from a receipt; `retries` stays,
    // being a property of the answers rather than of the sitting.
    let mut reports = reports.to_vec();
    for report in &mut reports {
        for fixture in &mut report.fixtures {
            fixture.perf = None;
        }
    }
    let file = Baseline {
        scoring_version: Some(SCORING_VERSION),
        recorded_at: Some(now),
        reports,
    };
    // Pretty-printed: baselines get committed, and a diff nobody can
    // read is a diff nobody reviews.
    serde_json::to_string_pretty(&file).expect("eval reports serialise") + "\n"
}

/// Read a baseline file.
pub fn read(path: &Path) -> Result<Baseline, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "Could not read the baseline {}: {e}. If there isn't one yet, \
             record it with --write-baseline {}.",
            path.display(),
            path.display(),
        )
    })?;
    let file: Baseline = serde_json::from_str(&text).map_err(|e| {
        format!(
            "Could not make sense of the baseline {}: {e}",
            path.display()
        )
    })?;
    Ok(file)
}

/// Write a baseline file, creating its directory if need be.
/// Every fixture the reports say this build could not read.
///
/// A report that names one measured a smaller bed than the bed. Reading
/// it is fine — that is how a person on a machine without pdfium sees
/// the rest of the run — but it cannot become evidence: a baseline
/// recorded from it would pin a denominator nobody declared, and a
/// comparison against one would call a fixture that never ran an
/// improvement or a drop (#256).
pub fn unrunnable_in(reports: &[EvalReport]) -> Vec<String> {
    reports
        .iter()
        .flat_map(|report| {
            report
                .unrunnable
                .iter()
                .map(move |fixture| format!("{}: {fixture}", report.pack))
        })
        .collect()
}

pub fn write(path: &Path, reports: &[EvalReport], now: DateTime<Utc>) -> Result<(), String> {
    let missing = unrunnable_in(reports);
    if !missing.is_empty() {
        return Err(format!(
            "This run could not read {} of the bed's fixtures, so it is not a measurement of \
             the bed and must not be recorded as one:\n  {}\nInstall what reads them (a PDF \
             fixture needs the `pdf` feature and a pdfium directory) and record again.",
            missing.len(),
            missing.join("\n  ")
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Could not make {}: {e}", parent.display()))?;
    }
    std::fs::write(path, to_json(reports, now))
        .map_err(|e| format!("Could not write the baseline {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Comparing

/// What moved between a baseline and this eval.
#[derive(Debug, Default)]
pub struct Comparison {
    /// Per-item disagreements between the two runs, rendered before any
    /// aggregate movement because these are the finding the aggregate
    /// only summarises (#237).
    pub discordant_items: Vec<String>,
    /// Things that got worse. Any of these exits non-zero.
    pub regressions: Vec<String>,
    /// Things that got better — never a failure, but the whole point of
    /// most prompt edits, so worth saying out loud.
    pub improvements: Vec<String>,
    /// Things that changed underneath the measurement rather than in
    /// it (#74). Never a regression by themselves; they are what a
    /// regression should be read against.
    pub notes: Vec<String>,
    /// Comparisons that could not honestly be made at all (#320).
    ///
    /// Deliberately not a regression: a regression says the model got
    /// worse, and this says nobody can tell. The harness exits 2 for the
    /// same reason it does on a scoring-version mismatch — a number that
    /// cannot be trusted should not be printed as one.
    pub refusals: Vec<String>,
}

impl Comparison {
    pub fn is_regression(&self) -> bool {
        !self.regressions.is_empty()
    }

    /// Whether anything could not be compared at all (#320).
    pub fn is_refused(&self) -> bool {
        !self.refusals.is_empty()
    }

    /// The paragraph the command prints under the table.
    pub fn report(&self) -> String {
        let mut out = String::new();
        if !self.discordant_items.is_empty() {
            out.push_str("Discordant scored items (before aggregate scores):\n");
            for item in &self.discordant_items {
                out.push_str(item);
                if !item.ends_with('\n') {
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        // Before anything else: a refusal means the lines below are
        // about a comparison that could not honestly be made, and
        // reading "nothing got worse" first would be the wrong order to
        // learn that in.
        if !self.refusals.is_empty() {
            let count = self.refusals.len();
            let plural = if count == 1 { "" } else { "s" };
            out.push_str(&format!(
                "{count} comparison{plural} refused — the bed moved under the baseline:\n"
            ));
            for refusal in &self.refusals {
                out.push_str(&format!("  - {refusal}\n"));
            }
            out.push('\n');
        }
        if self.regressions.is_empty() {
            out.push_str("Nothing got worse than the baseline.\n");
        } else {
            let count = self.regressions.len();
            let plural = if count == 1 { "" } else { "s" };
            out.push_str(&format!(
                "{count} thing{plural} got worse than the baseline:\n"
            ));
            for regression in &self.regressions {
                out.push_str(&format!("  - {regression}\n"));
            }
        }
        if !self.improvements.is_empty() {
            out.push_str("\nBetter than the baseline:\n");
            for improvement in &self.improvements {
                out.push_str(&format!("  - {improvement}\n"));
            }
        }
        // Last, and unindented: a note is context for everything above
        // it, not another entry in the list.
        for note in &self.notes {
            out.push_str(&format!("\n{note}\n"));
        }
        out
    }
}

/// Compare an eval against a baseline, pack and model by pack and model.
pub fn compare(baseline: &[EvalReport], now: &[EvalReport]) -> Comparison {
    let mut comparison = Comparison::default();

    for was in baseline {
        let Some(is) = now.iter().find(|report| {
            report.pack == was.pack
                && report.model_name() == was.model_name()
                && report.eval_set == was.eval_set
        }) else {
            comparison.regressions.push(format!(
                "{}: the baseline measured {} on the {} set and this eval didn't",
                was.pack,
                was.model_name(),
                was.eval_set.as_str(),
            ));
            continue;
        };
        // Which questions each side was asked (#320). A bed rewritten
        // under a baseline changes the answers a model could give
        // without changing the pack version or the scoring version, so
        // neither existing guard sees it — and the comparison would
        // report a drop or a hold, both readings wrong.
        match (was.bed.as_deref(), is.bed.as_deref()) {
            (Some(before), Some(after)) if before != after => {
                comparison.refusals.push(format!(
                    "{} / {} on the {} set: the baseline ran against a different bed ({} \
                     against {}). The fixtures changed under the measurement, so a \
                     comparison would report movement that is only the questions \
                     changing. Re-record the baseline against this bed.",
                    was.pack,
                    was.model_name(),
                    was.eval_set.as_str(),
                    short(before),
                    short(after),
                ));
                continue;
            }
            (None, _) | (_, None) => comparison.notes.push(format!(
                "{} / {} on the {} set: one side does not say which bed it ran against, \
                 so this comparison assumes they match. A baseline recorded before beds \
                 were identified cannot prove it.",
                was.pack,
                was.model_name(),
                was.eval_set.as_str(),
            )),
            _ => {}
        }
        // The runtime policy the two measurements ran under (#232) —
        // context, reasoning, answer bound. The same shape as the bed
        // check, for the same reason: a policy change moves scores and
        // wall times without touching the weights, the prompt, the bed
        // or the scoring, so neither of those guards sees it, and a
        // comparison across it would report movement that is only the
        // policy changing.
        match (&was.runtime, &is.runtime) {
            (Some(before), Some(after)) if before != after => {
                comparison.refusals.push(format!(
                    "{} / {} on the {} set: the baseline ran under a different runtime \
                     policy ({} against {}). The policy moved under the measurement, so \
                     a comparison would report movement that is only the policy \
                     changing. Re-record the baseline under this policy.",
                    was.pack,
                    was.model_name(),
                    was.eval_set.as_str(),
                    before.describe(),
                    after.describe(),
                ));
                continue;
            }
            (None, _) | (_, None) => comparison.notes.push(format!(
                "{} / {} on the {} set: one side does not say what runtime policy it \
                 ran under, so this comparison assumes they match. A baseline recorded \
                 before the policy was recorded cannot prove it.",
                was.pack,
                was.model_name(),
                was.eval_set.as_str(),
            )),
            _ => {}
        }
        compare_one(was, is, &mut comparison);
    }

    comparison
}

/// A digest, short enough to read in a sentence. Full digests belong in
/// the file; a person comparing two only needs to see that they differ.
fn short(digest: &str) -> &str {
    let hex = digest.strip_prefix("blake3:").unwrap_or(digest);
    &hex[..hex.len().min(12)]
}

/// Pair every measured build on the identical authored item ids. This
/// is the multi-model form of the baseline comparison used for #234:
/// the builds share the bed, so independent score intervals throw away
/// information the pairing already gives us.
pub fn compare_builds(reports: &[EvalReport]) -> String {
    let mut out = String::new();
    for (left_index, left) in reports.iter().enumerate() {
        for right in &reports[left_index + 1..] {
            if !left.metrics.contains_key(&EvalMetric::Classification)
                || !right.metrics.contains_key(&EvalMetric::Classification)
            {
                continue;
            }
            let left_items = left
                .fixtures
                .iter()
                .flat_map(|fixture| fixture.items.iter().cloned())
                .collect::<Vec<_>>();
            let right_items = right
                .fixtures
                .iter()
                .flat_map(|fixture| fixture.items.iter().cloned())
                .collect::<Vec<_>>();
            let paired = paired_classification_comparison(&left_items, &right_items);
            if paired.matched == 0 {
                continue;
            }

            if out.is_empty() {
                out.push_str("\nPaired classification build comparisons\n");
            }
            out.push('\n');
            for id in &paired.discordant_item_ids {
                let Some(was) = left_items.iter().find(|item| &item.id == id) else {
                    continue;
                };
                let Some(is) = right_items.iter().find(|item| &item.id == id) else {
                    continue;
                };
                out.push_str(&describe_discordance_between(
                    was,
                    is,
                    left.model_name(),
                    right.model_name(),
                ));
            }

            let pair_word = if paired.discordant == 1 {
                "pair"
            } else {
                "pairs"
            };
            out.push_str(&format!(
                "{} vs {}: paired classification comparison has {} discordant {pair_word} \
                 ({} worse in {}, {} better in {})",
                left.model_name(),
                right.model_name(),
                paired.discordant,
                paired.regressions,
                right.model_name(),
                paired.improvements,
                right.model_name(),
            ));
            if paired.discordant == 0 {
                out.push_str(
                    "; the builds made the same surfaced/confident-wrong outcome on every \
                     matched item, so McNemar has no difference to test.\n",
                );
            } else if !paired.can_reach_significance {
                out.push_str(
                    "; too few for exact McNemar to reach p < 0.05. \
                     The discordant items above are the finding.\n",
                );
            } else {
                out.push_str(&format!(
                    "; exact McNemar p = {:.4}.\n",
                    paired.exact_two_sided_p
                ));
            }
        }
    }
    out
}

/// One pack and model, measured twice.
fn compare_one(was: &EvalReport, is: &EvalReport, comparison: &mut Comparison) {
    let what = format!("{} / {}", was.pack, was.model_name());

    note_sidecar_change(&what, was, is, comparison);
    note_device_change(&what, was, is, comparison);

    // Verdict ordering is Pass < Marginal < Fail, so "greater" is worse.
    if is.verdict > was.verdict {
        comparison.regressions.push(format!(
            "{what}: the verdict was {}, now {}",
            was.verdict.label(),
            is.verdict.label(),
        ));
    } else if is.verdict < was.verdict {
        comparison.improvements.push(format!(
            "{what}: the verdict was {}, now {}",
            was.verdict.label(),
            is.verdict.label(),
        ));
    }

    for fixture_was in &was.fixtures {
        let Some(fixture_is) = is
            .fixtures
            .iter()
            .find(|fixture| fixture.fixture == fixture_was.fixture)
        else {
            comparison.regressions.push(format!(
                "{what}: {} was scored in the baseline and isn't now",
                fixture_was.fixture,
            ));
            continue;
        };
        compare_fixture(&what, fixture_was, fixture_is, comparison);
    }

    compare_paired_classifications(&what, was, is, comparison);
}

fn compare_paired_classifications(
    what: &str,
    was: &EvalReport,
    is: &EvalReport,
    comparison: &mut Comparison,
) {
    if !was.metrics.contains_key(&EvalMetric::Classification)
        || !is.metrics.contains_key(&EvalMetric::Classification)
    {
        return;
    }

    let before = was
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.items.iter().cloned())
        .collect::<Vec<_>>();
    let after = is
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.items.iter().cloned())
        .collect::<Vec<_>>();
    let paired = paired_classification_comparison(&before, &after);
    if paired.discordant == 0 {
        return;
    }

    let pair_word = if paired.discordant == 1 {
        "pair"
    } else {
        "pairs"
    };
    let summary = format!(
        "{what}: paired classification comparison has {} discordant {pair_word} \
         ({} worse, {} better)",
        paired.discordant, paired.regressions, paired.improvements
    );

    if !paired.can_reach_significance {
        comparison.notes.push(format!(
            "Note: {summary}; too few for exact McNemar to reach p < 0.05. \
             The discordant items above are the finding."
        ));
    } else if paired.exact_two_sided_p >= 0.05 {
        comparison.notes.push(format!(
            "Note: {summary}; exact McNemar p = {:.4}, so this run does not \
             distinguish the builds. The discordant items above are the finding.",
            paired.exact_two_sided_p
        ));
    } else if paired.regressions > paired.improvements {
        comparison.regressions.push(format!(
            "{summary}; exact McNemar p = {:.4}",
            paired.exact_two_sided_p
        ));
    } else {
        comparison.improvements.push(format!(
            "{summary}; exact McNemar p = {:.4}",
            paired.exact_two_sided_p
        ));
    }
}

/// One fixture, scored twice.
fn compare_fixture(
    what: &str,
    was: &FixtureResult,
    is: &FixtureResult,
    comparison: &mut Comparison,
) {
    let where_ = format!("{what}: {}", was.fixture);

    compare_items(&where_, was, is, comparison);

    for (step, scored_was) in &was.step_scores {
        let Some(scored_is) = is.step_scores.get(step) else {
            comparison
                .regressions
                .push(format!("{where_}: {step} was scored before and isn't now"));
            continue;
        };
        note_move(
            comparison,
            &format!("{where_}: {step}"),
            scored_was.score,
            scored_is.score,
        );
    }

    note_move(
        comparison,
        &format!("{where_}: the end result"),
        was.end_to_end,
        is.end_to_end,
    );
    note_cost_move(
        comparison,
        &format!("{where_}: the share sent for review"),
        was.needs_review_rate,
        is.needs_review_rate,
    );
}

fn compare_items(
    where_: &str,
    was: &FixtureResult,
    is: &FixtureResult,
    comparison: &mut Comparison,
) {
    for item_was in &was.items {
        let Some(item_is) = is.items.iter().find(|item| item.id == item_was.id) else {
            comparison.regressions.push(format!(
                "{where_}: scored item {} was in the baseline and isn't now",
                item_was.id
            ));
            continue;
        };
        if item_was.decision != item_is.decision {
            comparison
                .discordant_items
                .push(describe_discordance(item_was, item_is));
        }
    }

    for item_is in &is.items {
        if !was.items.iter().any(|item| item.id == item_is.id) {
            comparison.notes.push(format!(
                "Note: {where_} has a new scored item not present in the baseline: {}.",
                item_is.id
            ));
        }
    }
}

fn describe_discordance(was: &ScoredItem, is: &ScoredItem) -> String {
    describe_discordance_between(was, is, "baseline", "current")
}

fn describe_discordance_between(
    was: &ScoredItem,
    is: &ScoredItem,
    was_label: &str,
    is_label: &str,
) -> String {
    let mut out = format!("  {}\n", was.id);
    out.push_str(&format!(
        "    {was_label} (pack {}, prompt {}): expected {}; actual {}\n",
        was.pack_version,
        was.prompt_version,
        was.decision.describe_expected(),
        was.decision.describe_actual()
    ));
    describe_exchanges(&mut out, was_label, &was.exchanges);
    out.push_str(&format!(
        "    {is_label} (pack {}, prompt {}): expected {}; actual {}\n",
        is.pack_version,
        is.prompt_version,
        is.decision.describe_expected(),
        is.decision.describe_actual()
    ));
    describe_exchanges(&mut out, is_label, &is.exchanges);
    out
}

fn describe_exchanges(out: &mut String, which: &str, exchanges: &[runner::eval::ModelExchange]) {
    if exchanges.is_empty() {
        out.push_str(&format!("      {which} exchange: none\n"));
        return;
    }
    for exchange in exchanges {
        out.push_str(&format!(
            "      {which} exchange ({} batch {}):\n",
            exchange.step, exchange.batch
        ));
        out.push_str("        request:\n");
        indent(out, &exchange.request, 10);
        out.push_str("        response:\n");
        indent(out, &exchange.response, 10);
    }
}

fn indent(out: &mut String, text: &str, spaces: usize) {
    let padding = " ".repeat(spaces);
    for line in text.lines() {
        out.push_str(&padding);
        out.push_str(line);
        out.push('\n');
    }
    if text.is_empty() {
        out.push_str(&padding);
        out.push('\n');
    }
}

/// Say so when the llama-server changed between the two measurements
/// (#74).
///
/// Never a regression on its own — an upgrade that moves nothing is
/// good news. But when something *did* move, the weights being
/// byte-identical is the fact that sends someone off re-reading their
/// prompt edits, and this is the line that stops them. It is deliberately
/// a note rather than an entry in either list: it is the context the
/// lists should be read in.
fn note_sidecar_change(what: &str, was: &EvalReport, is: &EvalReport, comparison: &mut Comparison) {
    let (Some(was_sidecar), Some(is_sidecar)) = (&was.sidecar, &is.sidecar) else {
        return;
    };
    if was_sidecar.version == is_sidecar.version {
        return;
    }
    comparison.notes.push(format!(
        "Note: {what} ran on a different llama-server this time — the baseline used \
         {} and this eval used {}. The weights are pinned; the sidecar is not, and a \
         version bump can change grammar-constrained sampling on its own. Read \
         anything above against that before blaming a prompt.",
        was_sidecar.version, is_sidecar.version,
    ));
}

/// Say so when the device that answered changed between the two
/// measurements, or when one of them never said (#490).
///
/// A note and never a refusal, deliberately — the same treatment an
/// absent `runtime` or `bed` gets. A refusal hands someone a reason
/// they cannot act on cheaply (re-recording a baseline costs a GPU
/// run), and whether two devices' scores are comparable at all is the
/// open question `evals/RENTED-GPU.md` records, not a call this
/// comparison is entitled to make. What it *is* entitled to do is stop
/// a cross-device hold being read as a same-instrument one: a CPU-only
/// fallback build (`scripts/vendor-sidecar.sh`) is correct and many
/// times slower, and without this line its timings would be read
/// against a GPU baseline as a mystery.
fn note_device_change(what: &str, was: &EvalReport, is: &EvalReport, comparison: &mut Comparison) {
    // No sidecar means no device to speak of — a mock endpoint or a
    // replay makes no claim about hardware, and chat about a device
    // neither side could have recorded helps nobody.
    let (Some(was_sidecar), Some(is_sidecar)) = (&was.sidecar, &is.sidecar) else {
        return;
    };
    match (&was_sidecar.device, &is_sidecar.device) {
        (Some(before), Some(after)) if before != after => comparison.notes.push(format!(
            "Note: {what} was answered on a different device this time — the baseline \
             ran on {before} and this eval on {after}. The device moves timings and \
             can move scores; read anything above against that, and read a clean hold \
             as a cross-device one.",
        )),
        (None, _) | (_, None) => comparison.notes.push(format!(
            "Note: {what}: one side does not say which device answered, so this \
             comparison assumes they match. A measurement recorded before the device \
             travelled with the score cannot prove it.",
        )),
        _ => {}
    }
}

fn note_move(comparison: &mut Comparison, what: &str, was: f32, is: f32) {
    let change = is - was;
    if change.abs() <= FLOAT_NOISE {
        return;
    }
    let line = format!("{what} was {was:.2}, now {is:.2}");
    if change < 0.0 {
        comparison.regressions.push(line);
    } else {
        comparison.improvements.push(line);
    }
}

fn note_cost_move(comparison: &mut Comparison, what: &str, was: f32, is: f32) {
    if (is - was).abs() <= FLOAT_NOISE {
        return;
    }
    comparison.notes.push(format!(
        "Note: {what}, a tracked cost rather than a quality gate, was {:.1}%, now {:.1}%.",
        was * 100.0,
        is * 100.0,
    ));
}
