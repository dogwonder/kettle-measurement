//! Rust verifies; it never discovers (CLAUDE.md, 4 September 2026).
//!
//! Every list of words the resolver uses to *find* something on a page
//! is a phrasebook: written from one real letter, extended one letter
//! at a time, and invisible to a bed authored from the same list. The
//! replacement is a closed question answered as a passage id, with
//! Rust checking the chosen passage. Until each list is retired it is
//! staged here with a date and the question that retires it, so the
//! debt is finite and visible. A new list fails this test unless it is
//! staged, which is the point: adding one has to be a decision.

use std::path::Path;

/// (constant name, staged on, the closed question that retires it — or
/// why it is a verifier and stays).
const STAGED_PHRASEBOOKS: &[(&str, &str, &str)] = &[(
    "DATELINE_WORDS",
    "2026-09-05",
    "serves only the OCR date dispute (`date_dispute`, document.rs): two readings of a \
     photographed letter compared by the dateline each shows. Resolution no longer reads \
     it — the base a period counts from is the model's `from` reading (review of #626, \
     Task 5). Goes when the dispute is re-stated as the model's dateline reading checked \
     against the second OCR pass",
)];

#[test]
fn every_word_list_the_resolver_searches_with_is_staged_for_retirement() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/timeline.rs"))
            .expect("timeline.rs is readable");
    let mut unstaged: Vec<String> = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("const ") else {
            continue;
        };
        let Some((name, ty)) = rest.split_once(':') else {
            continue;
        };
        if !ty.trim_start().starts_with("[&str") {
            continue;
        }
        let name = name.trim();
        if !STAGED_PHRASEBOOKS
            .iter()
            .any(|(staged, ..)| *staged == name)
        {
            unstaged.push(format!("  timeline.rs:{}: `{name}`", number + 1));
        }
    }
    assert!(
        unstaged.is_empty(),
        "{} word list(s) the resolver searches the page with are not staged for retirement. \
         A finder list is a phrasebook written from one letter; the replacement is a closed \
         question answered as a passage id (CLAUDE.md, 4 September 2026). Stage it in \
         STAGED_PHRASEBOOKS with a date and the question that retires it, or write the \
         question instead:\n{}",
        unstaged.len(),
        unstaged.join("\n")
    );

    // And a stage that names nothing any more is stale.
    for (name, date, _) in STAGED_PHRASEBOOKS {
        assert!(
            source.contains(&format!("const {name}:")),
            "`{name}` (staged {date}) is no longer in timeline.rs — remove its stage"
        );
    }
}
