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
mod support;

/// Minimal synthetic PDFs with explicit blank pages, generated without
/// depending on another reader or a platform drawing API.
fn write_letter_pdf(path: &Path, texts: &[&str]) {
    let kids = (0..texts.len())
        .map(|i| format!("{} 0 R", 3 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!("<< /Type /Pages /Count {} /Kids [{kids}] >>", texts.len()),
    ];
    for (i, text) in texts.iter().enumerate() {
        let stream = if text.is_empty() {
            String::new()
        } else {
            format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET\n")
        };
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents {} 0 R >>", 4 + i * 2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}endstream",
            stream.len()
        ));
    }
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    std::fs::write(path, pdf).unwrap();
}

#[test]
fn the_letter_limit_counts_pdf_pages_including_blank_pages_before_model_use() {
    let sidecars = root().join("sidecars");
    if !runner::pdf::library_present(&sidecars) {
        eprintln!("skipping: no libpdfium in sidecars/ — see sidecars/README.md");
        return;
    }
    let dir = std::env::temp_dir().join(format!("kettle-letter-page-limit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_letter_pdf(
        &dir.join("three.pdf"),
        &["10 March 2026", "A synthetic letter.", ""],
    );
    write_letter_pdf(
        &dir.join("four.pdf"),
        &["10 March 2026", "A synthetic letter.", "", ""],
    );
    std::fs::write(dir.join("extra.txt"), "Another page.").unwrap();
    let pack = load_pack(&root().join("packs/app.kttl.letter-to-actions")).unwrap();
    assert_eq!(pack.manifest.inputs[0].max_pages, Some(3));
    let run = |paths: &[PathBuf], answers: &Answers| {
        runner::run::run_pack_with_resources(
            &pack,
            paths,
            answers,
            runner::run::RunResources {
                pdfium_dir: Some(&sidecars),
            },
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_| {},
            &runner::run_dir::NoLog,
        )
    };
    let read = runner::document::read_document_parts_limited(
        &[dir.join("three.pdf").as_path()],
        0,
        Some(&sidecars),
        Some(3),
    )
    .unwrap();
    assert_eq!(
        read.pages, 3,
        "a blank final page counts even without a passage"
    );
    run(&[dir.join("three.pdf")], &Answers::WithoutModel)
        .expect("three physical pages are accepted");
    for paths in [
        vec![dir.join("four.pdf")],
        vec![dir.join("three.pdf"), dir.join("extra.txt")],
    ] {
        let mock = support::MockModel::respond_once(
            "200 OK",
            support::completion_envelope(r#"{"results":[]}"#),
        );
        let error = run(&paths, &Answers::FromModel(mock.endpoint())).unwrap_err();
        assert!(
            matches!(
                error,
                runner::run::RunError::Parse(runner::parse::ParseError::TooManyPages { max: 3 })
            ),
            "{error:?}"
        );
        mock.assert_no_request();
    }
    std::fs::remove_dir_all(dir).unwrap();
}

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
