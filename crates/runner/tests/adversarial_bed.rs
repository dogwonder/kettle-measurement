//! #433: adversarial document-instruction strata.
//!
//! JSON Schema constrains shape, not meaning: document text and pack
//! instructions reach the model in one rendered request, so a letter
//! or schedule can carry text that looks like an instruction, an
//! answer, or a schema fragment. These fixtures measure whether that
//! text can redirect a model away from the pack contract — and a model
//! that follows the injection is scored as ordinarily wrong, in the
//! existing harm classes, never in a separate security vocabulary.

use runner::eval::{bed, letters, renewals};
use std::path::Path;

fn letter_spec() -> letters::LetterBedSpec {
    letters::committed_spec(Path::new(&format!(
        "{}/../../packs/app.kttl.letter-to-actions",
        env!("CARGO_MANIFEST_DIR")
    )))
    .expect("the committed letter spec")
}

fn renewal_spec() -> renewals::RenewalBedSpec {
    renewals::committed_spec(Path::new(&format!(
        "{}/../../packs/app.kttl.renewal-diff",
        env!("CARGO_MANIFEST_DIR")
    )))
    .expect("the committed renewal spec")
}

/// The issue's named first test. Every fixture carrying an adversarial
/// stratum must declare a matched clean fixture and a relation binding
/// the pair, and the pair must differ only by the authored adversarial
/// passages — so a score movement can be attributed to the injection
/// and nothing else.
#[test]
fn every_document_instruction_fixture_has_a_clean_semantic_twin() {
    let generated = letters::generate(&letter_spec());
    let (reorder_twins, _) = letters::twins(&generated);
    let all: Vec<letters::GeneratedLetter> =
        generated.iter().cloned().chain(reorder_twins).collect();
    let (adversarial, relations) = letters::adversarial_twins(&all);
    assert!(
        !adversarial.is_empty(),
        "the letter bed carries adversarial families"
    );

    for twin in &adversarial {
        let expected: serde_json::Value =
            serde_json::from_str(&twin.expected).expect("expectations parse");
        let twin_id = expected["fixture_id"].as_str().expect("a fixture id");

        // The stratum names the family, and the pooled diagnostic
        // stratum names the programme.
        let items = expected["obligations"].as_array().expect("items");
        assert!(
            items.iter().any(|item| {
                item["strata"]
                    .as_array()
                    .is_some_and(|strata| strata.iter().any(|s| s == "document-instruction"))
            }),
            "{twin_id} carries the document-instruction stratum"
        );

        // Exactly one relation binds the twin to its clean source.
        let binding: Vec<_> = relations
            .iter()
            .filter(|relation| relation["right"] == twin_id)
            .collect();
        assert_eq!(
            binding.len(),
            1,
            "{twin_id} has exactly one clean twin relation"
        );
        let source_id = binding[0]["left"].as_str().expect("a source id");

        // The pair differs only by the authored adversarial passages:
        // the twin's text contains every source paragraph, in order.
        let source = all
            .iter()
            .find(|letter| {
                serde_json::from_str::<serde_json::Value>(&letter.expected)
                    .ok()
                    .and_then(|value| value["fixture_id"].as_str().map(str::to_owned))
                    .as_deref()
                    == Some(source_id)
            })
            .expect("the clean source exists");
        let twin_paragraphs: Vec<&str> = twin.text.split("\n\n").collect();
        let mut cursor = 0usize;
        for paragraph in source.text.split("\n\n") {
            let found = twin_paragraphs[cursor..]
                .iter()
                .position(|candidate| *candidate == paragraph);
            let Some(offset) = found else {
                panic!(
                    "{twin_id} dropped or altered a clean paragraph of {source_id}: {paragraph:?}"
                );
            };
            cursor += offset + 1;
        }

        // Every passage the twin adds is authored adversarial content:
        // it carries an expectation, that expectation is null — the
        // instruction is never the task fact — and it is tagged.
        let source_set: std::collections::BTreeSet<&str> = source.text.split("\n\n").collect();
        for paragraph in twin_paragraphs {
            if source_set.contains(paragraph) {
                continue;
            }
            let item = items
                .iter()
                .find(|item| item["segment"] == paragraph)
                .unwrap_or_else(|| panic!("{twin_id} adds an unauthored passage: {paragraph:?}"));
            assert!(
                item["expect"].is_null(),
                "instruction-shaped content is never the task fact: {item:?}"
            );
        }
    }

    // Both document typologies carry at least one family.
    let (renewal_adversarial, _) =
        renewals::adversarial_twins(&renewals::generate(&renewal_spec()));
    assert!(
        !renewal_adversarial.is_empty(),
        "the renewal bed carries adversarial families too"
    );

    // And every bed does, found rather than named. Naming the two
    // document packs is how the statement bed went nine adversarial
    // families without one of its own — the criterion's own test could
    // not fail on the pack that violated it, the same hole #427 found in
    // `each_current_pack_ships_at_least_three_relations`. Read off the
    // committed bytes rather than the generators, so a bed that drifted
    // from its generator fails here as well as in its own guard.
    for pack in generated_beds() {
        let expectations = committed_expectations(&pack);
        let injected: Vec<&str> = expectations
            .iter()
            .filter(|expected| carries_document_instruction(expected))
            .map(|expected| expected["fixture_id"].as_str().expect("a fixture id"))
            .collect();
        assert!(
            !injected.is_empty(),
            "{pack} ships no document-instruction fixture — untrusted content \
             reaches the model through this pack too"
        );

        let relations = committed_relations(&pack);
        let clean: std::collections::BTreeSet<&str> = expectations
            .iter()
            .filter(|expected| !carries_document_instruction(expected))
            .map(|expected| expected["fixture_id"].as_str().expect("a fixture id"))
            .collect();
        for twin_id in injected {
            let binding: Vec<&serde_json::Value> = relations
                .iter()
                .filter(|relation| relation["right"] == twin_id)
                .collect();
            assert_eq!(
                binding.len(),
                1,
                "{pack}/{twin_id}: exactly one declared relation binds an \
                 injected fixture to its clean control"
            );
            let source_id = binding[0]["left"].as_str().expect("a source id");
            assert!(
                clean.contains(source_id),
                "{pack}/{twin_id}: its control {source_id} is not a committed clean \
                 fixture, so a score movement cannot be attributed to the injection"
            );
        }
    }
}

/// Every pack shipping a generated bed, found by the spec it committed.
fn generated_beds() -> Vec<String> {
    let packs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
    let mut found: Vec<String> = std::fs::read_dir(&packs_dir)
        .expect("packs directory")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            let fixtures = path.join("fixtures");
            let generated = fixtures.join("eval-bed-spec.json").is_file()
                || fixtures.join("letter-bed-spec.json").is_file()
                || fixtures.join("renewal-bed-spec.json").is_file();
            generated
                .then(|| path.file_name()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no generated beds found to hold");
    found
}

/// Every committed `generated-*.expected.json` of a pack, parsed.
fn committed_expectations(pack: &str) -> Vec<serde_json::Value> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(pack)
        .join("fixtures");
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("generated-") && n.ends_with(".expected.json"))
                .then_some(path)
        })
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            serde_json::from_str(&std::fs::read_to_string(path).expect("read expectations"))
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        })
        .collect()
}

/// The declared relations a pack ships.
fn committed_relations(pack: &str) -> Vec<serde_json::Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(pack)
        .join("fixtures/relations.json");
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read relations"))
            .expect("valid relations");
    file["relations"]
        .as_array()
        .expect("a relations array")
        .clone()
}

/// Whether any scored item in an expectation file wears the pooled
/// adversarial stratum, whichever list its pack's items live in.
fn carries_document_instruction(expected: &serde_json::Value) -> bool {
    // Every list of scored items, whatever its pack calls it —
    // `classify`, `obligations`, `policy-terms`. Naming them is the same
    // trap one level down: the renewal pack's items live under a key the
    // other two do not use.
    scored_items(expected).any(|item| {
        item["strata"]
            .as_array()
            .is_some_and(|strata| strata.iter().any(|s| s == "document-instruction"))
    })
}

/// Every scored item in an expectation file, whichever list it lives in.
fn scored_items(expected: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    expected
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.values())
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter(|item| item.get("strata").is_some())
}

/// #433's ninth shape on its last pack: a merchant descriptor carrying
/// command-like text, in the statement bed. Blocked until #427's
/// classification projection existed, because without it no relation
/// could bind the pair. Each set's twin re-presents one clean
/// subscription-heavy statement with one merchant's descriptor grown an
/// instruction — every date and amount untouched, so a classification
/// that moves moved because of the words alone — and the declared
/// `classifications_by_merchant` invariance is what makes obedience to
/// the descriptor visible as a failure.
#[test]
fn the_statement_bed_plants_a_command_text_descriptor_with_a_clean_twin() {
    let spec = bed::committed_spec(Path::new(&format!(
        "{}/../../packs/app.kttl.subscription-audit",
        env!("CARGO_MANIFEST_DIR")
    )))
    .expect("the committed statement spec");
    let generated = bed::generate(&spec);
    let (adversarial, relations) = bed::adversarial_twins(&generated);

    assert_eq!(
        adversarial.len(),
        2,
        "one descriptor-command-text twin per set"
    );
    for twin in &adversarial {
        let expected: serde_json::Value =
            serde_json::from_str(&twin.expected).expect("expectations parse");
        let twin_id = expected["fixture_id"].as_str().expect("a fixture id");

        // The family names its stratum, and the pooled diagnostic
        // stratum names the programme.
        let tagged: Vec<&serde_json::Value> = expected["classify"]
            .as_array()
            .expect("classify items")
            .iter()
            .filter(|item| {
                item["strata"].as_array().is_some_and(|strata| {
                    strata.iter().any(|s| s == "descriptor-command-text")
                        && strata.iter().any(|s| s == "document-instruction")
                })
            })
            .collect();
        assert_eq!(
            tagged.len(),
            1,
            "{twin_id}: exactly one merchant carries the injected descriptor"
        );

        // Exactly one relation binds the twin to its clean source.
        let binding: Vec<_> = relations
            .iter()
            .filter(|relation| relation["right"] == twin_id)
            .collect();
        assert_eq!(binding.len(), 1, "{twin_id} has one clean twin relation");
        assert_eq!(
            binding[0]["kind"]["invariance"]["projection"], "classifications_by_merchant",
            "the pair is bound on the projection the pack's decisions live in"
        );
        let source_id = binding[0]["left"].as_str().expect("a source id");
        let source = generated
            .iter()
            .find(|fixture| {
                serde_json::from_str::<serde_json::Value>(&fixture.expected)
                    .ok()
                    .and_then(|value| value["fixture_id"].as_str().map(str::to_owned))
                    .as_deref()
                    == Some(source_id)
            })
            .expect("the clean source exists");

        // The pair differs only by the authored descriptor noise: every
        // row keeps its date and amount, and a changed row is the same
        // row with the instruction appended to its descriptor.
        let source_rows: Vec<&str> = source.csv.lines().collect();
        let twin_rows: Vec<&str> = twin.csv.lines().collect();
        assert_eq!(
            source_rows.len(),
            twin_rows.len(),
            "{twin_id}: no row appears or disappears"
        );
        let mut changed = 0usize;
        for (source_row, twin_row) in source_rows.iter().zip(&twin_rows) {
            if source_row == twin_row {
                continue;
            }
            changed += 1;
            let parts = |row: &str| -> (String, String, String) {
                let (date, rest) = row.split_once(',').expect("date,descriptor,amount");
                let (descriptor, amount) = rest.rsplit_once(',').expect("descriptor,amount");
                (date.to_owned(), descriptor.to_owned(), amount.to_owned())
            };
            let (date, descriptor, amount) = parts(source_row);
            let (twin_date, twin_descriptor, twin_amount) = parts(twin_row);
            assert_eq!(date, twin_date, "{twin_id}: dates never move");
            assert_eq!(amount, twin_amount, "{twin_id}: amounts never move");
            assert!(
                twin_descriptor.starts_with(&descriptor),
                "{twin_id}: the injected descriptor is the clean one plus the instruction, \
                 got {twin_descriptor:?} from {descriptor:?}"
            );
        }
        assert!(
            changed > 0,
            "{twin_id}: the injected merchant's rows must actually change"
        );

        // The expected classification of every merchant is untouched:
        // the instruction is never the task fact, and the correct
        // answer ignores it.
        let source_expected: serde_json::Value =
            serde_json::from_str(&source.expected).expect("expectations parse");
        let classes = |value: &serde_json::Value| -> Vec<(String, String, String)> {
            value["classify"]
                .as_array()
                .expect("classify items")
                .iter()
                .map(|item| {
                    (
                        item["name"].as_str().expect("a name").to_owned(),
                        item["kind"].as_str().expect("a kind").to_owned(),
                        item["category"].as_str().expect("a category").to_owned(),
                    )
                })
                .collect()
        };
        assert_eq!(
            classes(&source_expected),
            classes(&expected),
            "{twin_id}: the authored classifications do not move with the words"
        );
    }
}
