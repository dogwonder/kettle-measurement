//! A PDF fixture is scored through pdfium when the evaluator is told
//! where the reader is (#256). Before this, the eval built its runs with
//! `RunResources::default()`, so every PDF fixture in every bed failed
//! with "missing its PDF reader" — the PDF path was unmeasured by
//! construction, whatever the bed held.

#![cfg(feature = "pdf")]

use runner::eval::fixture::FixtureEvaluator;
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evaluator(fixtures: &Path, pdfium_dir: Option<PathBuf>) -> FixtureEvaluator {
    FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: MachineInfo {
            cpu: "Apple M1 Pro".to_owned(),
            ram_gb: 16,
            os: "macOS 15.5".to_owned(),
        },
        sidecar: None,
        peak_rss: None,
        fixtures_dir: Some(fixtures.to_path_buf()),
        runs_dir: None,
        resume_dir: None,
        pdfium_dir,
    }
}

#[test]
fn a_pdf_fixture_is_scored_through_pdfium_when_the_reader_is_named() {
    let sidecars = root().join("sidecars");
    if !runner::pdf::library_present(&sidecars) {
        // The same guard as tests/run.rs: libpdfium is vendored, never
        // committed, so CI cannot run this and says so loudly.
        eprintln!("skipping: no libpdfium in sidecars/ — see sidecars/README.md");
        return;
    }
    let pack_dir = root().join("packs/app.kttl.subscription-audit");
    let pack = load_pack(&pack_dir).expect("pack loads");

    // A bed of one PDF, paired the way discovery pairs: <stem>.pdf beside
    // <stem>.expected.json.
    let bed = std::env::temp_dir().join(format!("kettle-pdf-fixture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&bed);
    std::fs::create_dir_all(&bed).unwrap();
    std::fs::copy(
        pack_dir.join("fixtures/statement-04.pdf"),
        bed.join("statement-04.pdf"),
    )
    .unwrap();
    std::fs::copy(
        pack_dir.join("fixtures/statement-04.expected.json"),
        bed.join("statement-04.expected.json"),
    )
    .unwrap();

    // Naming no reader does not fail the run: #256 made a document this
    // build cannot open an *unrunnable fixture* rather than an error,
    // because failing the whole eval made the deterministic floor
    // untestable on any machine without pdfium — which is every CI
    // runner. What must still hold is that it is not quietly scored: it
    // is named, and it contributes nothing.
    //
    // This assertion used to expect an `Err`, and had been failing on
    // any machine with a vendored libpdfium since #575 while passing in
    // CI, where the body above skips. That is #603 from the other side,
    // and the reason the skip is now declared rather than merely
    // written.
    let without = evaluator(&bed, None)
        .evaluate(&pack)
        .expect("an unreadable fixture is reported, not raised");
    assert_eq!(
        without.unrunnable,
        vec!["statement-04.pdf".to_owned()],
        "the fixture this build cannot open is named in the report"
    );
    assert!(
        without.fixtures.is_empty(),
        "and nothing was scored from it: {:?}",
        without.fixtures
    );

    let report = evaluator(&bed, Some(sidecars))
        .evaluate(&pack)
        .expect("the reader is named, so the PDF is read and scored");
    assert_eq!(
        report.fixtures.len(),
        1,
        "the one PDF fixture was scored, not skipped"
    );
}
