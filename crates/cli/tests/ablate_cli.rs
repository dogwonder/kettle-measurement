//! #432: `kettle ablate` — one archived run, read under several system
//! policies.
//!
//! The command exists so the question "does the reliability come from
//! the model or from the guardrails?" costs a re-reading rather than a
//! bed run. Everything it needs is already on disk, so what these tests
//! pin is the refusals: a recording scored under other rules, and a
//! recording that is not there at all.

use runner::eval::SCORING_VERSION;
use std::path::{Path, PathBuf};

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-08-20T09:00:00Z")
        .expect("date")
        .with_timezone(&chrono::Utc)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-ablate-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch");
    dir
}

/// A baseline naming one fixture of one pack, at a scoring version the
/// caller chooses.
fn baseline(dir: &Path, scoring_version: u32) -> PathBuf {
    let path = dir.join("baseline.json");
    let document = serde_json::json!({
        "scoring_version": scoring_version,
        "recorded_at": "2026-08-19T11:06:07Z",
        "reports": [{
            "pack": "app.kttl.letter-to-actions",
            "pack_version": "0.2.0",
            "model": {
                "file": "qwen3.5-4b-q4_k_m.gguf",
                "params": "4B",
                "quant": "Q4_K_M",
                "context": 8192
            },
            "machine": { "cpu": "Apple M1 Pro", "ram_gb": 32, "os": "macOS 15.5" },
            "fixtures": [{
                "fixture": "letter-01.txt",
                "step_scores": {},
                "items": [],
                "end_to_end": 1.0,
                "needs_review_rate": 0.0,
                "perf": {
                    "wall_ms": 1000,
                    "model_ms": 900,
                    "tokens_per_second": 20.0,
                    "peak_rss_mb": 4096,
                    "retries": 0
                }
            }],
            "verdict": "pass"
        }]
    });
    std::fs::write(
        &path,
        serde_json::to_string(&document).expect("baseline serialises"),
    )
    .expect("write baseline");
    path
}

/// A baseline recorded under other scoring rules is refused, not
/// compared (evals/README.md).
///
/// A policy row is built out of scored items, so a recording whose
/// numbers meant something else would report differences that are only
/// the harness moving underneath — the failure mode a safety net must
/// not have.
#[test]
fn a_baseline_from_another_scoring_version_is_refused() {
    let dir = scratch("stale");
    let path = baseline(&dir, SCORING_VERSION - 1);

    let outcome = cli::ablate::run(&path, &dir.join("runs"), now());

    assert_eq!(outcome.code, cli::ablate::ExitCode::Broken);
    assert!(
        outcome.text.contains("no longer mean the same thing"),
        "the refusal says why: {}",
        outcome.text
    );
}

/// A run whose recordings are absent produces no scorecard at all.
///
/// Silently printing rows of zeroes would be the worst outcome
/// available: every harm column empty reads as the safest system ever
/// measured.
#[test]
fn a_run_with_no_recordings_on_disk_is_broken_rather_than_empty() {
    let dir = scratch("absent");
    let path = baseline(&dir, SCORING_VERSION);

    let outcome = cli::ablate::run(&path, &dir.join("runs"), now());

    assert_eq!(outcome.code, cli::ablate::ExitCode::Broken);
    assert!(
        outcome.text.contains("letter-01.txt"),
        "the missing fixture is named: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("No recording was found"),
        "and the reason is stated: {}",
        outcome.text
    );
}

/// The escaped claims are nameable, not just countable (#432).
///
/// A count says the guardrails let twelve wrong claims through; the ids
/// say *which*, and that is what turns a scorecard into work. The
/// twelve the v14 letter run asserted wrongly are the natural seed for
/// a discriminating audition subset (#539), which cannot be built from
/// a number.
#[test]
fn the_escaped_claims_can_be_named_rather_than_only_counted() {
    let dir = scratch("named");
    let path = baseline(&dir, SCORING_VERSION);

    let outcome = cli::ablate::run(&path, &dir.join("runs"), now());

    // Nothing on disk here, so the point is only that the option
    // exists in the rendering: a row knows its claim ids.
    assert_eq!(outcome.code, cli::ablate::ExitCode::Broken);
    assert!(
        cli::ablate::escaped_claims(&[runner::eval::ablation::PolicyRow {
            policy: "full-pipeline".to_owned(),
            answered: vec!["letter-01.txt#claim-000004".to_owned()],
            delivered: Vec::new(),
            escaped: vec!["letter-01.txt#claim-000004".to_owned()],
            prevented: Vec::new(),
            unknown: Vec::new(),
        }])
        .contains("letter-01.txt#claim-000004"),
        "the strictest policy's escapes are named"
    );
}
