//! `kettle mutate` — run the semantic mutation harness over one pack
//! and answer the safety-case question: do the safeguards hold? (#426)
//!
//! Exit codes mirror `eval`'s contract, and since #466 the exit code
//! means something *new* survived: 0 when every survivor falls inside
//! a family the pack's `expected-survivors.json` declares and no
//! family is over its count, 1 when anything survived beyond the
//! declaration — an undeclared family, or a declared family above its
//! count — each named, 2 the harness could not honestly run. A new
//! survivor is a finding about a missing guard, and the table is the
//! diagnosis — the aggregate only a summary of it.

use runner::eval::mutation::{
    Containment, ExpectedSurvivors, MutationHarness, MutationOperator, MutationRecord,
    MutationReport, Triage,
};
use runner::eval::oracle;
use runner::packs::load_pack;
use std::path::PathBuf;

pub struct Options {
    /// The pack's id, e.g. app.kttl.letter-to-actions.
    pub pack: String,
    /// Directory holding task packs.
    pub packs_dir: PathBuf,
    /// A direct pack directory, overriding discovery. Tests use it;
    /// the command line reaches it via --packs-dir plus the id.
    pub pack_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    Survivors = 1,
    CouldNotRun = 2,
}

pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

fn could_not_run(text: String) -> Outcome {
    Outcome {
        text,
        code: ExitCode::CouldNotRun,
    }
}

pub fn run(options: &Options) -> Outcome {
    let pack_dir = match &options.pack_dir {
        Some(dir) => dir.clone(),
        None => options.packs_dir.join(&options.pack),
    };
    let pack = match load_pack(&pack_dir) {
        Ok(pack) => pack,
        Err(e) => return could_not_run(format!("Could not load {}: {e}", pack_dir.display())),
    };
    if pack.manifest.id != options.pack {
        return could_not_run(format!(
            "{} holds {}, not {}",
            pack_dir.display(),
            pack.manifest.id,
            options.pack
        ));
    }

    // The declaration is read before the harness runs: a broken one
    // must refuse in seconds, not after minutes of replay.
    let expected = match ExpectedSurvivors::load(&pack_dir) {
        Ok(expected) => expected,
        Err(e) => return could_not_run(e),
    };

    let recording = match oracle::recording(&pack) {
        Ok(recording) => recording,
        Err(e) => return could_not_run(format!("The oracle could not answer the bed: {e}")),
    };
    let harness = MutationHarness {
        machine: crate::eval::machine::detect(),
    };
    let report = match harness.run(&pack, &recording, &MutationOperator::ALL) {
        Ok(report) => report,
        Err(e) => return could_not_run(e),
    };

    outcome_of(&report, &expected)
}

/// The report's verdict against the pack's declaration, as an exit
/// code and a table. Split from `run` so the survivor path is testable
/// without fabricating a pack whose guards genuinely fail — today no
/// such pack exists, which is the healthy state this command exists to
/// keep.
pub fn outcome_of(report: &MutationReport, expected: &ExpectedSurvivors) -> Outcome {
    let triage = expected.triage(report);
    let code = if triage.anything_new() {
        ExitCode::Survivors
    } else {
        ExitCode::Ok
    };
    Outcome {
        text: render(report, &triage),
        code,
    }
}

/// The table people read: one row per operator, then the survivors
/// sorted against the declaration — declared families summarised with
/// their counts and reasons, anything new named in full. Operator
/// names are their serialised forms, so a row can be grepped straight
/// back to the code and the records.
fn render(report: &MutationReport, triage: &Triage<'_>) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} — mutation v{}, scoring v{}",
        report.pack, report.mutation_version, report.scoring_version
    ));
    lines.push(String::new());

    for operator in MutationOperator::ALL {
        let records: Vec<_> = report
            .records
            .iter()
            .filter(|record| record.operator == operator)
            .collect();
        if records.is_empty() {
            lines.push(format!(
                "  {:<32} no sites in this recording",
                operator_name(operator)
            ));
            continue;
        }
        let killed = records.iter().filter(|record| record.killed).count();
        let survived = records.len() - killed;
        lines.push(format!(
            "  {:<32} {} mutant{}, {} killed, {} survived",
            operator_name(operator),
            records.len(),
            if records.len() == 1 { "" } else { "s" },
            killed,
            survived,
        ));
    }

    let survivors = report.survivors();
    lines.push(String::new());
    if survivors.is_empty() {
        lines.push("survived: 0 — every planted harm was caught".to_owned());
        return lines.join("\n");
    }

    lines.push(format!("survived: {}", survivors.len()));

    // Item-level detection is a second, separate number — never a
    // kill. wrong_value_from_multi_value_quote kills 0 of its sites
    // and has no gate path at all at these denominators; folding
    // detection into "killed" would delete that fact (#466).
    let detected = survivors
        .iter()
        .filter(|survivor| !survivor.affected_items.is_empty())
        .count();
    lines.push(format!(
        "  item-level detection (a separate number, never a kill): {detected} of {} \
         survivors visibly moved a scored decision",
        survivors.len()
    ));

    for tally in &triage.declared {
        let count = format!(
            "  expected: {} — {} found, {} declared",
            tally.family.name, tally.found, tally.family.count
        );
        if tally.over_count() {
            lines.push(format!(
                "{count} — over its declared count: something new survived inside this family"
            ));
        } else if tally.found < tally.family.count {
            lines.push(format!(
                "{count} — under its declared count; tighten the declaration"
            ));
        } else {
            lines.push(count);
        }
        lines.push(format!("    {}", tally.family.reason));
    }

    if !triage.anything_new() {
        lines.push("new: 0 — nothing survived beyond the declared expectations".to_owned());
    }
    if !triage.new.is_empty() {
        lines.push(format!(
            "new: {} — something new survived; each names a missing guard:",
            triage.new.len()
        ));
        for survivor in &triage.new {
            lines.push(format!(
                "  undeclared family: {} {}",
                operator_name(survivor.operator),
                moved_label(survivor)
            ));
            lines.push(format!(
                "  {} at {} ({}) — expected {}, nothing caught it; moved: {}; {:?} -> {:?}",
                operator_name(survivor.operator),
                survivor.site,
                &survivor.source_digest[survivor.source_digest.len().saturating_sub(8)..],
                containment_name(survivor.operator.expected_containment()),
                if survivor.affected_items.is_empty() {
                    "nothing".to_owned()
                } else {
                    survivor.affected_items.join(", ")
                },
                survivor.before,
                survivor.after,
            ));
        }
    }
    lines.join("\n")
}

/// A survivor's moved-behaviour, for naming the family it would have
/// had to be declared under.
fn moved_label(record: &MutationRecord) -> String {
    match record.affected_items.len() {
        0 => "moving nothing".to_owned(),
        1 => "moving one item".to_owned(),
        n => format!("moving {n} items"),
    }
}

fn operator_name(operator: MutationOperator) -> String {
    serde_json::to_value(operator)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{operator:?}"))
}

fn containment_name(containment: Containment) -> String {
    match containment {
        Containment::Guardrail(guardrail) => format!("the {guardrail:?} guardrail"),
        Containment::Scoring => "the eval gate to fail".to_owned(),
        Containment::NoEffect => "no observable change".to_owned(),
        Containment::CostShift => "the review cost to rise".to_owned(),
    }
}
