// The progress sequence derives from the pipeline, in the runner —
// the same source the run emits from — so the app never authors a
// pack's step list per pack id (#244). Sequences are asserted against
// the three real packs: a label changed in one place and not the other
// is the drift this test exists to catch.

use std::path::Path;

use runner::packs::{load_pack, Pack};
use runner::run::step_labels;

fn pack(name: &str) -> Pack {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(name);
    load_pack(&dir).expect("real pack loads")
}

#[test]
fn the_progress_sequence_derives_from_the_pipeline() {
    assert_eq!(
        step_labels(&pack("app.kttl.subscription-audit").manifest),
        [
            "Reading your statement",
            "Grouping payments by merchant",
            "Sorting merchants",
            "Checking for price rises",
            "Writing your report",
        ]
    );
    assert_eq!(
        step_labels(&pack("app.kttl.letter-to-actions").manifest),
        [
            "Reading your document",
            "Reading what it asks of you",
            "Working out the deadlines",
            "Writing your report",
        ]
    );
    assert_eq!(
        step_labels(&pack("app.kttl.renewal-diff").manifest),
        [
            "Reading your documents",
            "Reading what each document says",
            "Comparing the two documents",
            "Writing your report",
        ]
    );
}

#[test]
fn the_optional_prose_summary_has_no_progress_label() {
    // The summary step emits no progress (its deterministic fallback
    // makes skipping it honest, #33), so the sequence must not promise
    // a row the run will never mark done.
    let manifest = &pack("app.kttl.subscription-audit").manifest;
    let prose_steps = manifest
        .pipeline
        .iter()
        .filter(|step| {
            matches!(
                step,
                runner::packs::PipelineStep::Model { schema: None, .. }
            )
        })
        .count();
    assert_eq!(prose_steps, 1, "the subscription pack carries the summary");
    assert_eq!(step_labels(manifest).len(), manifest.pipeline.len() - 1);
}
