//! The weights do not learn from your documents.
//!
//! "Is it training on my stuff?" is the question people bring to
//! anything called AI, and it is the one claim in the category most
//! often made loosely — an article this week had local models
//! "learning your habits", which is a privacy promise pointing the
//! wrong way: a thing that learns your habits has to keep them.
//!
//! Kettle's answer is mechanical rather than a matter of policy. The
//! model file is downloaded once, checked against a pinned sha256, and
//! from then on it is only ever read. Nothing in the pipeline writes to
//! it, and the process that reads it is `llama-server` — an inference
//! server, given a model to read and a port to answer on.
//!
//! Two scans, in the same deliberately dumb style as the network
//! boundary next door: markers on lines of shipped source. What they
//! can and cannot prove is the same shape as that crate's. A source
//! scan shows that no *call site* was added; it cannot show what a
//! dependency does on its own thread, and it is not a substitute for
//! reading `sidecar.rs`. It is here so that adding one is a deliberate
//! act somebody has to argue for, rather than a change nobody noticed.
//!
//! This is the owning test for the `weights-never-learn` claim.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Shipped Rust source: the crates and the Tauri shell, excluding test
/// targets, which are compiled out of a release and which write model
/// files constantly to stand their own fixtures up.
fn shipped_sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for dir in [
        root.join("crates/runner/src"),
        root.join("crates/cli/src"),
        root.join("app/src-tauri/src"),
    ] {
        collect(&dir, &mut found);
    }
    assert!(
        found.len() > 20,
        "the scan found almost nothing, which means it is looking in the wrong place"
    );
    found
}

fn collect(dir: &Path, into: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                into.push((path, before_the_test_module(&text)));
            }
        }
    }
}

/// Everything above `mod tests`. Inline test modules sit at the bottom
/// of a file by convention here, they are `#[cfg(test)]`, and they are
/// where fixtures write pretend weights to a scratch directory.
fn before_the_test_module(text: &str) -> String {
    match text.find("\nmod tests {") {
        Some(at) => text[..at].to_owned(),
        None => text.to_owned(),
    }
}

/// A line that is only a comment claims nothing about behaviour.
fn is_code(line: &str) -> bool {
    let trimmed = line.trim_start();
    !(trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*"))
}

/// The one file allowed to write a model file: the downloader, which is
/// how a model gets onto the machine in the first place.
const THE_DOWNLOADER: &str = "crates/runner/src/download.rs";

#[test]
fn nothing_but_the_downloader_ever_writes_a_model_file() {
    let root = repo_root();
    let writes = ["File::create", "fs::write", "OpenOptions", "create_new"];
    let weights = [".gguf", "models_dir", "weights"];

    let mut offenders = Vec::new();
    for (path, source) in shipped_sources(&root) {
        let relative = path.to_string_lossy().replace('\\', "/");
        if relative.ends_with(THE_DOWNLOADER) {
            continue;
        }
        for (number, line) in source.lines().enumerate() {
            if is_code(line)
                && writes.iter().any(|marker| line.contains(marker))
                && weights.iter().any(|marker| line.contains(marker))
            {
                offenders.push(format!("{relative}:{}: {}", number + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a model file is written outside the downloader, so \"the weights are only ever read\" \
         is no longer true as written:\n{}\n\nIf this is deliberate, the claim \
         `weights-never-learn` has to change before this test does.",
        offenders.join("\n"),
    );
}

#[test]
fn the_sidecar_is_asked_to_read_a_model_and_never_to_change_one() {
    let root = repo_root();
    // llama.cpp's own vocabulary for the things that would make a model
    // learn: fine-tuning, adapters, and anything that saves a model back
    // out. None of these has ever appeared in Kettle's source; the point
    // of naming them is that the day one does, it fails here first.
    let learning = [
        "--lora",
        "--train",
        "finetune",
        "fine-tune",
        "--save-model",
        "train-text-from-scratch",
    ];

    let mut offenders = Vec::new();
    for (path, source) in shipped_sources(&root) {
        for (number, line) in source.lines().enumerate() {
            if is_code(line) {
                for marker in learning {
                    if line.contains(marker) {
                        offenders.push(format!(
                            "{}:{}: {}",
                            path.to_string_lossy().replace('\\', "/"),
                            number + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "source asks the model tooling to learn something:\n{}",
        offenders.join("\n"),
    );

    // And the positive half: the spawn really does pass the weights as
    // the thing to read. A test that only listed forbidden words would
    // pass just as happily against a file that spawned nothing at all.
    let sidecar = std::fs::read_to_string(root.join("crates/runner/src/sidecar.rs"))
        .expect("the sidecar module is there");
    assert!(
        sidecar.contains("llama-server") || sidecar.contains("sidecar_binary"),
        "the sidecar module no longer names the server it starts"
    );
    assert!(
        sidecar.contains(".arg(\"-m\")"),
        "the sidecar is no longer given a model to read, so this test is measuring nothing"
    );
}
