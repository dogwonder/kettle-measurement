//! The renewal bed (#66, AUTHORING steps 2–3).
//!
//! The bed on disk is generated, so the thing to hold is that it is
//! *exactly* what the committed spec generates, and that what the spec
//! generates is worth measuring against.

use runner::eval::fixture::fixtures_in;
use runner::eval::renewals::{committed_spec, generate};
use runner::packs::load_pack;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff")
}

/// The bed on disk is the spec's output, byte for byte. A bed that
/// drifted from its generator is a bed nobody can regenerate, and the
/// spec stops describing what was actually measured.
#[test]
fn regenerating_the_renewal_bed_reproduces_it_byte_for_byte() {
    let spec = committed_spec(&pack_dir()).expect("the committed spec reads");
    let dir = pack_dir().join("fixtures");

    let mut differ = Vec::new();
    for renewal in generate(&spec) {
        for (name, want) in [
            (format!("{}-previous.txt", renewal.stem), renewal.previous),
            (format!("{}-renewal.txt", renewal.stem), renewal.renewal),
            (format!("{}.expected.json", renewal.stem), renewal.expected),
        ] {
            match std::fs::read_to_string(dir.join(&name)) {
                Ok(found) if found == want => {}
                _ => differ.push(name),
            }
        }
    }

    assert!(
        differ.is_empty(),
        "{} file(s) differ from the spec — run `kettle bed --pack-dir packs/app.kttl.renewal-diff`: {:?}",
        differ.len(),
        &differ[..differ.len().min(5)]
    );
}

/// Generation is pure: same spec in, same bytes out, on any machine and
/// on any day. Nothing here may read a clock or a random number.
#[test]
fn generation_is_deterministic() {
    let spec = committed_spec(&pack_dir()).expect("spec reads");

    assert_eq!(generate(&spec), generate(&spec));
}

/// The claim `renewals.rs` makes in its own comments, held to.
///
/// Two identical passages are **one** decision, not two (#310). A bed
/// that repeated a sentence would count one answer many times over
/// while reading like more evidence — which is exactly how the letter
/// bed's 415 rows turned out to be 23 sentences.
#[test]
fn no_two_passages_are_identical() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    // A relation twin (#427, #433) plants nothing: it re-binds or
    // re-presents a source fixture's passages, and its repeats collapse
    // into the same decision keys. The guard is for the generator
    // planting accidental duplicates, which a declared twin is not —
    // and the declaration is the exemption: any fixture named as a
    // relation's right-hand side.
    let relations: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(pack_dir().join("fixtures/relations.json"))
            .expect("relations read"),
    )
    .expect("relations parse");
    let twins: std::collections::BTreeSet<String> = relations["relations"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(|relation| relation["right"].as_str().map(str::to_owned))
        .collect();

    let mut seen: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
    for fixture in &fixtures {
        if twins.contains(&fixture.expected.fixture_id) {
            continue;
        }
        for item in &fixture.expected.policy_terms {
            let folded = item
                .segment
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            seen.entry((
                fixture.expected.eval_set.as_str().to_owned(),
                item.role.clone(),
                folded,
            ))
            .or_default()
            .push(fixture.name.clone());
        }
    }

    let repeated: Vec<_> = seen.iter().filter(|(_, at)| at.len() > 1).collect();
    assert!(
        repeated.is_empty(),
        "{} passage(s) appear more than once in the same set and role: {:?}",
        repeated.len(),
        &repeated[..repeated.len().min(3)]
    );
}

/// Every expectation says which document it is about, and names a role
/// the pack declares. A term expectation with no role cannot be
/// attributed, and attribution is the failure a renewal diff cannot
/// survive — it turns a rise into a cut.
#[test]
fn every_expectation_names_a_declared_role() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let declared: BTreeSet<&str> = pack
        .manifest
        .inputs
        .iter()
        .map(|input| input.role.as_str())
        .collect();

    let fixtures = fixtures_in(&pack).expect("fixtures load");
    assert!(!fixtures.is_empty(), "the bed is not empty");
    for fixture in &fixtures {
        assert_eq!(fixture.inputs.len(), 2, "{} is a pair", fixture.name);
        for item in &fixture.expected.policy_terms {
            assert!(
                declared.contains(item.role.as_str()),
                "{} expects '{}' from an undeclared role {:?}",
                fixture.name,
                item.id,
                item.role
            );
        }
    }
}

/// A quote must be findable in the passage it claims to come from —
/// the guardrail #258 imposes on the *run* applies to the bed too. An
/// expectation whose own quote is not in its own passage would be
/// unreachable: the run would route it to a person and the bed would
/// score a miss for a reading nobody could have made.
#[test]
fn every_expected_quote_is_in_its_own_passage() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    for fixture in &fixtures {
        for item in &fixture.expected.policy_terms {
            let Some(expect) = &item.expect else { continue };
            let squash = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                squash(&item.segment).contains(&squash(&expect.quote)),
                "{}: '{}' quotes words that are not in its passage",
                fixture.name,
                item.id
            );
            assert!(
                item.segment.contains(&expect.value),
                "{}: '{}' expects a value the passage never writes",
                fixture.name,
                item.id
            );
        }
    }
}

/// The bed measures both halves of the lens. A bed of passages that all
/// state a value would reward answering "yes" every time and could not
/// measure an invention; one with no unmodelled values could not show
/// the pack's honest edge.
#[test]
fn the_bed_plants_values_absences_and_unmodelled_terms() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for fixture in &fixtures {
        for item in &fixture.expected.policy_terms {
            for stratum in item.strata.iter().filter(|s| *s != "any-schedule") {
                *counts.entry(stratum.as_str()).or_default() += 1;
            }
        }
    }

    for planted in ["value-stated", "states-nothing", "unmodelled-value"] {
        assert!(
            counts.get(planted).copied().unwrap_or(0) > 0,
            "the bed plants no {planted}: {counts:?}"
        );
    }
}

/// A per-fixture gate reads `decisions_in`, and until #430 that count
/// knew nothing about policy terms — every renewal fixture read as
/// empty, so a gate this bed's decisions comfortably support would be
/// refused as ungateable. The renewal pack gates pooled today; this
/// holds the count honest for the pack type, not the current choice.
#[test]
fn per_fixture_gating_counts_the_term_decisions_a_fixture_carries() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    // The median fixture carries 6 scored term decisions, so a 0.8 bar
    // (5 decisions before one error can still clear it) fits this bed.
    let mut eval = BTreeMap::new();
    eval.insert("policy-terms".to_owned(), 0.8_f32);
    runner::eval::Thresholds::from_eval(&eval)
        .with_gate(runner::eval::Gate::PerFixture)
        .fits(&fixtures)
        .expect("six decisions per median fixture support a 0.8 per-fixture bar");
}

/// The bed half of #461. A commercial schedule really does write a bare
/// `Excess:` under a section heading, and the correct reading is a
/// referral — the bed authors that as `review: true` with no term
/// named, because naming one would demand a determinate answer to an
/// indeterminate question, which is the bug this family exists to stop
/// (#457). Two sub-shapes with different verdict weight: both documents
/// bare goes in the gated stratum (asserting on a coin flip is the
/// harm), while labelled-plus-bare — where a model may harmonise the
/// bare line with the other document's label, an evidenced inference no
/// field data yet convicts — is measured in its own stratum and never
/// gated. Promotion of the residual into the gate is a bed change made
/// on independent evidence (#428), not an authoring-time guess.
/// Asserted independently for each set. Fails today at 0.
#[test]
fn the_bed_carries_unqualified_excess_referrals() {
    let spec = committed_spec(&pack_dir()).expect("the renewal bed spec");
    let mut gated: BTreeMap<String, usize> = BTreeMap::new();
    let mut residual: BTreeMap<String, usize> = BTreeMap::new();

    for renewal in generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&renewal.expected).expect("expectations are json");
        let set = expected["eval_set"].as_str().expect("a set").to_owned();
        for item in expected["policy-terms"].as_array().expect("terms") {
            let referral =
                item["review"] == serde_json::Value::Bool(true) && item["expect"].is_null();
            if !referral {
                continue;
            }
            let strata: Vec<&str> = item["strata"]
                .as_array()
                .expect("strata")
                .iter()
                .filter_map(|s| s.as_str())
                .collect();
            if strata.contains(&"excess-unqualified") {
                assert!(
                    strata.contains(&"any-schedule"),
                    "the both-bare case is the settled harm and belongs in the gate: {item}"
                );
                *gated.entry(set.clone()).or_default() += 1;
            }
            if strata.contains(&"excess-unqualified-residual") {
                assert!(
                    !strata.contains(&"any-schedule"),
                    "the residual is measured, never gated — a gate that adjudicates \
                     an open question produces verdicts the evidence cannot back: {item}"
                );
                *residual.entry(set.clone()).or_default() += 1;
            }
        }
    }

    for set in ["development", "exam"] {
        assert!(
            gated.get(set).copied().unwrap_or(0) >= 8,
            "the {set} set carries too few gated unqualified-excess referrals: {gated:?}"
        );
        assert!(
            residual.get(set).copied().unwrap_or(0) >= 4,
            "the {set} set carries too few residual referrals: {residual:?}"
        );
    }
}
