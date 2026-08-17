//! `kettle render` — make a committed report fixture through the same
//! renderer a real run uses (#225).

use std::path::{Path, PathBuf};

fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-render-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[test]
fn renders_results_through_the_runner_and_writes_the_requested_file() {
    let results = repo("fixtures/run-01/results.json");
    let template = repo("packs/app.kttl.subscription-audit/report.html.tera");
    let output = scratch("writes").join("report.html");

    cli::render::write_report(&results, &template, &output).expect("render the fixture");

    let report: runner::results::RunReport =
        serde_json::from_str(&std::fs::read_to_string(&results).unwrap()).unwrap();
    let expected =
        runner::render::render_report(&std::fs::read_to_string(&template).unwrap(), &report, None)
            .unwrap();
    assert_eq!(std::fs::read_to_string(output).unwrap(), expected);
}

#[test]
fn the_committed_viewer_fixture_matches_a_fresh_render() {
    let results = repo("fixtures/run-01/results.json");
    let template = repo("packs/app.kttl.subscription-audit/report.html.tera");
    let committed = repo("fixtures/run-01/report.html");

    let rendered = cli::render::report_from_files(&results, &template).expect("render the fixture");
    assert_eq!(
        std::fs::read_to_string(committed).unwrap(),
        rendered,
        "fixtures/run-01/report.html is stale; regenerate it with the command in \
         fixtures/run-01/README.md"
    );
}

/// A comparison report reaches the comparison renderer (#66).
///
/// Dispatch is on the document's own `schema` for the reason #243 gave:
/// fed to the audit renderer this would not fail, it would render a
/// page with every money field empty — the quiet wrong answer. A third
/// typology is where a two-branch dispatch would first go wrong
/// silently, so it is asserted rather than assumed.
#[test]
fn a_comparison_report_renders_through_its_own_renderer() {
    let dir = scratch("comparison");
    let results = dir.join("results.json");
    let template = repo("packs/app.kttl.renewal-diff/report.html.tera");

    let report = runner::results::ComparisonReport {
        schema: runner::results::COMPARISON_REPORT_SCHEMA.to_owned(),
        run: runner::results::ComparisonRunInfo {
            id: "renewal-01".to_owned(),
            pack: "app.kttl.renewal-diff".to_owned(),
            pack_version: "0.1.0".to_owned(),
            documents: vec![runner::results::ComparedDocument {
                role: "previous".to_owned(),
                label: "Last year's policy".to_owned(),
                file: "policy-2025.pdf".to_owned(),
            }],
            passages: 4,
            started: "2026-08-04T09:00:00Z".to_owned(),
            finished: "2026-08-04T09:00:20Z".to_owned(),
        },
        summary: runner::results::ComparisonSummary {
            terms_count: 0,
            changed_count: 0,
            unchanged_count: 0,
            added_count: 0,
            removed_count: 0,
            needs_review_count: 0,
            note: "Comparing the two documents, nothing Kettle reads has changed between them."
                .to_owned(),
        },
        changes: Vec::new(),
        needs_review: Vec::new(),
    };
    std::fs::write(&results, serde_json::to_string(&report).unwrap()).unwrap();

    let html = cli::render::report_from_files(&results, &template).expect("render the comparison");
    assert!(html.contains("policy-2025.pdf"), "{html}");
    assert!(html.contains("nothing Kettle reads has changed"));
}

#[test]
fn an_invalid_results_file_names_the_file_that_needs_attention() {
    let dir = scratch("invalid-results");
    let results = dir.join("results.json");
    let output = dir.join("report.html");
    std::fs::write(&results, "{}").unwrap();

    let error = cli::render::write_report(
        &results,
        &repo("packs/app.kttl.subscription-audit/report.html.tera"),
        &output,
    )
    .unwrap_err();

    assert!(error.contains(&results.display().to_string()), "{error}");
    assert!(!output.exists(), "a failed render must not leave an output");
}
