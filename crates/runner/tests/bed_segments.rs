//! Every expectation names a passage the reader actually produces
//! (#504).
//!
//! The bed joins an expectation to a reading by the passage it came out
//! of — `same_passage`, whitespace-normalised exact equality — so an
//! expectation naming a string no segmenter ever emits cannot be met by
//! any model. It scores zero by *construction* rather than by
//! measurement, and reads on the table as a model that failed.
//!
//! That is a live risk rather than a theoretical one. `invoice_totals`
//! is the first shape in this bed that is not prose: its passage is
//! written row-wise, because that is how the print rows run, and a
//! correct reading takes each column in turn. The expectation is
//! authored to the second — deliberately, since deriving it from the
//! segmenter would make the bed agree with the run on exactly the thing
//! under test — and this is what holds the two authored halves
//! together.
//!
//! Reads committed fixtures only: no `models/`, no `sidecars/`, so it
//! means the same thing locally and in CI.

use runner::document::segments_from_text;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixtures(pack: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(pack)
        .join("fixtures")
}

/// Every `(fixture stem, expected segment)` a pack's committed
/// expectations name.
fn expected_segments(pack: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(fixtures(pack)).expect("the pack's fixtures") {
        let path = entry.expect("a fixture").path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let Some(stem) = name.strip_suffix(".expected.json") else {
            continue;
        };
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("readable"))
                .expect("committed expectations are valid JSON");
        let Some(items) = json["obligations"].as_array() else {
            continue;
        };
        for item in items {
            if let Some(segment) = item["segment"].as_str() {
                out.push((stem.to_owned(), segment.to_owned()));
            }
        }
    }
    out
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_letter_expectation_names_a_passage_the_reader_produces() {
    let pack = "app.kttl.letter-to-actions";
    let wanted = expected_segments(pack);
    assert!(
        wanted.len() > 700,
        "the whole bed was read, not a corner of it: {} expectations",
        wanted.len()
    );

    let mut unreachable = Vec::new();
    let mut checked_tabular = 0usize;
    for (stem, segment) in &wanted {
        let text = std::fs::read_to_string(fixtures(pack).join(format!("{stem}.txt")))
            .expect("every expectation has its letter");
        let produced: BTreeSet<String> = segments_from_text(&text)
            .iter()
            .map(|s| words(&s.text))
            .collect();
        if stem.contains("invoice_totals") {
            checked_tabular += 1;
        }
        if !produced.contains(&words(segment)) {
            unreachable.push(format!(
                "{stem}\n    wants: {segment:?}\n    reads: {produced:#?}"
            ));
        }
    }

    assert!(
        checked_tabular > 0,
        "the tabular shape is in the bed — this test's whole reason for existing"
    );
    assert!(
        unreachable.is_empty(),
        "{} of {} expectations name a passage no reader emits:\n{}",
        unreachable.len(),
        wanted.len(),
        unreachable.join("\n")
    );
}

#[test]
fn the_tabular_shape_reads_its_due_date_away_from_the_money() {
    // The property the shape exists to measure, asserted on the
    // committed fixtures rather than on a page written to pass. Read
    // row-wise — which is what the file states, and what every reader
    // did before #406 — the due date lands between the sub total and
    // the VAT, and the figures appear to attach to the wrong labels.
    let pack = "app.kttl.letter-to-actions";
    let mut checked = 0usize;

    for entry in std::fs::read_dir(fixtures(pack)).expect("the pack's fixtures") {
        let path = entry.expect("a fixture").path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !name.contains("invoice_totals") || !name.ends_with(".txt") {
            continue;
        }
        checked += 1;

        let text = std::fs::read_to_string(&path).expect("readable");
        let read = segments_from_text(&text)
            .iter()
            .map(|s| words(&s.text))
            .collect::<Vec<_>>()
            .join("\n");

        let due = read.find("Due date").expect("the due date label is read");
        let sub = read.find("Sub total").expect("the sub total is read");
        assert!(
            !(sub < due),
            "{name}: the totals column swallowed the due date:\n{read}"
        );

        // And the totals keep their own values, which is the half a
        // reordering alone would not fix.
        for label in ["Sub total", "VAT", "Total"] {
            let line = read
                .lines()
                .find(|line| line.contains(label))
                .unwrap_or_else(|| panic!("{name}: {label} is read"));
            assert!(
                line.contains('£'),
                "{name}: {label} keeps a figure beside it: {line:?}"
            );
        }
    }

    // Derived from the committed spec, never a literal: this read 24
    // until #552 appended twelve development families, and a hard-coded
    // count turns a bed that grew into a test that failed. What the
    // assertion is for is that the loop saw *every* invoice letter --
    // a filter that silently matched none would otherwise pass every
    // property above vacuously.
    let spec =
        runner::eval::letters::committed_spec(fixtures(pack).parent().expect("the pack directory"))
            .expect("the committed spec");
    let expected: usize = [&spec.sets.development, &spec.sets.exam]
        .iter()
        .map(|set| set.shapes.get("invoice_totals").map_or(0, Vec::len))
        .sum();
    assert_eq!(checked, expected, "every invoice letter in both voices");
}
