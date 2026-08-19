//! The table people read after an eval (#38, brief §6):
//!
//! ```text
//! app.kttl.subscription-audit v1.0.0 · 2 fixtures · 3 runs · Apple M1 8GB
//!
//! model                             normalise (pooled)              e2e (mean)  review (mean)  time   verdict
//! qwen2.5-3b-instruct-q4_k_m.gguf   0.88 (n=100; 95% CI 0.80–0.93)  0.96        12%            4m10s  PASS
//! ```
//!
//! One row per model, one column per binomial model-step measure. Counts
//! are pooled across fixtures: equal-weighting a ten-item statement and
//! an eighty-item statement would discard the denominator. Every
//! proportion carries `n` and its 95% Wilson interval.
//!
//! Classification is not a step-accuracy column. The lines below the
//! table report precision, surfaced recall and confident-wrong rate for
//! every class, overall and per item stratum, followed by one PASS/FAIL
//! line per declared ceiling. Every metric a report carries prints such
//! a block — extraction as well as classification (#286), because a
//! declared ceiling is the part a person is meant to act on and a
//! 115-minute measurement that hides its own headline finding is a
//! measurement people will misread. The verdict is not pooled: it
//! remains the evaluator's conjunctive pack judgement.

use runner::eval::relations::RelationOutcome;
use runner::eval::GateOutcome;
use runner::eval::{
    CalibrationReport, ClassPerformance, ClassificationGate as Gate, ClassificationSlice,
    EvalMetric, EvalReport, ExtractionSlice, HarmClass, MetricReport, ProportionEstimate,
    RankingSignal, Spread, Stability,
};
use runner::kinds::KindFrom;
use std::collections::{BTreeMap, BTreeSet};

/// The table for one pack, one row per model.
pub fn render(pack: &str, reports: &[&EvalReport], runs: u32) -> String {
    let Some(first) = reports.first() else {
        return format!("{pack}: nothing was measured.\n");
    };

    let has_classification_metrics = reports
        .iter()
        .any(|report| report.metrics.contains_key(&EvalMetric::Classification));
    let steps: Vec<String> = reports
        .iter()
        .flat_map(|report| report.fixtures.iter())
        .flat_map(|fixture| fixture.step_scores.keys().cloned())
        .filter(|step| !(has_classification_metrics && step == "classify"))
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();

    let mut headers = vec!["model".to_owned()];
    headers.extend(steps.iter().map(|step| format!("{step} (pooled)")));
    headers.extend([
        "e2e (mean)".to_owned(),
        "review (mean)".to_owned(),
        "time".to_owned(),
        "verdict".to_owned(),
    ]);

    let mut rows: Vec<Vec<String>> = vec![headers];
    for report in reports {
        let mut row = vec![report.model_name().to_owned()];
        for step in &steps {
            row.push(match pooled_step(report, step) {
                Some(estimate) => mark(format_estimate(&estimate), moved_step(report, step)),
                // A step this pack's fixtures never scored for this
                // model: an empty column, not a zero, which would read
                // as "got everything wrong".
                None => "-".to_owned(),
            });
        }
        row.push(mark(
            format!("{:.2}", mean(report.fixtures.iter().map(|f| f.end_to_end))),
            moved(report, |stability| stability.end_to_end),
        ));
        row.push(mark(
            format!(
                "{:.0}%",
                mean(report.fixtures.iter().map(|f| f.needs_review_rate)) * 100.0
            ),
            moved(report, |stability| stability.needs_review_rate),
        ));
        row.push(duration(
            report
                .fixtures
                .iter()
                .map(|fixture| fixture.perf.wall_ms)
                .sum(),
        ));
        row.push(report.verdict.label().to_owned());
        rows.push(row);
    }

    let mut out = format!(
        "{pack} v{} · {} set · {} · {} · {} {}GB\n\n",
        first.pack_version,
        first.eval_set.as_str(),
        count(first.fixtures.len(), "fixture"),
        count(runs as usize, "run"),
        first.machine.cpu,
        first.machine.ram_gb,
    );
    out.push_str(&columns(&rows));
    for report in reports {
        append_metrics(&mut out, report);
        append_containment(&mut out, report);
        append_relations(&mut out, report);
    }
    out
}

/// What the declared relations did (#453).
///
/// A failed relation fails the report on its own (#427), so a table
/// that printed none could show FAIL with every stated number passing
/// — which is what both v11 model measurements did on 8 August 2026.
/// The evidence runs the other way too: five held relations are a real
/// assurance result, and a run that holds every inversion law deserves
/// to say so.
///
/// Unjudgeable relations are counted separately and always carry their
/// reason. An unjudged relation reading as a held one is exactly what
/// `relations.rs` refuses at load time, and a bare tally would
/// reintroduce it at display time.
fn append_relations(out: &mut String, report: &EvalReport) {
    if report.relations.is_empty() {
        return;
    }
    let count_of = |matching: fn(&RelationOutcome) -> bool| {
        report
            .relations
            .iter()
            .filter(|relation| matching(&relation.outcome))
            .count()
    };
    let held = count_of(|outcome| matches!(outcome, RelationOutcome::Held));
    let failed = count_of(|outcome| matches!(outcome, RelationOutcome::Failed { .. }));
    let unjudgeable = count_of(|outcome| matches!(outcome, RelationOutcome::Unjudgeable { .. }));

    out.push('\n');
    out.push_str(&format!(
        "{} relations: {held} held, {failed} failed, {unjudgeable} unjudgeable\n",
        report.model_name()
    ));
    // Held ones are counted, not listed: the tally is the evidence, and
    // a line each would bury the failure the operator is scanning for.
    for relation in &report.relations {
        match &relation.outcome {
            RelationOutcome::Held => {}
            RelationOutcome::Failed { smallest_diff } => out.push_str(&format!(
                "relation / {}: FAILED; {smallest_diff}\n",
                relation.id
            )),
            RelationOutcome::Unjudgeable { because } => out.push_str(&format!(
                "relation / {}: unjudgeable; {because}\n",
                relation.id
            )),
        }
    }
}

fn append_containment(out: &mut String, report: &EvalReport) {
    let candidates: usize = report
        .fixtures
        .iter()
        .map(|fixture| fixture.containment.candidates)
        .sum();
    let contained: usize = report
        .fixtures
        .iter()
        .map(|fixture| fixture.containment.contained)
        .sum();
    let escaped: usize = report
        .fixtures
        .iter()
        .map(|fixture| fixture.containment.escaped)
        .sum();
    let pipeline_introduced: usize = report
        .fixtures
        .iter()
        .map(|fixture| fixture.containment.pipeline_introduced)
        .sum();
    if candidates == 0 && contained == 0 && escaped == 0 {
        return;
    }

    out.push('\n');
    out.push_str(&format!(
        "{} claim containment: {candidates} candidates; {contained} scored decisions surfaced; {escaped} wrong assertions escaped",
        report.model_name()
    ));
    // #470: never folded into containment or escapes — this is the
    // count of errors the pipeline itself introduced, and it prints
    // only when one was recorded, because a permanent zero would be a
    // claim the run never measured.
    if pipeline_introduced > 0 {
        out.push_str(&format!(
            "; {pipeline_introduced} errors the pipeline introduced"
        ));
    }
    out.push('\n');
    let guardrails: BTreeSet<_> = report
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.containment.by_guardrail.keys().copied())
        .collect();
    for guardrail in guardrails {
        let (failed, contained, escaped) =
            report
                .fixtures
                .iter()
                .fold((0, 0, 0), |(failed, contained, escaped), fixture| {
                    let cell = fixture.containment.by_guardrail.get(&guardrail);
                    (
                        failed + cell.map_or(0, |cell| cell.failed),
                        contained + cell.map_or(0, |cell| cell.contained),
                        escaped + cell.map_or(0, |cell| cell.escaped),
                    )
                });
        out.push_str(&format!(
            "claim guardrail / {}: {failed} failed; {contained} contained; {escaped} escaped\n",
            guardrail_name(guardrail)
        ));
    }
}

fn guardrail_name(guardrail: runner::claim_trace::Guardrail) -> &'static str {
    use runner::claim_trace::Guardrail;
    match guardrail {
        Guardrail::Schema => "schema",
        Guardrail::Pairing => "pairing",
        Guardrail::Quote => "quote",
        Guardrail::QuoteIdentifiesPassage => "quote_identifies_passage",
        Guardrail::ValueShape => "value_shape",
        Guardrail::PackCoverage => "pack_coverage",
        Guardrail::DeterministicDerivation => "deterministic_derivation",
        Guardrail::ReviewRouting => "review_routing",
        Guardrail::Report => "report",
        Guardrail::Action => "action",
    }
}

/// The marker on a number the repeats disagreed about (#83).
///
/// It goes on the number itself, where the number is read. The warning
/// underneath says what moved and by how much, but a table read on its
/// own should not be able to look steady when it wasn't.
const MOVED: char = '⚠';

fn mark(cell: String, moved: bool) -> String {
    if moved {
        format!("{cell} {MOVED}")
    } else {
        cell
    }
}

/// Whether any fixture's repeats disagreed about one step.
fn moved_step(report: &EvalReport, step: &str) -> bool {
    report.fixtures.iter().any(|fixture| {
        fixture
            .stability
            .as_ref()
            .and_then(|stability| stability.steps.get(step))
            .is_some_and(Spread::moved)
    })
}

/// Whether any fixture's repeats disagreed about the number `pick`
/// pulls out of its stability block.
fn moved(report: &EvalReport, pick: impl Fn(&Stability) -> Spread) -> bool {
    report
        .fixtures
        .iter()
        .filter_map(|fixture| fixture.stability.as_ref())
        .any(|stability| pick(stability).moved())
}

/// Pool item counts across statements. Equal-weighting fixture means
/// would make a ten-item statement count the same as a one-item one.
fn pooled_step(report: &EvalReport, step: &str) -> Option<ProportionEstimate> {
    let scores: Vec<_> = report
        .fixtures
        .iter()
        .filter_map(|fixture| fixture.step_scores.get(step))
        .collect();
    (!scores.is_empty()).then(|| {
        ProportionEstimate::from_counts(
            scores.iter().map(|score| score.correct).sum(),
            scores.iter().map(|score| score.expected).sum(),
        )
    })
}

fn format_estimate(estimate: &ProportionEstimate) -> String {
    match (estimate.estimate, estimate.wilson_95) {
        (Some(value), Some(interval)) => format!(
            "{value:.2} (n={}; 95% CI {:.2}\u{2013}{:.2})",
            estimate.n, interval.low, interval.high
        ),
        _ => format!("undefined (n={}; 95% CI undefined)", estimate.n),
    }
}

/// Every metric the report carries, one block each.
///
/// The `match` is exhaustive deliberately (#286). This used to ask
/// `metrics.get(&EvalMetric::Classification)` and bind one variant, so
/// when #279 added Extraction the letter pack's per-class numbers and
/// its declared ceilings were computed, stored in the report, written to
/// the baseline — and never printed. Nothing failed; the lookup simply
/// found nothing and returned. A metric that can be computed but not
/// displayed is a metric that will be forgotten, so a third variant must
/// now stop the build here rather than go quiet on screen.
///
/// It is the payload enum that is matched, not the [`EvalMetric`] key: a
/// new key can only be reported by carrying a payload, so the payload is
/// where the compiler can hold the line.
fn append_metrics(out: &mut String, report: &EvalReport) {
    for metric in report.metrics.values() {
        match metric {
            MetricReport::Classification(metrics) => {
                append_heading(out, report, CLASSIFICATION);
                append_classification_slice(out, "overall", &metrics.overall);
                for (stratum, slice) in &metrics.strata {
                    append_classification_slice(out, stratum, slice);
                }
                append_gates(out, &metrics.gates);
                append_calibration(out, report, &metrics.calibration);
            }
            MetricReport::Extraction(metrics) => {
                append_heading(out, report, EXTRACTION);
                append_extraction_slice(out, "overall", &metrics.overall);
                for (stratum, slice) in &metrics.strata {
                    append_extraction_slice(out, stratum, slice);
                }
                append_gates(out, &metrics.gates);
                append_calibration(out, report, &metrics.calibration);
            }
        }
    }
}

/// What the declared confidence was worth on this run (#429).
///
/// Printed with the metrics rather than beside the verdict on purpose:
/// it is evidence about an answer's *label*, and no gate is read from
/// it. The line that matters most is the last one, because a table of
/// buckets reads like a ranking whether or not the buckets ranked
/// anything.
fn append_calibration(out: &mut String, report: &EvalReport, calibration: &CalibrationReport) {
    if calibration.buckets.is_empty() && calibration.untraceable == 0 {
        return;
    }
    let model = report.model_name();
    for (level, bucket) in &calibration.buckets {
        out.push_str(&format!(
            "confidence / {level}: {} decisions; {} wrong; {} routed to review; error {}\n",
            bucket.decisions,
            bucket.errors,
            bucket.routed_to_review,
            format_estimate(&bucket.error_rate),
        ));
    }
    if calibration.untraceable > 0 {
        out.push_str(&format!(
            "confidence / untraceable: {} decision(s) no single declared level \
             answered for — counted, never assigned\n",
            calibration.untraceable
        ));
    }
    out.push_str(&match &calibration.signal {
        RankingSignal::NoEvidence => {
            format!("{model} confidence: no decision carried a level to read\n")
        }
        RankingSignal::NoVariation { level } => format!(
            "{model} confidence: NO RANKING SIGNAL — every decision was declared \
             `{level}`, so routing by it separates nothing\n"
        ),
        RankingSignal::Unproven => format!(
            "{model} confidence: UNPROVEN — the levels varied and no two intervals \
             separated, so this bed has not shown one is safer than another\n"
        ),
        RankingSignal::Ranks => format!(
            "{model} confidence: ranks — a more confident bucket is measurably \
             safer than a less confident one\n"
        ),
        RankingSignal::Inverted {
            more_confident,
            less_confident,
            withdrawn_by,
        } => format!(
            "{model} confidence: INVERTED — `{more_confident}` is measurably *more* \
             wrong than `{less_confident}`, so routing the less confident \
             decisions to review spends the review on the wrong ones \
             ({} further error{} in `{less_confident}` would withdraw this)\n",
            withdrawn_by,
            if *withdrawn_by == 1 { "" } else { "s" },
        ),
    });
}

/// What each block's lines are prefixed with — the question the numbers
/// answer, so two metrics in one report can't be read as one.
const CLASSIFICATION: &str = "classification";
const EXTRACTION: &str = "extraction";

fn append_heading(out: &mut String, report: &EvalReport, metric: &str) {
    out.push('\n');
    out.push_str(&format!("{} {metric} metrics\n", report.model_name()));
}

/// The declared ceilings and their verdicts. Shared by both metrics: a
/// ceiling means the same thing whichever question it is a ceiling on,
/// which is why [`runner::eval`] computes them in one place too.
fn append_gates(out: &mut String, gates: &BTreeMap<String, BTreeMap<HarmClass, Gate>>) {
    for (stratum, classes) in gates {
        for (class, gate) in classes {
            out.push_str(&format!(
                "gate / {stratum} / {}: confident-wrong over decisions {} <= {:.2}: {}{}; {} ({})\n",
                harm_class_name(*class),
                format_estimate(&gate.observed),
                gate.max_wilson_95,
                gate.outcome.label(),
                // An unproven ceiling says what it would take, because
                // the shortfall is the actionable part: grow the bed by
                // this much, or declare a ceiling this bed can carry.
                match gate.outcome {
                    GateOutcome::Unproven { decisions_needed } => format!(
                        " — needs {decisions_needed} decisions, has {}",
                        gate.observed.n
                    ),
                    _ => String::new(),
                },
                gate.reason,
                gate.date,
            ));
        }
    }
}

fn append_classification_slice(out: &mut String, stratum: &str, slice: &ClassificationSlice) {
    append_harm_classes(out, CLASSIFICATION, stratum, &slice.harm_classes);
    for (dimension, classes) in [("kind", &slice.kinds), ("category", &slice.categories)] {
        for (class, performance) in classes {
            out.push_str(&format!(
                "classification / {stratum} / {dimension} / {class}: {}\n",
                format_performance(performance),
            ));
        }
    }
}

/// Extraction has no kind or category dimension: the miss/invent lens
/// *is* the question, so a slice is its harm classes and nothing else.
fn append_extraction_slice(out: &mut String, stratum: &str, slice: &ExtractionSlice) {
    append_harm_classes(out, EXTRACTION, stratum, &slice.harm_classes);
}

fn append_harm_classes(
    out: &mut String,
    metric: &str,
    stratum: &str,
    classes: &BTreeMap<HarmClass, ClassPerformance>,
) {
    for (class, performance) in classes {
        out.push_str(&format!(
            "{metric} / {stratum} / harm / {}: {}\n",
            harm_class_name(*class),
            format_performance(performance),
        ));
    }
}

fn format_performance(performance: &ClassPerformance) -> String {
    format!(
        "precision {}; recall {}; confident-wrong {}; over decisions {}{}",
        format_estimate(&performance.precision),
        format_estimate(&performance.recall),
        format_estimate(&performance.confident_wrong_rate),
        // Both, always, and in this order (#310): the row rate is what
        // a person's statement would show them, the decision rate is
        // what the ceiling judges. Printing only one invites reading it
        // as the other.
        format_estimate(&performance.confident_wrong_distinct),
        format_paths(&performance.confident_wrong_by_path),
    )
}

/// Which decisions produced the confident-wrong cell (#272), on the
/// line the cell is read.
///
/// Nothing is printed when the decomposition is empty, because empty
/// means "not recorded" — a metric with no kind, or an item written
/// before the branch was written down. Printing "via nothing" would
/// claim knowledge the record does not have.
fn format_paths(paths: &BTreeMap<KindFrom, usize>) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let named: Vec<String> = paths
        .iter()
        .map(|(path, count)| format!("{} {count}", path_name(*path)))
        .collect();
    format!(" via {}", named.join(", "))
}

fn path_name(path: KindFrom) -> &'static str {
    match path {
        KindFrom::CategoryMap => "category_map",
        KindFrom::Cadence => "cadence",
        KindFrom::CadenceDespitePeriodic => "cadence_despite_periodic",
        KindFrom::Income => "income",
    }
}

fn harm_class_name(class: HarmClass) -> &'static str {
    match class {
        HarmClass::Subscription => "subscription",
        HarmClass::NotSubscription => "not_subscription",
        HarmClass::Obligation => "obligation",
        HarmClass::NoObligation => "no_obligation",
    }
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let values: Vec<f32> = values.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

/// "2 fixtures", "1 run".
fn count(how_many: usize, thing: &str) -> String {
    let plural = if how_many == 1 { "" } else { "s" };
    format!("{how_many} {thing}{plural}")
}

/// Milliseconds as "4m10s" or "45s" — the "in four minutes" half of a
/// tier claim (brief §5).
fn duration(milliseconds: u64) -> String {
    let seconds = (milliseconds as f64 / 1000.0).round() as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}

/// Rows padded into aligned columns, two spaces between.
fn columns(rows: &[Vec<String>]) -> String {
    let width = |column: usize| {
        rows.iter()
            .filter_map(|row| row.get(column))
            .map(|cell| cell.chars().count())
            .max()
            .unwrap_or(0)
    };
    let widths: Vec<usize> = (0..rows.iter().map(Vec::len).max().unwrap_or(0))
        .map(width)
        .collect();

    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == row.len() {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{cell:<width$}  ", width = widths[column]));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use runner::eval::fixture::EvalSelection;
    use runner::eval::relations::{Projection, RelationKind, RelationOutcome, RelationResult};
    use runner::eval::{MachineInfo, ModelInfo, Verdict};

    fn report(relations: Vec<RelationResult>) -> EvalReport {
        EvalReport {
            reused_fixtures: 0,
            pack: "app.kttl.letter-to-actions".to_owned(),
            pack_version: "0.2.0".to_owned(),
            eval_set: EvalSelection::Development,
            model: Some(ModelInfo {
                file: "qwen3.5-4b-q4_k_m.gguf".to_owned(),
                params: "4B".to_owned(),
                quant: "Q4_K_M".to_owned(),
                context: 8192,
            }),
            machine: MachineInfo {
                cpu: "Apple M1 Pro".to_owned(),
                ram_gb: 32,
                os: "macOS 26.5.2".to_owned(),
            },
            evidence: None,
            relations,
            sidecar: None,
            fixtures: Vec::new(),
            bed: None,
            runtime: None,
            metrics: BTreeMap::new(),
            verdict: Verdict::Fail,
        }
    }

    fn relation(id: &str, outcome: RelationOutcome) -> RelationResult {
        RelationResult {
            id: id.to_owned(),
            kind: RelationKind::Invariance {
                projection: Projection::ObligationsSet,
            },
            left: "development-payment-01".to_owned(),
            right: "development-payment-01-reordered".to_owned(),
            outcome,
        }
    }

    /// #453. A failed relation flips the verdict to FAIL on its own
    /// (#427), so a table that prints no relation prints FAIL with
    /// every stated number passing. It must name the relation and the
    /// smallest difference that failed it — the field the judge
    /// already carries.
    #[test]
    fn a_failed_relation_is_named_with_the_difference_that_failed_it() {
        let report = report(vec![relation(
            "adv-injection-footer-omit-or-invent-holds-development",
            RelationOutcome::Failed {
                smallest_diff: "only the right side asserts : response/Trentham & Yale \
                                Solicitors/no particular date/undated"
                    .to_owned(),
            },
        )]);

        let out = render("app.kttl.letter-to-actions", &[&report], 1);

        assert!(
            out.contains("adv-injection-footer-omit-or-invent-holds-development"),
            "the failed relation must be named: {out}"
        );
        assert!(
            out.contains("only the right side asserts"),
            "and say what differed: {out}"
        );
    }

    /// The other direction. Five held relations are an assurance
    /// result — role-swap inversion holding on three shapes, two
    /// injections moving nothing — and a table that shows none of it
    /// hides evidence, not just a failure.
    #[test]
    fn the_tally_counts_held_failed_and_unjudgeable() {
        let report = report(vec![
            relation("reorder-holds", RelationOutcome::Held),
            relation("swap-inverts", RelationOutcome::Held),
            relation(
                "injection-holds",
                RelationOutcome::Failed {
                    smallest_diff: "only the left side asserts : payment".to_owned(),
                },
            ),
            relation(
                "unicode-confusables-holds",
                RelationOutcome::Unjudgeable {
                    because: "development-payment-01 routed item payment-01 to review".to_owned(),
                },
            ),
        ]);

        let out = render("app.kttl.letter-to-actions", &[&report], 1);

        assert!(
            out.contains("relations: 2 held, 1 failed, 1 unjudgeable"),
            "the tally is the line an operator scans: {out}"
        );
    }

    /// An unjudged relation reading as a held one is exactly what
    /// `relations.rs` refuses at load time; a silent tally would
    /// reintroduce it at display time. Non-zero means say why.
    #[test]
    fn an_unjudgeable_relation_says_why_it_could_not_be_judged() {
        let report = report(vec![relation(
            "reorder-holds",
            RelationOutcome::Unjudgeable {
                because: "development-payment-01 routed item payment-01 to review".to_owned(),
            },
        )]);

        let out = render("app.kttl.letter-to-actions", &[&report], 1);

        assert!(
            out.contains("routed item payment-01 to review"),
            "an unjudgeable relation must carry its reason: {out}"
        );
    }

    /// A pack that declares no relations says nothing about them. The
    /// subscription table must not grow a line of zeroes.
    #[test]
    fn a_pack_with_no_relations_prints_no_relation_block() {
        let out = render("app.kttl.letter-to-actions", &[&report(Vec::new())], 1);

        assert!(
            !out.contains("relations:"),
            "nothing declared, nothing to report: {out}"
        );
    }

    #[test]
    fn minutes_and_seconds() {
        assert_eq!(duration(45_000), "45s");
        assert_eq!(duration(250_000), "4m10s");
        assert_eq!(duration(3_605_000), "60m05s");
    }

    #[test]
    fn counts_are_plural_unless_there_is_one() {
        assert_eq!(count(1, "run"), "1 run");
        assert_eq!(count(3, "run"), "3 runs");
        assert_eq!(count(0, "fixture"), "0 fixtures");
    }
}
