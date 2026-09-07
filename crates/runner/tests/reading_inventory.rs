//! The capability inventory's contract, independent of whichever verifier
//! is live (`evals/capabilities/`, #595).
//!
//! Every `*-forms.json` file under `evals/capabilities` is one capability
//! family. A case names a source form, what the document states (or that
//! it is absent, ambiguous or unsupported), where the distinction came
//! from, which packs consume it, how far the deterministic verifier
//! reaches, and a synthetic document a model could be asked to read.
//! Those are kept apart: nothing here turns "a case exists" into "a
//! capability passes".
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn inventories() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/capabilities");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("evals/capabilities is readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-forms.json"))
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no inventory files found in {}",
        dir.display()
    );
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("inventory is readable");
            let value: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                value,
            )
        })
        .collect()
}

/// The vocabularies the coverage fields may use. Kept in step with
/// `scripts/capability-coverage.py`, which reports on the same files.
const VERIFIER_COVERAGE: [&str; 5] = [
    "main-check",
    "not-exercised",
    "model-judgement",
    "unsupported",
    "none",
];
const MODEL_COVERAGE: [&str; 2] = ["not-measured", "measured"];

#[test]
fn inherited_forms_keep_truth_provenance_and_separate_coverage() {
    let (_, dates) = inventories()
        .into_iter()
        .find(|(name, _)| name == "date-forms.json")
        .expect("the inherited date inventory is present");
    let cases = dates["cases"].as_array().unwrap();
    let ids: BTreeSet<_> = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    for n in 1..=33 {
        assert!(
            ids.contains(format!("date-form-{n:03}").as_str()),
            "lost inherited form {n}"
        );
    }
    for case in cases {
        assert!(matches!(
            case["source_truth"]["status"].as_str(),
            Some("day" | "refused")
        ));
    }
}

/// Every family, every case: the fields that let a reader tell source
/// truth, verifier reach and model measurement apart are present and
/// drawn from the fixed vocabularies; ids are unique across families;
/// the model-reading document really contains the phrase.
#[test]
fn every_family_keeps_the_three_answers_apart() {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (file, inventory) in inventories() {
        let capability = inventory["capability"]
            .as_str()
            .unwrap_or_else(|| panic!("{file}: a capability name"));
        let cases = inventory["cases"]
            .as_array()
            .unwrap_or_else(|| panic!("{file}: cases"));
        assert!(!cases.is_empty(), "{file}: an empty family says nothing");
        for case in cases {
            let id = case["id"].as_str().unwrap_or_else(|| panic!("{file}: id"));
            if let Some(other) = seen.insert(id.to_owned(), file.clone()) {
                panic!("{id} appears in both {other} and {file}");
            }
            assert_eq!(case["capability"].as_str(), Some(capability), "{id}");
            assert!(
                !case["form"].as_str().unwrap_or_default().is_empty(),
                "{id}: form"
            );
            let phrase = case["phrase"].as_str().unwrap_or_default();
            assert!(!phrase.is_empty(), "{id}: phrase");
            assert!(
                case["source_truth"]["status"].is_string(),
                "{id}: source_truth.status"
            );
            let provenance = &case["provenance"];
            assert!(
                provenance["revision"].is_string(),
                "{id}: provenance.revision"
            );
            let source = &provenance["independent_source"];
            // A citation carries a locator a reader can follow and the
            // sentence relied on — never a fetchable address, which the
            // privacy boundary permits only under `assurance/`.
            assert!(
                source == "pending"
                    || (source["locator"].is_string() && source["quoted_rule"].is_string()),
                "{id}: independent_source is 'pending' or carries locator and quoted_rule"
            );
            assert!(
                case["consumer_packs"]
                    .as_array()
                    .is_some_and(|packs| !packs.is_empty()),
                "{id}: consumer_packs"
            );
            let verifier = &case["verifier"];
            assert!(verifier["boundary"].is_string(), "{id}: verifier.boundary");
            let coverage = verifier["coverage"].as_str().unwrap_or_default();
            assert!(
                VERIFIER_COVERAGE.contains(&coverage),
                "{id}: verifier coverage {coverage:?} is not in the vocabulary"
            );
            let owner = if capability == "dates-and-periods" {
                Some((
                    "reading_vocabulary.rs",
                    "every_surface_form_reads_as_the_table_says",
                ))
            } else if verifier["check"].is_object() {
                Some((
                    "inventory_verifier.rs",
                    "every_checked_case_comes_out_as_the_inventory_says",
                ))
            } else {
                None
            };
            if let Some((file, name)) = owner {
                let test = &verifier["test"];
                assert_eq!(test["file"], format!("crates/runner/tests/{file}"), "{id}");
                assert_eq!(test["name"], name, "{id}");
                assert!(
                    !test["scope"].as_str().unwrap_or_default().is_empty(),
                    "{id}: check scope"
                );
                assert!(
                    matches!(coverage, "main-check" | "unsupported"),
                    "{id}: stale check status"
                );
                let source = std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests")
                        .join(file),
                )
                .unwrap();
                assert!(
                    source.contains(&format!("fn {name}(")),
                    "{id}: owning test missing"
                );
            } else {
                assert!(
                    verifier["test"].is_null(),
                    "{id}: no adapter executes this inventory case"
                );
                assert_ne!(
                    coverage, "main-check",
                    "{id}: no adapter executes this inventory case"
                );
            }
            if matches!(
                verifier["check"]["expected"].as_str(),
                Some("accepted-misread" | "not-a-sum")
            ) {
                assert_eq!(
                    coverage, "unsupported",
                    "{id}: an executable limitation is not a supported capability"
                );
            }
            // A form the verifier cannot check must say why, so an
            // unsupported capability is never a silent pass.
            if matches!(coverage, "unsupported" | "none") {
                assert!(
                    verifier["known_gap"].is_string(),
                    "{id}: {coverage} coverage needs a known_gap saying what is missing"
                );
            }
            let model = &case["model_reading"];
            let model_coverage = model["coverage"].as_str().unwrap_or_default();
            assert!(
                MODEL_COVERAGE.contains(&model_coverage),
                "{id}: model coverage {model_coverage:?} is not in the vocabulary"
            );
            assert!(model["case_id"].is_string(), "{id}: model_reading.case_id");
            assert!(
                model["document"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(phrase),
                "{id}: the model-reading document does not contain the phrase"
            );
        }
    }
}

/// The families the plan names are all present, so a missing capability
/// is visible as a missing file rather than an absent row nobody counts.
#[test]
fn every_planned_capability_family_has_an_inventory() {
    let present: BTreeSet<String> = inventories()
        .into_iter()
        .map(|(_, inventory)| inventory["capability"].as_str().unwrap().to_owned())
        .collect();
    for family in [
        "dates-and-periods",
        "times",
        "money-and-quantities",
        "obligations",
        "parties-places-and-references",
        "relationships",
        "formats",
    ] {
        assert!(present.contains(family), "no inventory for {family}");
    }
}
