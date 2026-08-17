//! `kettle doctor` — plain-language checks that the pieces Kettle needs
//! are in place: the llama-server binary, that it starts and responds,
//! and whether a model file is present yet.

use runner::sidecar::{Sidecar, SidecarRuntime};
use std::path::Path;
use std::time::Duration;

pub struct DoctorReport {
    pub lines: Vec<String>,
    pub all_ok: bool,
}

pub fn run_checks(sidecar_binary: &Path, models_dir: &Path, sidecars_dir: &Path) -> DoctorReport {
    let mut lines = Vec::new();
    let mut all_ok = true;

    // The model is found first because the start-up check needs it:
    // llama-server without `-m` starts in router mode with nothing
    // loaded and still answers /health, so a check that spawns it bare
    // proves only that the file is executable. This check exists to
    // answer "can Kettle actually run?", and that means loading the
    // model a run would use.
    let model = first_gguf(models_dir);

    if sidecar_binary.is_file() {
        lines.push(format!(
            "✓ llama-server binary: found ({})",
            sidecar_binary.display()
        ));
        match &model {
            None => {
                lines.push("– Starts up: skipped (no model to load yet)".into());
                lines.push("– Responds when asked: skipped (no model to load yet)".into());
            }
            Some(name) => {
                let log =
                    std::env::temp_dir().join(format!("kettle-doctor-{}.log", std::process::id()));
                match Sidecar::spawn(
                    sidecar_binary,
                    &models_dir.join(name),
                    &log,
                    SidecarRuntime::default(),
                ) {
                    Ok(mut sidecar) => {
                        lines.push("✓ Starts up: yes".into());
                        // Loading several gigabytes off disk is not
                        // quick, and the first run after a restart is
                        // the slowest. Waiting is the honest thing.
                        match sidecar.wait_until_ready(Duration::from_secs(180)) {
                            Ok(()) => lines.push("✓ Responds when asked: yes".into()),
                            Err(e) => {
                                all_ok = false;
                                lines.push(format!("✗ Responds when asked: no — {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        all_ok = false;
                        lines.push(format!("✗ Starts up: no — {e}"));
                        lines.push("– Responds when asked: skipped (did not start)".into());
                    }
                }
            }
        }
    } else {
        all_ok = false;
        lines.push(format!(
            "✗ llama-server binary: not found — expected {}",
            sidecar_binary.display()
        ));
        lines.push("– Starts up: skipped (no binary to try)".into());
        lines.push("– Responds when asked: skipped (no binary to try)".into());
    }

    lines.push(match &model {
        Some(name) => format!("✓ Model file: found ({name})"),
        None => format!(
            "– Model file: none yet — put a .gguf file in {}",
            models_dir.display()
        ),
    });

    // PDF reading is optional either way — CSV is the recommended path,
    // so its absence never fails the check-up.
    lines.push(pdf_reader_line(sidecars_dir));

    DoctorReport { lines, all_ok }
}

#[cfg(feature = "pdf")]
fn pdf_reader_line(sidecars_dir: &Path) -> String {
    if runner::pdf::library_present(sidecars_dir) {
        "✓ PDF reading: ready".into()
    } else {
        format!(
            "– PDF reading: not set up — put {} in {} (CSV from your bank works best anyway)",
            runner::pdf::library_filename(),
            sidecars_dir.display()
        )
    }
}

#[cfg(not(feature = "pdf"))]
fn pdf_reader_line(_sidecars_dir: &Path) -> String {
    "– PDF reading: not included in this build (CSV from your bank works best anyway)".into()
}

/// First `.gguf` file in the models directory, if any.
fn first_gguf(models_dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(models_dir).ok()?;
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.ends_with(".gguf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A models directory holding one plausible-looking file. The mock
    /// sidecar never reads it; `run_checks` only needs a model to point
    /// `-m` at.
    fn models_dir_with_a_model(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kettle-doctor-{}-{}-models",
            std::process::id(),
            name
        ));
        std::fs::create_dir_all(&dir).expect("create models dir");
        std::fs::write(dir.join("tiny-test-model.gguf"), b"not a real model")
            .expect("write fake gguf");
        dir
    }

    #[test]
    fn healthy_binary_passes_spawn_and_health_checks() {
        let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../runner/tests/mock_bin/llama-server-mock.sh");
        let report = run_checks(
            &mock,
            &models_dir_with_a_model("healthy"),
            Path::new("/nowhere/sidecars"),
        );
        let text = report.lines.join("\n");

        assert!(text.contains("Starts up: yes"), "{text}");
        assert!(text.contains("Responds when asked: yes"), "{text}");
        assert!(report.all_ok, "{text}");
    }

    #[test]
    fn without_a_model_the_start_up_check_is_skipped_not_faked() {
        // llama-server with no `-m` starts in router mode: it listens,
        // answers /health, and can load nothing. Spawning it bare would
        // report "Responds when asked: yes" about a server that answers
        // every real question with a 400. Skipping says less and means
        // more — and a missing model is an expected state, not a
        // failure, so the check-up still passes.
        let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../runner/tests/mock_bin/llama-server-mock.sh");
        let report = run_checks(
            &mock,
            Path::new("/nowhere/models"),
            Path::new("/nowhere/sidecars"),
        );
        let text = report.lines.join("\n");

        assert!(text.contains("Starts up: skipped (no model"), "{text}");
        assert!(!text.contains("Responds when asked: yes"), "{text}");
        assert!(report.all_ok, "{text}");
    }

    #[test]
    fn gguf_in_models_dir_is_reported_by_name() {
        let models_dir =
            std::env::temp_dir().join(format!("kettle-doctor-test-models-{}", std::process::id()));
        std::fs::create_dir_all(&models_dir).expect("create models dir");
        std::fs::write(models_dir.join("tiny-test-model.gguf"), b"not a real model")
            .expect("write fake gguf");

        let report = run_checks(
            Path::new("/nowhere/llama-server"),
            &models_dir,
            Path::new("/nowhere/sidecars"),
        );
        let text = report.lines.join("\n");

        assert!(
            text.contains("Model file: found (tiny-test-model.gguf)"),
            "{text}"
        );
    }

    #[test]
    fn absent_pdf_reader_is_reported_but_not_a_failure() {
        let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../runner/tests/mock_bin/llama-server-mock.sh");
        let report = run_checks(
            &mock,
            &models_dir_with_a_model("pdf"),
            Path::new("/nowhere/sidecars"),
        );
        let text = report.lines.join("\n");

        // Whether the build includes PDF reading or not, doctor says so —
        // and either way an absent reader never fails the check-up.
        assert!(text.contains("– PDF reading: not"), "{text}");
        assert!(report.all_ok, "{text}");
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn vendored_pdf_library_reports_ready() {
        let sidecars = std::env::temp_dir().join(format!(
            "kettle-doctor-test-sidecars-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&sidecars).expect("create sidecars dir");
        std::fs::write(
            sidecars.join(runner::pdf::library_filename()),
            b"not a real lib",
        )
        .expect("write fake library");

        let report = run_checks(
            Path::new("/nowhere/llama-server"),
            Path::new("/nowhere/models"),
            &sidecars,
        );
        let text = report.lines.join("\n");

        assert!(text.contains("✓ PDF reading: ready"), "{text}");
    }

    #[test]
    fn missing_binary_is_reported_and_later_checks_skipped() {
        let report = run_checks(
            Path::new("/nowhere/llama-server"),
            Path::new("/nowhere/models"),
            Path::new("/nowhere/sidecars"),
        );
        let text = report.lines.join("\n");

        assert!(text.contains("not found"), "{text}");
        assert!(text.contains("skipped"), "{text}");
        assert!(text.contains("none yet"), "{text}");
        assert!(!report.all_ok);
    }
}
