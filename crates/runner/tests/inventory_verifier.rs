//! The inventory's verifier column, executed: every case that carries a
//! `verifier.check` is run through `reading::check` on main and must come
//! out as the inventory says — supported, not a sum, absent, or the
//! documented misread. No model, no fixtures. A verifier change that
//! widens or narrows what a sum or a name is fails here against the
//! inventory rather than drifting under it.
use runner::document::Segment;
use runner::reading::{check, Checked, Kind, Reading};
use std::collections::BTreeSet;
use std::path::Path;

fn cases_with_checks() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/capabilities");
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("evals/capabilities")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with("-forms.json"))
        .collect();
    files.sort();
    for path in files {
        let inventory: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for case in inventory["cases"].as_array().unwrap() {
            if case["verifier"]["check"].is_object() {
                out.push((case["id"].as_str().unwrap().to_owned(), case.clone()));
            }
        }
    }
    out
}

fn segments(document: &str) -> Vec<Segment> {
    document
        .split('\n')
        .enumerate()
        .map(|(ordinal, text)| Segment {
            document: 0,
            page: 1,
            ordinal,
            text: text.to_owned(),
            rows: Vec::new(),
        })
        .collect()
}

#[test]
fn every_checked_case_comes_out_as_the_inventory_says() {
    let cases = cases_with_checks();
    assert!(
        cases.len() >= 20,
        "the money and party families carry checks"
    );
    for (id, case) in cases {
        let spec = &case["verifier"]["check"];
        let kind = match spec["kind"].as_str().unwrap() {
            "Money" => Kind::Money,
            "Name" => Kind::Name,
            other => panic!("{id}: unknown check kind {other}"),
        };
        let value = spec["value"].as_str().unwrap();
        let phrase = case["phrase"].as_str().unwrap();
        let segments = segments(case["model_reading"]["document"].as_str().unwrap());
        let own = segments
            .iter()
            .find(|s| s.text.contains(phrase))
            .unwrap_or_else(|| panic!("{id}: the phrase is in one passage"))
            .clone();
        // The value is read where it is printed: the first passage that
        // prints it, which for a sign-off name is not the ask's own.
        let at = segments
            .iter()
            .find(|s| !value.is_empty() && s.text.contains(value))
            .map_or(own.ordinal, |s| s.ordinal);
        let shown: BTreeSet<usize> = (0..segments.len()).collect();
        let reading = if value.is_empty() {
            Reading::absent(own.ordinal)
        } else {
            Reading::new(at, value)
        };
        let checked = check(&reading, kind, &own, &shown, &segments);
        let expected = spec["expected"].as_str().unwrap();
        let ok = match expected {
            "supported" => matches!(&checked, Checked::Supported { parses: true, .. }),
            "supported-with-warning" => {
                matches!(&checked, Checked::Supported { parses: true, warnings, .. } if !warnings.is_empty())
            }
            // On the page but not a sum: kept as words, nothing derived.
            "not-a-sum" => !matches!(&checked, Checked::Supported { parses: true, .. }),
            "absent" => matches!(&checked, Checked::Absent),
            // The documented gap: the verifier passes a value the truth
            // says is something else. Recorded, so a fix here shows up as
            // this case flipping rather than as silence.
            "accepted-misread" => matches!(&checked, Checked::Supported { parses: true, .. }),
            other => panic!("{id}: unknown expectation {other}"),
        };
        assert!(ok, "{id}: expected {expected}, verifier said {checked:?}");
    }
}
