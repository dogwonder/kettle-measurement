//! #334 §1 and §2: a role says how many files it takes, and what kind.
//!
//! #332 made `inputs` mean something — roles bind by name, and a run
//! that cannot say which document is which is refused. Two gaps were
//! left and named there rather than left as folklore:
//!
//! 1. `multiple: bool` says "one" or "any number". It cannot say
//!    exactly two, at least two, or at most twelve, so a pack has no
//!    way to stop somebody dropping four hundred files on it.
//! 2. `accept` is decorative. A pack declaring `["application/pdf"]`
//!    could be handed a `.txt` and find out mid-run, per file, from a
//!    step that knows nothing about which role wanted what.
//!
//! CONTRACT: these tests are the specification. If one looks wrong,
//! stop and say so — reporting a defect in the contract is a good
//! outcome and a better one than bending the test around it.

mod support;

use runner::document;
use runner::packs::{load_pack, Count, FileSemantics, PackError};
use runner::run::{run_pack_bound, Answers, InputBindingError, RunError};
use runner::run_dir::NoLog;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use support::MockModel;

/// A pack whose one role is declared however the test needs, with three
/// readable files beside it. Everything else is the smallest manifest
/// that loads.
fn pack_with_role(name: &str, role_json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-count-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write pack file");
    };
    write(
        "pack.json",
        &format!(
            r#"{{
          "id": "app.kttl.test-count",
          "name": "Count test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{role_json}],
          "capabilities": ["read"],
          "model": {{ "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 }},
          "copy": {{ "time": {{ "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }}, "will": [], "run_verb": "Run this task" }},
          "pipeline": [
            {{ "step": "preprocess", "impl": "builtin:document-text" }},
            {{ "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 }},
            {{ "step": "render", "template": "report.html.tera" }}
          ],
          "outputs": ["report.html"]
        }}"#
        ),
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do, and by when?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": {
                    "type": "object", "properties": {
                        "kind": { "enum": ["payment", "response", "appointment", "other"] },
                        "party": { "type": "string" },
                        "ask": { "type": "string" },
                        "deadline": { "type": "string" },
                        "anchor": { "type": "string" }
                    },
                    "required": ["kind", "party", "ask", "deadline", "anchor"]
                } }
            },
            "required": ["id", "confidence", "obligations"]
        } } }, "required": ["results"] }"#,
    );
    write(
        "report.html.tera",
        "<!doctype html><html><body></body></html>",
    );
    for file in ["one.txt", "two.txt", "three.txt", "four.txt"] {
        write(&format!("fixtures/{file}"), "1 August 2026\n\nA passage.\n");
    }
    write("fixtures/scan.pdf", "%PDF-1.4\n");
    write("fixtures/noextension", "1 August 2026\n\nA passage.\n");
    dir
}

const STATEMENTS: &str = r#"{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], "count": { "min": 2, "max": 3 } }"#;

fn bind(dir: &Path, files: &[&str]) -> Result<(), RunError> {
    let pack = load_pack(dir).expect("the pack loads");
    let bindings: Vec<(&str, PathBuf)> = files
        .iter()
        .map(|file| ("statement", dir.join("fixtures").join(file)))
        .collect();
    let mock = MockModel::respond_sequence(vec![("500 Internal Server Error", String::new())]);
    run_pack_bound(
        &pack,
        &bindings,
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .map(|_| ())
}

/// The binding error, or a panic naming what came back instead. A run
/// that got past binding is the failure this whole file is about, so it
/// must not be mistaken for a pass.
fn binding_error(result: Result<(), RunError>) -> InputBindingError {
    match result {
        Err(RunError::InputBinding(error)) => error,
        Err(other) => panic!("expected a binding refusal, got: {other}"),
        Ok(()) => panic!("expected a binding refusal, and the run went ahead"),
    }
}

// ── §1: how many ────────────────────────────────────────────────────

/// The manifest can say exactly two, at least two, and between two and
/// twelve — none of which `multiple: bool` could express.
#[test]
fn a_role_can_say_how_many_files_it_takes() {
    let cases: [(&str, Count); 3] = [
        (r#""count": 2"#, Count::Exactly(2)),
        (
            r#""count": { "min": 2 }"#,
            Count::Between {
                min: Some(2),
                max: None,
            },
        ),
        (
            r#""count": { "min": 2, "max": 12 }"#,
            Count::Between {
                min: Some(2),
                max: Some(12),
            },
        ),
    ];

    for (index, (declaration, expected)) in cases.iter().enumerate() {
        let dir = pack_with_role(
            &format!("declared-{index}"),
            &format!(
                r#"{{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], {declaration} }}"#
            ),
        );
        let pack = load_pack(&dir).expect("the pack loads");
        assert_eq!(pack.manifest.inputs[0].count, *expected, "{declaration}");
    }
}

/// Every pack in the repo predates `count`, and must keep meaning what
/// it meant: `multiple: false` and no declaration at all are both one
/// file, and `multiple: true` is one or more.
#[test]
fn multiple_still_means_what_it_always_meant() {
    let cases: [(&str, Count); 3] = [
        (r#", "multiple": false"#, Count::Exactly(1)),
        ("", Count::Exactly(1)),
        (
            r#", "multiple": true"#,
            Count::Between {
                min: Some(1),
                max: None,
            },
        ),
    ];

    for (index, (declaration, expected)) in cases.iter().enumerate() {
        let dir = pack_with_role(
            &format!("legacy-{index}"),
            &format!(
                r#"{{ "role": "statement", "label": "Your statements", "accept": ["text/plain"]{declaration} }}"#
            ),
        );
        let pack = load_pack(&dir).expect("the pack loads");
        assert_eq!(pack.manifest.inputs[0].count, *expected, "{declaration:?}");
    }
}

#[test]
fn several_files_are_separate_documents_unless_a_pack_calls_them_pages() {
    let ordinary = pack_with_role(
        "ordinary-files",
        r#"{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "count": { "min": 1 } }"#,
    );
    let pages = pack_with_role(
        "ordered-pages",
        r#"{ "role": "letter", "label": "Your letter", "accept": ["text/plain"], "count": { "min": 1 }, "file_semantics": "pages" }"#,
    );

    assert_eq!(
        load_pack(&ordinary).unwrap().manifest.inputs[0].file_semantics,
        FileSemantics::Documents
    );
    assert_eq!(
        load_pack(&pages).unwrap().manifest.inputs[0].file_semantics,
        FileSemantics::Pages
    );
}

#[test]
fn page_semantics_are_refused_for_a_non_document_preprocessor() {
    let dir = pack_with_role(
        "pages-on-statements",
        r#"{ "role": "statement", "label": "Your statement", "accept": ["text/plain"], "count": { "min": 1 }, "file_semantics": "pages" }"#,
    );
    let manifest = std::fs::read_to_string(dir.join("pack.json")).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace("builtin:document-text", "builtin:statement-parse"),
    )
    .unwrap();

    let error = load_pack(&dir).expect_err("statement files are not pages of one document");
    assert!(matches!(error, PackError::PagesNeedDocumentText { .. }));
}

/// Two fields saying the same thing can disagree, and then nothing
/// checks which one meant it.
#[test]
fn a_role_cannot_declare_both_multiple_and_count() {
    let dir = pack_with_role(
        "both",
        r#"{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], "multiple": true, "count": 2 }"#,
    );
    let error = load_pack(&dir).expect_err("a pack saying it twice is not loadable");
    assert!(
        matches!(error, PackError::Manifest(_)),
        "expected a manifest error, got: {error:?}"
    );
}

/// A count no set of files can satisfy is refused at load, not when
/// somebody has already chosen their documents.
#[test]
fn a_count_nothing_can_satisfy_is_refused_at_load() {
    for (name, declaration) in [
        ("inverted", r#""count": { "min": 3, "max": 2 }"#),
        ("none", r#""count": 0"#),
        ("none-range", r#""count": { "max": 0 }"#),
    ] {
        let dir = pack_with_role(
            name,
            &format!(
                r#"{{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], {declaration} }}"#
            ),
        );
        let error = match load_pack(&dir) {
            Err(error) => error,
            Ok(_) => panic!("{declaration} is a role nothing can fill, and it loaded"),
        };
        let PackError::UnsatisfiableCount { role, reason } = error else {
            panic!("expected UnsatisfiableCount for {declaration}, got: {error:?}");
        };
        assert_eq!(role, "statement");
        assert!(
            !reason.is_empty(),
            "the refusal tells a pack author what it read"
        );
    }
}

/// Too few and too many are different sentences, because they need
/// different things from a person: one more document, or one fewer.
#[test]
fn too_few_and_too_many_are_told_apart() {
    let dir = pack_with_role("range", STATEMENTS);

    let too_few = binding_error(bind(&dir, &["one.txt"]));
    let InputBindingError::TooFew { role, given, takes } = too_few else {
        panic!("expected TooFew, got: {too_few}");
    };
    assert_eq!((role.as_str(), given), ("statement", 1));
    assert_eq!(takes, "between two and three files");

    let too_many = binding_error(bind(&dir, &["one.txt", "two.txt", "three.txt", "four.txt"]));
    let InputBindingError::TooMany { role, given, takes } = too_many else {
        panic!("expected TooMany, got: {too_many}");
    };
    assert_eq!((role.as_str(), given), ("statement", 4));
    assert_eq!(takes, "between two and three files");
}

/// Both ends of the range are inclusive, and the counts between them
/// are accepted. Asserted through binding rather than through `permits`
/// alone: an off-by-one here refuses a person's documents.
#[test]
fn a_count_inside_the_range_binds() {
    let dir = pack_with_role("inside", STATEMENTS);
    for files in [
        vec!["one.txt", "two.txt"],
        vec!["one.txt", "two.txt", "three.txt"],
    ] {
        // Anything other than a binding refusal means binding passed
        // and the run failed later, which is what this test wants: the
        // mock model refuses every call.
        if let Err(RunError::InputBinding(error)) = bind(&dir, &files) {
            panic!("{} files refused: {error}", files.len());
        }
    }
}

/// The words a refusal uses, since they are what a person reads. Plain
/// British English, and singular where the number is one — "1 files" is
/// the shell talking to itself.
#[test]
fn a_declaration_reads_as_a_sentence() {
    assert_eq!(Count::Exactly(1).in_words(), "one file");
    assert_eq!(Count::Exactly(2).in_words(), "two files");
    assert_eq!(
        Count::Between {
            min: Some(2),
            max: None
        }
        .in_words(),
        "at least two files"
    );
    assert_eq!(
        Count::Between {
            min: None,
            max: Some(12)
        }
        .in_words(),
        "up to twelve files"
    );
    assert_eq!(
        Count::Between {
            min: Some(2),
            max: Some(12)
        }
        .in_words(),
        "between two and twelve files"
    );
}

// ── §2: what kind ───────────────────────────────────────────────────

/// `media_type` and `read_document` must agree, or a file is accepted
/// at binding and refused mid-run — the failure this check exists to
/// move earlier, reintroduced one layer down.
#[test]
fn a_files_type_is_read_from_its_name() {
    let cases: [(&str, Option<&str>); 7] = [
        ("statement.csv", Some("text/csv")),
        ("letter.txt", Some("text/plain")),
        ("notes.md", Some("text/markdown")),
        ("notes.markdown", Some("text/markdown")),
        ("policy.pdf", Some("application/pdf")),
        ("policy.PDF", Some("application/pdf")),
        ("contract.docx", None),
    ];
    for (name, expected) in cases {
        assert_eq!(document::media_type(Path::new(name)), expected, "{name}");
    }
}

/// A pack that says it reads PDFs is not handed a text file.
#[test]
fn a_type_the_role_does_not_accept_is_refused() {
    let dir = pack_with_role(
        "wrong-type",
        r#"{ "role": "statement", "label": "Your statements", "accept": ["application/pdf"], "count": 1 }"#,
    );

    let error = binding_error(bind(&dir, &["one.txt"]));
    let InputBindingError::WrongType {
        role,
        file,
        accepted,
    } = error
    else {
        panic!("expected WrongType, got: {error}");
    };
    assert_eq!(role, "statement");
    assert_eq!(file, "one.txt", "the file by name, never its full path");
    assert_eq!(accepted, vec!["application/pdf".to_owned()]);
}

/// A file whose type cannot be told is refused rather than assumed:
/// "I could not tell" is not evidence that it was a PDF.
#[test]
fn a_file_whose_type_cannot_be_told_is_refused() {
    let dir = pack_with_role(
        "unknown-type",
        r#"{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], "count": 1 }"#,
    );

    let error = binding_error(bind(&dir, &["noextension"]));
    assert!(
        matches!(error, InputBindingError::WrongType { .. }),
        "expected WrongType, got: {error}"
    );
}

/// The type a role accepts binds, and nothing about this check makes a
/// legitimate file harder to give.
#[test]
fn an_accepted_type_binds() {
    let dir = pack_with_role(
        "right-type",
        r#"{ "role": "statement", "label": "Your statements", "accept": ["text/plain"], "count": 1 }"#,
    );
    if let Err(RunError::InputBinding(error)) = bind(&dir, &["one.txt"]) {
        panic!("a text file refused by a role accepting text: {error}");
    }
}
