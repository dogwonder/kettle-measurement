//! #265: the generated bed must be its generator's output, not a
//! parallel artefact that happens to look like it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
}

/// Every committed `generated-*` file, keyed by file name.
fn committed_bed() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let dir = pack_dir().join("fixtures");
    for entry in std::fs::read_dir(&dir).expect("fixtures directory") {
        let path = entry.expect("dir entry").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("generated-") && name != "relations.json" {
            continue;
        }
        files.insert(
            name.to_owned(),
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}")),
        );
    }
    files
}

#[test]
fn regenerating_the_bed_reproduces_it_byte_for_byte() {
    // The whole issue in one assertion. Red today because there is no
    // generator; green only when the committed bed is exactly what the
    // committed spec emits.
    //
    // This is what stops the bed and its generator drifting apart
    // again, and it is what turns #256 (a large fixture, PDF-scored
    // items) and #257 (merchant naming) from multi-hundred-file hand
    // patches into diffs a person can read.
    let spec = runner::eval::bed::committed_spec(&pack_dir()).expect("the committed bed spec");

    let fixtures = runner::eval::bed::generate(&spec);
    let (twins, mut relations) = runner::eval::bed::twins(&fixtures);
    let (adversarial, adversarial_relations) = runner::eval::bed::adversarial_twins(&fixtures);
    relations.extend(adversarial_relations);
    let mut generated: BTreeMap<String, String> = BTreeMap::new();
    for fixture in fixtures.into_iter().chain(twins).chain(adversarial) {
        generated.insert(format!("{}.csv", fixture.stem), fixture.csv);
        generated.insert(format!("{}.expected.json", fixture.stem), fixture.expected);
    }
    generated.insert(
        "relations.json".to_owned(),
        runner::eval::renewals::relations_file(relations),
    );

    let committed = committed_bed();
    assert!(
        !committed.is_empty(),
        "no generated-* fixtures found — the bed itself is missing"
    );

    // Names first: a missing or surplus fixture is a different failure
    // from a fixture whose contents moved, and saying which is which is
    // the difference between a five-minute fix and an afternoon.
    let generated_names: Vec<&String> = generated.keys().collect();
    let committed_names: Vec<&String> = committed.keys().collect();
    assert_eq!(
        generated_names, committed_names,
        "the generator emits a different set of fixtures than is committed"
    );

    for (name, want) in &committed {
        assert_eq!(
            generated.get(name),
            Some(want),
            "{name} differs from what the generator emits"
        );
    }
}

#[test]
fn the_old_model_framing_is_out_of_the_beds_truth() {
    // #253's last box: two expectations were authored when the model
    // still answered `kind`, and the framing leaked into the truth.
    // Twelve credits are income, not a one-off; and a season ticket
    // renewed a year on — refunded and re-bought or not — genuinely
    // recurs, so its kind follows the pack's transport→utility policy
    // and its series belongs in `recurring`.
    let spec = runner::eval::bed::committed_spec(&pack_dir()).expect("the committed bed spec");

    for fixture in runner::eval::bed::generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&fixture.expected).expect("expectations parse");
        for item in expected["classify"].as_array().expect("classify items") {
            let id = item["id"].as_str().expect("item id");
            if id.ends_with("-monthly-salary") {
                assert_eq!(item["kind"], "income", "{id}: a salary is income");
            }
            if id.ends_with("-annual-season-ticket") {
                assert_eq!(item["kind"], "utility", "{id}: an annual renewal is a bill");
                let merchant = item["name"].as_str().expect("merchant name");
                let series: Vec<&str> = expected["recurring"]
                    .as_array()
                    .expect("recurring list")
                    .iter()
                    .filter(|series| series["merchant"] == merchant)
                    .map(|series| series["period"].as_str().expect("period"))
                    .collect();
                assert_eq!(series, vec!["yearly"], "{id}: the renewal recurs yearly");
            }
        }
    }
}

#[test]
fn every_scored_item_carries_the_pooled_stratum() {
    // #316: the pack gates pooled, on `any-statement`, plus a per-stratum
    // `subscription` ceiling on the three strata where a confident denial
    // is the harm the stratum exists to catch. A pooled gate can only
    // read evidence that is tagged pooled, so the tag belongs on every
    // scored item — the same shape `any-letter` already has in the letter
    // bed, for the same reason (#310: a 5% ceiling needs 73 distinct
    // decisions, and slicing nineteen ways left every slice at eight).
    let spec = runner::eval::bed::committed_spec(&pack_dir()).expect("the committed bed spec");

    for fixture in runner::eval::bed::generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&fixture.expected).expect("expectations parse");
        for item in expected["classify"].as_array().expect("classify items") {
            let id = item["id"].as_str().expect("item id");
            let strata: Vec<&str> = item["strata"]
                .as_array()
                .expect("strata list")
                .iter()
                .map(|s| s.as_str().expect("stratum name"))
                .collect();
            assert!(
                strata.contains(&"any-statement"),
                "{id}: scored but not in the pooled stratum — its evidence \
                 cannot reach the gate that reads it (strata: {strata:?})"
            );
        }
    }
}

#[test]
fn generating_twice_gives_the_same_bytes() {
    // Determinism stated separately, because the failure it catches is
    // different: a generator seeded from the clock or from an unordered
    // map passes the comparison above on the machine that recorded the
    // bed and nowhere else.
    let spec = runner::eval::bed::committed_spec(&pack_dir()).expect("the committed bed spec");
    assert_eq!(
        runner::eval::bed::generate(&spec),
        runner::eval::bed::generate(&spec),
        "the generator is not deterministic"
    );
    assert_eq!(
        runner::eval::bed::twins(&runner::eval::bed::generate(&spec)).0,
        runner::eval::bed::twins(&runner::eval::bed::generate(&spec)).0,
        "the twin pass is not deterministic"
    );
}
