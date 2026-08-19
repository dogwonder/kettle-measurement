//! Nothing committed to this tree quotes a document that is not itself
//! committed and synthetic.
//!
//! The data rules say somebody's real records must never enter the
//! repository, and `.gitignore` enforces that for the *original*: a real
//! document is named `*.private.<ext>` and stays out. Nothing enforced
//! it for what a run makes **from** one. A run directory copied into
//! `fixtures/` carries `results.json`, the rendered report and `raw/` —
//! the document's own sentences, verbatim, twice over, since a recorded
//! `decision_key` is only lower-cased and not hashed. None of those
//! files matches an ignore rule, and `fixtures/` is inside the published
//! boundary (`assurance/claims.json`), so the copy would leave for a
//! public repository.
//!
//! That is not hypothetical. `fixtures/letter-01/` was made exactly that
//! way on 18 August, by copying a run out of the app's own directory.
//! It was safe because the run read a bed letter. The neighbouring run
//! in the same directory that hour read a real bank statement, and the
//! three commands would have been identical.
//!
//! So the check is on **content, not names**. A name check passes a real
//! statement filed tidily; asking where each sentence came from does not.
//! Every passage a committed artefact quotes must appear verbatim in a
//! committed fixture — and a fixture is synthetic by the data rules,
//! generated from a spec `kettle bed` can restore.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whitespace is not evidence of anything. The preprocessor flattens a
/// two-column invoice table into one line per row, so a passage can be
/// a true quotation of a fixture whose bytes it does not match.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every committed fixture's text, as one haystack: the pack beds and
/// the statement fixtures that predate them. Synthetic by the data
/// rules, and regenerable from a committed spec.
fn committed_corpus(root: &Path) -> (String, BTreeSet<String>) {
    let mut corpus = String::new();
    let mut names = BTreeSet::new();
    let packs = root.join("packs");
    for pack in std::fs::read_dir(&packs)
        .expect("packs/")
        .filter_map(Result::ok)
    {
        let fixtures = pack.path().join("fixtures");
        let Ok(entries) = std::fs::read_dir(&fixtures) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_document = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "txt" | "csv" | "md"));
            if !is_document {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                corpus.push_str(&normalise(&text));
                corpus.push('\n');
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                names.insert(name.to_owned());
            }
        }
    }
    assert!(
        names.len() > 100,
        "the corpus is nearly empty, so this test would pass anything"
    );
    (corpus, names)
}

/// Fields that carry a document's own words rather than anybody's
/// composition.
///
/// Deliberately not "every string", and the first draft of this list
/// was too greedy: it caught `export.text`, which is Kettle writing a
/// calendar entry ("Pay £480.00 towards the estate matter (Trentham &
/// Yale Solicitors) — within 14 days"), and an obligation's `ask`,
/// which is the model rephrasing. Neither is a quotation and neither
/// appears in the letter. A guard that fails on correct behaviour gets
/// switched off, so it has to name what is a quotation **by contract**:
/// a segment is a passage as read, `raw_input` is the input itself, and
/// evidence must be verbatim (#452, #460).
/// Leaves that are the document's own words, wherever they appear.
///
/// Two drafts were wrong before this one, and both were wrong the same
/// way — too greedy, failing on correct behaviour. `export.text` is
/// Kettle writing a calendar entry; an obligation's `ask` is the model
/// rephrasing; `evidence.reason` is Rust explaining its own arithmetic
/// ("12 payments, one every month on or near the 3rd"). None of those
/// is a quotation and none appears in the source. A guard that cries
/// wolf gets switched off, so this names only what the pipeline
/// promises is verbatim: passages as read, transaction descriptors
/// untidied, and the quoted spans evidence is required to carry
/// (#452, #460).
const QUOTED_FIELDS: [&str; 6] = [
    "segment",
    "raw_input",
    "raw",
    "description",
    "raw_merchant",
    "in_the_letter",
];

/// `text` is the one that depends on where it is: inside evidence it is
/// the passage a claim rests on and must be verbatim, while inside an
/// action's `export` it is Kettle composing a calendar entry. Same key,
/// opposite contract — which is why this walk carries context at all.
const EVIDENCE: &str = "evidence";
const CONTEXTUAL: &str = "text";

/// Evidence in an action names its passages `passage_1`, `passage_2`.
fn is_passage(key: &str) -> bool {
    key.starts_with("passage_")
}

/// Fields naming the document a run read.
const INPUT_FIELDS: [&str; 2] = ["file", "input_files"];

fn walk_json(node: &Value, quoted: &mut Vec<String>, inputs: &mut Vec<String>) {
    walk(node, false, quoted, inputs);
}

fn walk(node: &Value, in_evidence: bool, quoted: &mut Vec<String>, inputs: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            for (key, value) in map {
                let inside = in_evidence || key == EVIDENCE;
                match value {
                    Value::String(text)
                        if QUOTED_FIELDS.contains(&key.as_str())
                            || is_passage(key)
                            || (inside && key == CONTEXTUAL) =>
                    {
                        quoted.push(text.clone());
                    }
                    Value::String(name) if INPUT_FIELDS.contains(&key.as_str()) => {
                        inputs.push(name.clone());
                    }
                    Value::Array(items) if INPUT_FIELDS.contains(&key.as_str()) => {
                        for item in items {
                            if let Value::String(name) = item {
                                inputs.push(name.clone());
                            }
                        }
                    }
                    other => walk(other, inside, quoted, inputs),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, in_evidence, quoted, inputs);
            }
        }
        _ => {}
    }
}

/// Every JSON file committed under `fixtures/`, which is where a run's
/// outputs land when somebody keeps one as an example.
fn committed_artefacts(root: &Path) -> Vec<(PathBuf, Value)> {
    let mut found = Vec::new();
    collect(&root.join("fixtures"), &mut found);
    assert!(
        !found.is_empty(),
        "no committed artefacts found, so this test is measuring nothing"
    );
    found
}

fn collect(dir: &Path, into: &mut Vec<(PathBuf, Value)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|e| e == "json") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    into.push((path, value));
                }
            }
        }
    }
}

#[test]
fn a_committed_artefact_only_quotes_committed_fixtures() {
    let root = repo_root();
    let (corpus, _) = committed_corpus(&root);

    let mut strays = Vec::new();
    for (path, value) in committed_artefacts(&root) {
        let (mut quoted, mut inputs) = (Vec::new(), Vec::new());
        walk_json(&value, &mut quoted, &mut inputs);
        for passage in quoted {
            let normalised = normalise(&passage);
            // A word or two carries no disclosure and matches
            // everything; the point of the check is whole sentences.
            if normalised.len() < 25 {
                continue;
            }
            if !corpus.contains(&normalised) {
                strays.push(format!(
                    "{}: {}",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    &normalised[..normalised.len().min(90)]
                ));
            }
        }
        let _ = inputs;
    }

    assert!(
        strays.is_empty(),
        "a committed artefact quotes text that is in no committed fixture, so it was made from \
         a document this repository does not have — which is how somebody's real records reach \
         a public tree:\n{}",
        strays.join("\n"),
    );
}

#[test]
fn a_committed_artefact_only_names_committed_fixtures() {
    let root = repo_root();
    let (_, names) = committed_corpus(&root);

    let mut strays = Vec::new();
    for (path, value) in committed_artefacts(&root) {
        let (mut quoted, mut inputs) = (Vec::new(), Vec::new());
        walk_json(&value, &mut quoted, &mut inputs);
        for name in inputs {
            // Only file names, not the many other things called `file`
            // in these documents — a model's file name, a path in a
            // policy block.
            let looks_like_a_document = [".txt", ".csv", ".md", ".pdf", ".jpg", ".png"]
                .iter()
                .any(|extension| name.ends_with(extension));
            if looks_like_a_document && !names.contains(&name) {
                strays.push(format!(
                    "{}: {name}",
                    path.strip_prefix(&root).unwrap_or(&path).display()
                ));
            }
        }
        let _ = quoted;
    }

    assert!(
        strays.is_empty(),
        "a committed artefact names a document that is not a committed fixture:\n{}\n\nA real \
         document's name is a disclosure on its own — the run records that were deleted from \
         this machine held one naming a bank and a date range.",
        strays.join("\n"),
    );
}
