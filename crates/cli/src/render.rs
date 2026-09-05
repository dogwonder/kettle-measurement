//! Render a saved `results.json` through a pack report template.
//!
//! This is deliberately a thin edge around [`runner::render`]: fixture
//! regeneration and a real run must not acquire two rendering paths.

use runner::results::{
    ComparisonReport, LetterReport, RunReport, COMPARISON_REPORT_SCHEMA, LETTER_REPORT_SCHEMA,
};
use std::path::Path;

/// Read `results` and `template`, then return the self-contained report.
///
/// Which report it is comes from the document's own `schema` field, not
/// from the caller guessing (#243). A letter report fed to the audit
/// renderer would not fail loudly — it would render a page with every
/// money field empty, which is the quiet wrong answer this project
/// keeps refusing.
pub fn report_from_files(results: &Path, template: &Path) -> Result<String, String> {
    let raw_results = std::fs::read_to_string(results)
        .map_err(|e| format!("Could not read {}: {e}", results.display()))?;
    let template_source = std::fs::read_to_string(template)
        .map_err(|e| format!("Could not read {}: {e}", template.display()))?;
    let schema = serde_json::from_str::<serde_json::Value>(&raw_results)
        .map_err(|e| format!("{} is not a Kettle results file: {e}", results.display()))?
        .get("schema")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();

    if schema == LETTER_REPORT_SCHEMA {
        let report: LetterReport = serde_json::from_str(&raw_results)
            .map_err(|e| format!("{} is not a Kettle letter report: {e}", results.display()))?;
        return runner::render::render_letter_report(&template_source, &report)
            .map_err(|e| format!("Could not render {}: {e}", results.display()));
    }

    if schema == COMPARISON_REPORT_SCHEMA {
        let report: ComparisonReport = serde_json::from_str(&raw_results).map_err(|e| {
            format!(
                "{} is not a Kettle comparison report: {e}",
                results.display()
            )
        })?;
        return runner::render::render_comparison_report(&template_source, &report)
            .map_err(|e| format!("Could not render {}: {e}", results.display()));
    }

    if schema != runner::results::RUN_REPORT_SCHEMA {
        // A family this build knows at a version it does not read is
        // another Kettle's document, not a broken file (#419, the
        // persisted-schema policy in `runner::results::schema_version`).
        if let Some((family, version)) = runner::results::schema_version(&schema) {
            if ["run-report", "letter-report", "comparison-report"].contains(&family) {
                return Err(format!(
                    "{} was written by another version of Kettle ({schema}, version {version}); \
                     this build reads {}, {} and {}",
                    results.display(),
                    runner::results::RUN_REPORT_SCHEMA,
                    LETTER_REPORT_SCHEMA,
                    COMPARISON_REPORT_SCHEMA
                ));
            }
        }
        return Err(format!(
            "{} is not a Kettle results file: unknown schema {schema:?}",
            results.display()
        ));
    }
    let report: RunReport = serde_json::from_str(&raw_results)
        .map_err(|e| format!("{} is not a Kettle results file: {e}", results.display()))?;
    runner::render::render_report(&template_source, &report, None)
        .map_err(|e| format!("Could not render {}: {e}", results.display()))
}

/// Render to an explicit destination.
pub fn write_report(results: &Path, template: &Path, output: &Path) -> Result<(), String> {
    let html = report_from_files(results, template)?;
    std::fs::write(output, html).map_err(|e| format!("Could not write {}: {e}", output.display()))
}
