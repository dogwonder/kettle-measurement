//! `kettle packs list` and `kettle run --dry-run` (#19).
//!
//! Both answer questions you ask *before* trusting a run: what can this
//! thing do, and what is it about to do. Neither spawns a sidecar and
//! neither reads a model — a dry run that needed the model would be
//! answering a different question.
//!
//! RED BY DESIGN: both functions are `todo!()`. The tests below are the
//! contract; the exact column widths are not.

use runner::cache::CacheKey;
use runner::packs::{Pack, PipelineStep};
use std::path::PathBuf;

/// One line per pack: id, name, version, what it accepts, and what it
/// is allowed to do. Capabilities are shown because "read-only" is the
/// promise the whole app rests on — it should be visible without
/// opening a manifest.
pub fn list_packs(packs: &[Pack]) -> String {
    let mut out = String::new();
    for pack in packs {
        let manifest = &pack.manifest;
        out.push_str(&format!(
            "{} — {} ({})\n",
            manifest.id, manifest.name, manifest.version
        ));
        // Each declared document on its own line, in the pack's own
        // words (#334 §3). Flattening every role's `accept` into one
        // list said a two-document pack "reads" its types twice and
        // never said it wanted two documents at all — the question
        // `packs list` exists to answer before you trust a run.
        for input in &manifest.inputs {
            let accepts: Vec<&str> = input.accept.iter().map(String::as_str).collect();
            out.push_str(&format!(
                "  reads: {} — {} ({})\n",
                input.label,
                accepts.join(", "),
                input.count.in_words(),
            ));
        }
        out.push_str(&format!("  may: {}\n\n", manifest.capabilities.join(", ")));
    }
    out
}

/// How many distinct merchants these inputs would group into — the same
/// deterministic grouping `run::run_pack` does before any model step, so
/// the batch counts below match what a real run would actually ask.
/// `Err` only when a file can't be read; the dry run says so and still
/// shows the rest of the plan.
fn count_merchants(inputs: &[PathBuf]) -> Result<usize, runner::parse::ParseError> {
    let mut raw_merchants: Vec<String> = Vec::new();
    for input in inputs {
        let parsed = runner::parse::parse_statement_file(input)?;
        raw_merchants.extend(parsed.transactions.into_iter().map(|t| t.raw_merchant));
    }

    let mut cleaned: Vec<String> = Vec::new();
    for raw in &raw_merchants {
        let name = runner::cleanup::clean_merchant(raw);
        if !cleaned.contains(&name) {
            cleaned.push(name);
        }
    }
    let names: Vec<&str> = cleaned.iter().map(String::as_str).collect();
    Ok(runner::cleanup::group_merchants(&names).len())
}

/// What a run would do, without doing it: the resolved pipeline in
/// order, each model step's batch count for these inputs, and whether
/// the results cache already holds this exact run.
pub fn dry_run(pack: &Pack, inputs: &[PathBuf], cached: Option<&CacheKey>) -> String {
    let mut out = String::new();

    if let Some(key) = cached {
        out.push_str(&format!(
            "The answer for this exact run is already known (cache entry {key}) \
             — running it again would ask nothing new.\n\n"
        ));
    }

    out.push_str(&format!(
        "{} ({})\n",
        pack.manifest.name, pack.manifest.version
    ));

    // Counting merchants is deterministic — parsing and grouping, no
    // model involved — so the dry run can say exactly how much asking
    // there will be rather than guessing.
    let merchants = count_merchants(inputs).ok();
    if merchants.is_none() {
        out.push_str("  (couldn't read the files, so the amount of asking can't be shown)\n");
    }

    let mut shown = 0usize;
    for step in &pack.manifest.pipeline {
        match step {
            PipelineStep::Preprocess { .. } => {
                shown += 1;
                out.push_str(&format!("{shown}. Reading your statement\n"));
            }
            PipelineStep::Model {
                schema: Some(_),
                role,
                ..
            } => {
                // The declared role, same as `run_pack` (#120) — the plan
                // has to describe the run that will actually happen, and
                // counting steps was how the two could disagree.
                let label = match runner::run::ModelRole::declared(role.as_deref()) {
                    Some(runner::run::ModelRole::Normalise) => "Grouping payments by merchant",
                    Some(runner::run::ModelRole::Classify) => "Sorting merchants",
                    Some(runner::run::ModelRole::Obligations) => "Reading what it asks of you",
                    Some(runner::run::ModelRole::PolicyTerms) => "Reading what each document says",
                    // Unreachable via `load_pack`, which refuses unknown
                    // roles — but the plan should not invent a name for
                    // a step it cannot describe.
                    None => "A step this Kettle doesn't know",
                };
                shown += 1;
                match merchants {
                    Some(count) => {
                        let items = vec![String::new(); count];
                        let batches = runner::packs::step_batches(step, &items)
                            .map(|batches| batches.len())
                            .unwrap_or(1);
                        let batch_word = if batches == 1 { "batch" } else { "batches" };
                        out.push_str(&format!(
                            "{shown}. {label} — {count} merchants, {batches} {batch_word}\n"
                        ));
                    }
                    None => out.push_str(&format!("{shown}. {label}\n")),
                }
            }
            PipelineStep::Model { schema: None, .. } => {
                // The optional prose summary: `run_pack` never reports
                // progress on it either, so the plan stays quiet here.
            }
            PipelineStep::Aggregate { implementation } => {
                // Each builtin's own progress label, as `run_pack`
                // reports it. One label for every aggregate step said
                // "Checking for price rises" over a letter's timeline
                // sort, which is the plan describing a run that was
                // never going to happen.
                let label = match implementation.as_str() {
                    "builtin:timeline-sort" => "Working out the deadlines",
                    "builtin:term-diff" => "Comparing the two documents",
                    _ => "Checking for price rises",
                };
                shown += 1;
                out.push_str(&format!("{shown}. {label}\n"));
            }
            PipelineStep::Render { .. } => {
                shown += 1;
                out.push_str(&format!("{shown}. Writing your report\n"));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use runner::packs::load_pack;
    use std::path::Path;

    fn pack() -> Pack {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit");
        load_pack(&dir).expect("the pack loads")
    }

    fn statement() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.subscription-audit/fixtures/statement-01.csv")
    }

    #[test]
    fn list_shows_what_a_pack_is_and_what_it_may_do() {
        let listed = list_packs(&[pack()]);

        assert!(listed.contains("app.kttl.subscription-audit"));
        assert!(listed.contains("Subscription & Recurring Spend Audit"));
        assert!(listed.contains("1.5.0"));
        assert!(listed.contains("read"), "the read-only promise is visible");
        assert!(
            listed.contains("csv") || listed.contains("CSV"),
            "and what you can give it: {listed}"
        );
    }

    /// #334 §3: a pack declaring two documents says so, in its own
    /// words. The flattened list said "text/plain, application/pdf,
    /// text/plain, application/pdf" and never mentioned that two
    /// documents were wanted — which is the question `packs list`
    /// exists to answer before anyone trusts a run.
    #[test]
    fn list_names_each_document_a_pack_asks_for() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff");
        let pack = runner::packs::load_pack(&dir).expect("the renewal pack loads");

        let listed = list_packs(&[pack]);

        assert!(listed.contains("Last year's policy"), "{listed}");
        assert!(listed.contains("This year's renewal"), "{listed}");
        assert!(
            listed.contains("one file"),
            "and how many of each: {listed}"
        );
    }

    #[test]
    fn the_plan_names_each_model_step_by_its_declared_role() {
        // #120: the plan describes the run that will happen, so it has
        // to read the manifest the same way `run_pack` does. Both used
        // to count schema-bearing steps, which meant a pack could be
        // described one way and executed another.
        let pack = pack();
        let roles: Vec<Option<&str>> = pack
            .manifest
            .pipeline
            .iter()
            .filter_map(|step| match step {
                PipelineStep::Model {
                    schema: Some(_),
                    role,
                    ..
                } => Some(role.as_deref()),
                _ => None,
            })
            .collect();
        assert_eq!(roles, [Some("normalise"), Some("classify")]);

        let plan = dry_run(&pack, &[statement()], None);
        let grouping = plan.find("Grouping payments by merchant");
        let sorting = plan.find("Sorting merchants");
        assert!(
            grouping.is_some() && sorting.is_some(),
            "both declared roles are described: {plan}"
        );
        assert!(
            grouping < sorting,
            "and in the order the manifest declares them: {plan}"
        );
    }

    #[test]
    fn dry_run_lists_the_pipeline_in_order_without_asking_the_model() {
        let plan = dry_run(&pack(), &[statement()], None);

        let steps: Vec<&str> = [
            "Reading your statement",
            "Grouping payments by merchant",
            "Sorting merchants",
            "Checking for price rises",
            "Writing your report",
        ]
        .into_iter()
        .collect();
        let mut at = 0;
        for step in steps {
            let found = plan[at..]
                .find(step)
                .unwrap_or_else(|| panic!("{step:?} missing or out of order in:\n{plan}"));
            at += found + step.len();
        }
    }

    #[test]
    fn dry_run_says_how_much_asking_there_will_be() {
        let plan = dry_run(&pack(), &[statement()], None);
        // statement-01 has six merchants; at batch sizes 20 and 15 that
        // is one batch each. Saying so is how you spot a pack that
        // would ask a thousand times.
        assert!(
            plan.contains("1 batch"),
            "the plan says how many times the model gets asked:\n{plan}"
        );
    }

    #[test]
    fn dry_run_says_when_the_answer_is_already_known() {
        let key = runner::cache::cache_key(
            "app.kttl.subscription-audit",
            "1.0.0",
            &[statement()],
            "test",
        )
        .expect("the statement is readable");
        let plan = dry_run(&pack(), &[statement()], Some(&key));
        assert!(
            plan.to_lowercase().contains("already"),
            "a cached run should say so rather than pretend it will work:\n{plan}"
        );
    }
}
