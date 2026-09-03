//! A test that cannot run must fail, and the ones that don't are counted
//! (#603).
//!
//! `CLAUDE.md` has said since PR #99 that a test needing something
//! gitignored must **fail** without it, never skip quietly into green: a
//! vacuously-passing test once hid a real failure through ten
//! consecutive CI runs. `.claude/hooks/no-quietened-tests.sh` made that
//! a control rather than a convention — but it catches *markers*, the
//! `ignore` attribute in Rust and a skipped or exclusive case in a
//! Vitest file, and a quiet skip written as a runtime `return` carries
//! no marker at all. So the rule the hook enforces has a hole exactly
//! the shape of the tests that most need it. The 2 September review put
//! the count at eight; it was twelve.
//!
//! This is not a demand that every such skip disappear. libpdfium and
//! llama-server are vendored and never committed, so a CI runner that
//! downloads neither genuinely cannot run those bodies, and failing them
//! there would make main permanently red for a reason nobody can act on.
//! What is refused is a skip nobody wrote down. The idiom is the one
//! #466 settled for tolerated mutation survivors and
//! `STAGED_GOVUK_COMPONENTS` for staged components: an exception is
//! declared machine-readably, beside the harness that reads it, with
//! what it needs and why it cannot be committed — never remembered from
//! a PR body.
//!
//! Three properties, and the third is the one that makes the other two
//! more than a list:
//!
//! - a **new** quiet skip fails, which is what the hook could not give;
//! - a declaration whose site is gone fails, so an exception cannot
//!   outlive its reason;
//! - a declaration must name the artefact the skip's own message names,
//!   so a row cannot be pasted onto the wrong site and still read true.
//!
//! Scope is `crates/*/tests/*.rs`, and stops there on purpose:
//! `app/src-tauri` is a separate workspace and outside the published
//! boundary (#478), so a test here that read it would break the
//! projected public tree the moment it ran there. That workspace has one
//! quiet skip of its own (`core.rs`, libpdfium) and **nothing yet
//! guards it** — the honest statement of the gap rather than a claim
//! that it is covered elsewhere.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The vendored artefacts a test body may legitimately need. A closed
/// set, like the pack execution contract's roles (#120): a declaration
/// naming anything else is refused, because "it needs a thing" is the
/// excuse this test exists to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Needs {
    /// `sidecars/libpdfium.*`, from pdfium-binaries.
    Libpdfium,
    /// `sidecars/<platform>/llama-server`, from
    /// `scripts/vendor-sidecar.sh`.
    LlamaServer,
}

impl Needs {
    /// Which artefact a skip's own message says it wants.
    ///
    /// Read out of the message rather than merely looked for in it: the
    /// first version of this asked whether the message *contained* the
    /// declared artefact's word, with `"sidecar"` standing for
    /// llama-server — and every pdfium message contains it too, inside
    /// `sidecars/`. A pdfium site declared as needing llama-server
    /// passed. A test that only ever asks "is my word in there" cannot
    /// tell two artefacts apart when one's name is a substring of the
    /// other's path, which is the whole job.
    fn from_message(message: &str) -> Option<Self> {
        // Ordered: the more specific name is tried first, so a message
        // naming both cannot be read as the vaguer one.
        if message.contains("libpdfium") {
            Some(Needs::Libpdfium)
        } else if message.contains("vendored sidecar") {
            Some(Needs::LlamaServer)
        } else {
            None
        }
    }
}

/// One tolerated quiet skip, and why it cannot simply be made to fail.
struct Declared {
    file: &'static str,
    test: &'static str,
    needs: Needs,
    /// Why the thing it needs is not committed, so the skip cannot be
    /// removed by vendoring it into the repository.
    why: &'static str,
}

/// Every quiet skip this repository tolerates. Adding a row is a
/// deliberate act with a reason; nothing else may skip.
///
/// All twelve are the same two artefacts. libpdfium is a per-platform
/// binary from pdfium-binaries and llama-server is a ~24MB dylib closure
/// built by `scripts/vendor-sidecar.sh`; committing either would put a
/// vendored binary in a repository that is published as a public tree
/// (#478), and downloading them on every push would spend the rationed
/// Actions allowance. The route that makes these bodies run is a runner
/// that has them, not a smaller skip.
const DECLARED: &[Declared] = &[
    Declared {
        file: "runner/tests/eval_pdf_fixture.rs",
        test: "a_pdf_fixture_is_scored_through_pdfium_when_the_reader_is_named",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary (#256)",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "statement_04_lines_alone_cannot_tell_money_out_from_money_in",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "statement_04_pdf_reconstructs_to_the_same_transactions_as_its_csv",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "statement_05_reads_its_direction_out_of_the_running_balance",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "synthetic_statement_pdf_extracts_reading_order_lines",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "the_same_layout_without_a_balance_is_still_refused",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "two_statements_can_be_read_in_one_process",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/pdf.rs",
        test: "two_threads_reading_at_once_do_not_corrupt_anything",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/run.rs",
        test: "text_layer_pdf_reaches_recurring_findings_and_report_totals",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/run.rs",
        test: "text_layer_pdf_reaches_the_ordinary_pipeline",
        needs: Needs::Libpdfium,
        why: "the PDF reader is a vendored per-platform binary",
    },
    Declared {
        file: "runner/tests/sidecar.rs",
        test: "the_vendored_sidecar_finds_its_dylibs_beside_itself",
        needs: Needs::LlamaServer,
        why: "llama-server is a ~24MB dylib closure built per platform (#50)",
    },
    Declared {
        file: "runner/tests/sidecar.rs",
        test: "the_vendored_sidecar_loads_nothing_from_outside_its_own_directory",
        needs: Needs::LlamaServer,
        why: "llama-server is a ~24MB dylib closure built per platform (#50)",
    },
];

/// A quiet skip as the source shows it: a test body that prints
/// "skipping" and returns instead of asserting anything.
#[derive(Debug, Clone)]
struct Found {
    file: String,
    test: String,
    /// The message the body prints, which is all a person running the
    /// suite ever sees of it.
    message: String,
}

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every `crates/*/tests/*.rs`, sorted, so a failure reads the same on
/// every machine.
fn test_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for krate in std::fs::read_dir(crates_dir()).expect("crates/") {
        let tests = krate.expect("a crate directory").path().join("tests");
        let Ok(entries) = std::fs::read_dir(&tests) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("a test file").path();
            if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Scan one file for bodies that print "skipping" and return.
///
/// Deliberately textual, like the hook it extends: the property is about
/// what a reader of the source sees, and a scanner that needed the crate
/// to compile could not run before the code it guards.
fn quiet_skips_in(path: &Path) -> Vec<Found> {
    let source = std::fs::read_to_string(path).expect("a test file reads");
    let file = path
        .strip_prefix(crates_dir())
        .expect("under crates/")
        .to_string_lossy()
        .replace('\\', "/");

    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        if !line.contains("eprintln!(\"skipping") {
            continue;
        }
        // A print alone is not a skip; the body has to give up. The
        // `return` follows within a couple of lines in every shape we
        // have, allowing for a wrapped argument list.
        let gives_up = lines[n + 1..lines.len().min(n + 4)]
            .iter()
            .any(|l| l.trim_start().starts_with("return"));
        if !gives_up {
            continue;
        }
        // Walk back to the enclosing `fn`, which is the name a person
        // needs in order to find it.
        let test = lines[..n]
            .iter()
            .rev()
            .find_map(|l| {
                let trimmed = l.trim_start();
                trimmed
                    .strip_prefix("fn ")
                    .or_else(|| trimmed.strip_prefix("pub fn "))
                    .and_then(|rest| rest.split(['(', '<']).next())
            })
            .expect("a quiet skip sits inside a function")
            .to_owned();
        found.push(Found {
            file: file.clone(),
            test,
            message: line.trim().to_owned(),
        });
    }
    found
}

fn scanned() -> Vec<Found> {
    test_files()
        .iter()
        .flat_map(|p| quiet_skips_in(p))
        .collect()
}

fn sites(found: &[Found]) -> BTreeSet<(String, String)> {
    found
        .iter()
        .map(|f| (f.file.clone(), f.test.clone()))
        .collect()
}

fn declared_sites() -> BTreeSet<(String, String)> {
    DECLARED
        .iter()
        .map(|d| (d.file.to_owned(), d.test.to_owned()))
        .collect()
}

fn describe(site: &(String, String)) -> String {
    format!("{} — {}", site.0, site.1)
}

#[test]
fn every_quiet_skip_is_declared() {
    let undeclared: Vec<_> = sites(&scanned())
        .difference(&declared_sites())
        .map(describe)
        .collect();

    assert!(
        undeclared.is_empty(),
        "a test that cannot run must fail, not return quietly. \
         These skip with nothing written down:\n  {}\n\
         Either make the body fail with a sentence saying what to vendor, \
         or add it to DECLARED in {} with what it needs and why that \
         cannot be committed.",
        undeclared.join("\n  "),
        file!(),
    );
}

#[test]
fn a_declaration_names_a_skip_that_is_still_there() {
    let stale: Vec<_> = declared_sites()
        .difference(&sites(&scanned()))
        .map(describe)
        .collect();

    assert!(
        stale.is_empty(),
        "DECLARED still tolerates skips that are gone:\n  {}\n\
         Remove the rows — an exception that outlives its reason stops \
         describing the tree.",
        stale.join("\n  "),
    );
}

#[test]
fn a_declaration_agrees_with_the_message_beside_it() {
    let found: BTreeMap<_, _> = scanned()
        .into_iter()
        .map(|f| ((f.file.clone(), f.test.clone()), f.message))
        .collect();

    let mut wrong = Vec::new();
    for row in DECLARED {
        assert!(
            !row.why.trim().is_empty(),
            "{} — {} is tolerated for no stated reason",
            row.file,
            row.test,
        );
        let site = (row.file.to_owned(), row.test.to_owned());
        let Some(message) = found.get(&site) else {
            continue; // the stale-declaration test owns this failure
        };
        match Needs::from_message(message) {
            Some(said) if said == row.needs => {}
            Some(said) => wrong.push(format!(
                "{} — {}\n      declared as needing {:?}, but says {said:?}: {message}",
                row.file, row.test, row.needs
            )),
            None => wrong.push(format!(
                "{} — {}\n      names no artefact this test knows: {message}\n      \
                 Say which of {:?} it needs, or add the artefact to Needs.",
                row.file,
                row.test,
                [Needs::Libpdfium, Needs::LlamaServer],
            )),
        }
    }

    assert!(
        wrong.is_empty(),
        "a declaration must name what the skip's own message names, \
         or the row was pasted onto the wrong site:\n  {}",
        wrong.join("\n  "),
    );
}
