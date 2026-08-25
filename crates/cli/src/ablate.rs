//! `kettle ablate` — one recording, read under several system policies
//! (#432).
//!
//! A model leaderboard cannot say whether the model is useful or
//! whether a guardrail is supplying the reliability the product claims.
//! This command answers it from evidence already on disk: an eval run
//! wrote what the pipeline recorded and what the harness scored, and
//! every intermediate policy is a re-reading of those two files rather
//! than a mode somebody could run.
//!
//! It downloads nothing, spawns no sidecar and calls no model, so it is
//! as cheap to re-ask a year later as it is today.

use runner::claim_trace::Guardrail;
use runner::eval::ablation::{self, Policy, PolicyRow};
use runner::eval::EvalReport;
use std::collections::BTreeSet;
use std::path::Path;

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

/// Score every policy the recordings support, for every report in a
/// baseline.
pub fn run(baseline_path: &Path, runs_dir: &Path, now: chrono::DateTime<chrono::Utc>) -> Outcome {
    let baseline = match crate::eval::baseline::read(baseline_path) {
        Ok(baseline) => baseline,
        Err(e) => return broken(e),
    };
    // The same refusal the comparison makes, for the same reason: a
    // policy row is built out of scored items, so a recording scored
    // under other rules would report differences that are only the
    // harness moving underneath (evals/README.md).
    let remark = match baseline.check(baseline_path, now) {
        Ok(remark) => remark,
        Err(e) => return broken(e),
    };

    let mut text = String::new();
    if let Some(remark) = remark {
        text.push_str(&remark);
        text.push_str("\n\n");
    }
    if baseline.reports.is_empty() {
        return broken(format!(
            "The baseline {} holds no reports, so there is nothing to \
             read policies out of.",
            baseline_path.display(),
        ));
    }

    let mut scored_anything = false;
    for report in &baseline.reports {
        let (section, scored) = ablate_report(report, runs_dir);
        text.push_str(&section);
        scored_anything |= scored;
    }

    if !scored_anything {
        text.push_str(
            "\nNo recording was found for any fixture. The archived run \
             directory is what this command reads; a baseline on its own \
             carries the scores and not the claims they were read from.\n",
        );
        return Outcome {
            text,
            code: ExitCode::Broken,
        };
    }

    Outcome {
        text,
        code: ExitCode::Ok,
    }
}

fn ablate_report(report: &EvalReport, runs_dir: &Path) -> (String, bool) {
    let fixtures: Vec<String> = report
        .fixtures
        .iter()
        .map(|fixture| fixture.fixture.clone())
        .collect();
    let walk = ablation::walk(
        runs_dir,
        &report.pack,
        report.model.as_ref().map(|model| model.file.as_str()),
        &fixtures,
    );

    let mut text = format!(
        "{} {} — {}\n",
        report.pack,
        report.pack_version,
        report
            .model
            .as_ref()
            .map(|model| model.file.as_str())
            .unwrap_or("no model (the deterministic floor)"),
    );
    text.push_str(&format!(
        "{} of {} fixtures had a recording on disk.\n",
        walk.recordings.len(),
        fixtures.len(),
    ));
    // Named, not counted, and named first: every column below is
    // smaller for each one of these, and a smaller harm column reads
    // as a better policy.
    if !walk.missing.is_empty() {
        text.push_str(&format!("Missing: {}\n", summarise(&walk.missing, 5),));
    }
    if walk.recordings.is_empty() {
        return (text, false);
    }

    let pooled = ablation::pool(&walk.recordings);
    let observed: BTreeSet<Guardrail> = pooled
        .traces
        .iter()
        .flat_map(|trace| &trace.checks)
        .map(|check| check.guardrail)
        .collect();
    let rows = ablation::scorecard(&pooled.traces, &pooled.verdicts, &Policy::ladder(&observed));

    text.push_str(&format!(
        "\n{} claims carry a verdict, out of {} recorded.\n\n",
        pooled.verdicts.len(),
        pooled.traces.len(),
    ));
    text.push_str(&table(&rows));
    text.push_str(&missed(&walk.recordings));
    text.push_str(&unsettled(&rows));
    text.push_str(&escaped_claims(&rows));
    text.push('\n');
    (text, true)
}

/// The harm the table cannot show, printed beside it (#432, #474).
///
/// A miss is an authored expectation the run asserted nothing from. It
/// produces no claim, so no boundary sees it and no guardrail can act
/// on it — it is identical under every policy, which is why it sits
/// beside the table rather than in a column that would read the same
/// number in every row and imply a comparison nobody made.
///
/// Without it the scorecard tells half a truth. On 25 August 2026 the
/// letter pack read 0 escaped under every rung while its eval reported
/// seven wrong answers on the sealed set, all of them misses. "Nothing
/// escaped" and "a person was told an invoice asked nothing of them"
/// were both true of the same run, and only the first was on the page.
fn missed(recordings: &[ablation::Recording]) -> String {
    let missed: Vec<String> = recordings
        .iter()
        .flat_map(|recording| {
            ablation::misses(&recording.items)
                .into_iter()
                .map(|item| format!("{}#{item}", recording.fixture))
        })
        .collect();
    if missed.is_empty() {
        return String::new();
    }
    format!(
        "\n{} authored expectation(s) the run asserted nothing from. No policy \n\
         above changes this number: a miss produces no claim, so nothing was \n\
         stopped and nothing escaped -- it is the harm containment cannot reach.\n",
        missed.len(),
    )
}

/// The gap between what a policy asserted and what the recording can
/// say about it.
///
/// `answered` is `delivered` plus `escaped` plus this, and a table
/// whose columns do not add up invites the reader to assume the
/// remainder was fine. On the letter bed the remainder is zero. On
/// renewal it is 227 of 363, because a passage stating several terms
/// links all of them to one scored decision, and which term that
/// decision judged is not in the recording — so the honest verdict for
/// every one of them is that there is none.
///
/// Printed rather than folded into a column, because it is a fact about
/// the instrument on this pack rather than about the policy.
fn unsettled(rows: &[PolicyRow]) -> String {
    let Some(strictest) = rows.last() else {
        return String::new();
    };
    let unsettled = strictest
        .answered
        .len()
        .saturating_sub(strictest.delivered.len() + strictest.escaped.len());
    if unsettled == 0 {
        return String::new();
    }
    format!(
        "\n{unsettled} of {} claims {} asserted carry no verdict the \n\
         recording can settle, so no column above counts them.\n",
        strictest.answered.len(),
        strictest.policy,
    )
}

/// The claims that escaped the strictest policy on the ladder, named.
///
/// A count says the guardrails let twelve wrong claims through; the ids
/// say which, and only the ids turn a scorecard into work — a
/// discriminating audition subset (#539) has to be built from items
/// something is measured to get wrong, and a number cannot say which
/// those are.
///
/// The last row is the strictest policy, because [`Policy::ladder`]
/// adds boundaries cumulatively. Its escapes are the ones the whole
/// pipeline failed to stop.
pub fn escaped_claims(rows: &[PolicyRow]) -> String {
    let Some(strictest) = rows.last() else {
        return String::new();
    };
    if strictest.escaped.is_empty() {
        return String::new();
    }
    let mut text = format!(
        "\n{} claim(s) escaped {}:\n",
        strictest.escaped.len(),
        strictest.policy,
    );
    for id in &strictest.escaped {
        text.push_str(&format!("  {id}\n"));
    }
    text
}

/// The scorecard, as columns that are never added together.
///
/// Harm, containment, the bound on containment and usefulness stay four
/// numbers. Collapsing them into one score would hide its weights, and
/// the system that wins a weighted harm score is always the one that
/// answers nothing (#432).
fn table(rows: &[PolicyRow]) -> String {
    let width = rows
        .iter()
        .map(|row| row.policy.chars().count())
        .max()
        .unwrap_or(6)
        .max("policy".len());
    let mut text = format!(
        "{:width$}  {:>8}  {:>9}  {:>8}  {:>9}  {:>8}\n",
        "policy", "answered", "delivered", "escaped", "prevented", "unknown",
    );
    for row in rows {
        text.push_str(&format!(
            "{:width$}  {:>8}  {:>9}  {:>8}  {:>9}  {:>8}\n",
            row.policy,
            row.answered.len(),
            row.delivered.len(),
            row.escaped.len(),
            row.prevented.len(),
            row.unknown.len(),
        ));
    }
    text
}

/// A list, kept readable without becoming a count.
fn summarise(items: &[String], shown: usize) -> String {
    if items.len() <= shown {
        return items.join(", ");
    }
    format!(
        "{}, and {} more",
        items[..shown].join(", "),
        items.len() - shown,
    )
}

fn broken(text: String) -> Outcome {
    Outcome {
        text: text + "\n",
        code: ExitCode::Broken,
    }
}
