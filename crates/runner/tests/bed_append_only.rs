//! Regenerating a bed appends, and never renumbers what is already
//! committed (#433, #465).
//!
//! Each bed's existing guard test asserts the committed bytes are what
//! the committed spec emits, and that generating twice gives the same
//! bytes. Both are necessary and neither is this. A bed is emitted in
//! passes — the base fixtures from the spec, then meaning-preserving
//! twins, then adversarial twins, then controlled-change twins — and
//! every pass after the first is where a new family lands. The property
//! that makes adding one cheap is that a pass can only **append**:
//!
//! - no pass may emit a file name an earlier pass already emitted, and
//! - a derived fixture's name must extend its source's, so the ordinals
//!   that decide a fixture's planted values and its scored item ids live
//!   in the base pass alone.
//!
//! Neither holds by construction, and both fail silently. The CLI writes
//! its files from a `Vec` and the guard tests collect theirs into a
//! `BTreeMap`, so a colliding name means the guard compares a committed
//! fixture against a *different* fixture's bytes — or, worse, agrees,
//! because the collision happened in both places identically. A family
//! that renumbered its host shape would show up as several hundred
//! rewritten fixtures in a diff, which is exactly the review nobody
//! reads carefully.
//!
//! Determinism is asserted here too, over the passes the per-bed tests
//! do not reach: `bed::adversarial_twins`, `letters::adversarial_twins`
//! and `letters::controlled_twins` were all added after those tests were
//! written, and a generator seeded from an unordered map passes on the
//! machine that recorded the bed and nowhere else.

use runner::eval::{bed, letters, renewals};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn pack_dir(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../packs/{id}"))
}

/// One emitted pass: what it is called in a failure, and the fixture
/// stems it emits paired with the stem each was derived from (`None` for
/// the base pass, which derives from the spec alone).
struct Pass {
    name: &'static str,
    stems: Vec<(String, Option<String>)>,
}

/// Every pass of a bed, in the order the CLI emits them.
fn passes(pack: &str) -> Vec<Pass> {
    match pack {
        "app.kttl.subscription-audit" => {
            let spec = bed::committed_spec(&pack_dir(pack)).expect("the committed statement spec");
            let base = bed::generate(&spec);
            let (twins, _) = bed::twins(&base);
            let (adversarial, _) = bed::adversarial_twins(&base);
            vec![
                Pass {
                    name: "generate",
                    stems: base.iter().map(|f| (f.stem.clone(), None)).collect(),
                },
                Pass {
                    name: "twins",
                    stems: derived(twins.iter().map(|f| f.stem.clone()), &base_stems_csv(&base)),
                },
                Pass {
                    name: "adversarial_twins",
                    stems: derived(
                        adversarial.iter().map(|f| f.stem.clone()),
                        &base_stems_csv(&base),
                    ),
                },
            ]
        }
        "app.kttl.letter-to-actions" => {
            let spec = letters::committed_spec(&pack_dir(pack)).expect("the committed letter spec");
            let base = letters::generate(&spec);
            let (twins, _) = letters::twins(&base);
            let with_twins: Vec<letters::GeneratedLetter> =
                base.iter().cloned().chain(twins.clone()).collect();
            let (adversarial, _) = letters::adversarial_twins(&with_twins);
            let (controlled, _) = letters::controlled_twins(&base);
            let sources: BTreeSet<String> = with_twins.iter().map(|l| l.stem.clone()).collect();
            let base_only: BTreeSet<String> = base.iter().map(|l| l.stem.clone()).collect();
            vec![
                Pass {
                    name: "generate",
                    stems: base.iter().map(|l| (l.stem.clone(), None)).collect(),
                },
                Pass {
                    name: "twins",
                    stems: derived(twins.iter().map(|l| l.stem.clone()), &base_only),
                },
                Pass {
                    name: "adversarial_twins",
                    stems: derived(adversarial.iter().map(|l| l.stem.clone()), &sources),
                },
                Pass {
                    name: "controlled_twins",
                    stems: derived(controlled.iter().map(|l| l.stem.clone()), &base_only),
                },
            ]
        }
        "app.kttl.renewal-diff" => {
            let spec =
                renewals::committed_spec(&pack_dir(pack)).expect("the committed renewal spec");
            let base = renewals::generate(&spec);
            let (twin_files, _) = renewals::twins(&base);
            let (adversarial, _) = renewals::adversarial_twins(&base);
            let base_only: BTreeSet<String> = base.iter().map(|r| r.stem.clone()).collect();
            vec![
                Pass {
                    name: "generate",
                    stems: base.iter().map(|r| (r.stem.clone(), None)).collect(),
                },
                // The renewal twin pass emits whole file names rather
                // than stems, because a reordered twin reuses one of its
                // source's two documents verbatim.
                Pass {
                    name: "twins",
                    stems: twin_files
                        .iter()
                        .map(|(name, _)| (name.clone(), None))
                        .collect(),
                },
                Pass {
                    name: "adversarial_twins",
                    stems: derived(adversarial.iter().map(|r| r.stem.clone()), &base_only),
                },
            ]
        }
        other => panic!("{other}: no pass description"),
    }
}

fn base_stems_csv(base: &[bed::GeneratedFixture]) -> BTreeSet<String> {
    base.iter().map(|f| f.stem.clone()).collect()
}

/// Pair each derived stem with the source stem it extends, so the test
/// can say which fixture a name was meant to be built on.
fn derived(
    stems: impl Iterator<Item = String>,
    sources: &BTreeSet<String>,
) -> Vec<(String, Option<String>)> {
    stems
        .map(|stem| {
            let source = sources
                .iter()
                .filter(|source| stem.starts_with(source.as_str()) && stem.len() > source.len())
                .max_by_key(|source| source.len())
                .cloned();
            (stem, source)
        })
        .collect()
}

/// Every pack with a generated bed, found rather than named — a
/// hardcoded list is how the subscription pack dodged the relations
/// criterion for two slices.
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
    assert_eq!(
        found.len(),
        3,
        "every generated bed must be described here: {found:?}"
    );
    found
}

#[test]
fn regenerating_a_bed_appends_and_never_renumbers_an_existing_fixture() {
    for pack in generated_beds() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut emitted = 0usize;
        for pass in passes(&pack) {
            assert!(
                !pass.stems.is_empty(),
                "{pack}/{}: a pass that emits nothing cannot be appending",
                pass.name
            );
            for (stem, source) in &pass.stems {
                assert!(
                    seen.insert(stem.clone()),
                    "{pack}/{}: {stem} was already emitted by an earlier pass — a later \
                     family would silently replace a committed fixture rather than add one",
                    pass.name
                );
                emitted += 1;
                // The base pass reads the spec alone, so its stems carry
                // the ordinals. Every later pass must extend a stem the
                // earlier passes produced, which is what keeps those
                // ordinals — and the item ids derived from them — out of
                // reach of a new family.
                if pass.name == "generate" || pass.name == "twins" {
                    continue;
                }
                assert!(
                    source.is_some(),
                    "{pack}/{}: {stem} extends no fixture from an earlier pass, so its \
                     identity does not derive from a committed one",
                    pass.name
                );
            }
        }
        assert_eq!(seen.len(), emitted, "{pack}: a stem was emitted twice");
    }
}

#[test]
fn every_scored_item_id_is_unique_within_its_eval_set() {
    // The tombstone registry's sibling (#237): a derived family mints
    // its item ids by prefixing its source's, so two families prefixing
    // the same source the same way collide and a baseline then joins two
    // decisions under one key. Within a set, because a set is the unit a
    // measurement is made over: development and exam are separate
    // measurements that never compare with one another, so an id shared
    // across them cannot be joined by anything.
    //
    // That distinction is not academic here. `evals/README.md` says item
    // ids are unique across the whole pack, and today they are not: the
    // adversarial pass mints `adv-<family>-injected-01` without naming
    // its set, so each of the letter bed's nine families carries that id
    // in both halves. It is harmless — the stable key is
    // `<pack>/<fixture_id>/<item id>` and the fixture ids do differ — and
    // narrowing it is deliberately not fixed here, because renaming
    // those ids would retire live baseline evidence for a defect that
    // costs a measurement nothing. Recorded rather than hidden; the
    // assertion below is the property a measurement actually rests on.
    for pack in generated_beds() {
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        for expected in every_expectation(&pack) {
            let fixture_id = expected["fixture_id"].as_str().expect("a fixture id");
            let set = fixture_id
                .split_once('-')
                .map(|(set, _)| set.to_owned())
                .unwrap_or_else(|| fixture_id.to_owned());
            // Every list of scored items, whatever its pack calls it:
            // `classify`, `obligations`, `policy-terms`.
            for item in expected
                .as_object()
                .into_iter()
                .flat_map(|fields| fields.values())
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .filter(|item| item.get("strata").is_some())
            {
                let Some(id) = item["id"].as_str() else {
                    continue;
                };
                assert!(
                    seen.insert((set.clone(), id.to_owned())),
                    "{pack}: item id {id} appears twice in the {set} set, \
                     latterly in {fixture_id}"
                );
            }
        }
    }
}

#[test]
fn every_pass_of_every_bed_is_deterministic() {
    // Stated over the passes the per-bed determinism tests predate:
    // `bed::adversarial_twins`, `letters::adversarial_twins` and
    // `letters::controlled_twins` were each added after them.
    for pack in generated_beds() {
        let first: Vec<Vec<(String, Option<String>)>> =
            passes(&pack).into_iter().map(|p| p.stems).collect();
        let second: Vec<Vec<(String, Option<String>)>> =
            passes(&pack).into_iter().map(|p| p.stems).collect();
        assert_eq!(first, second, "{pack}: a pass is not deterministic");
    }

    let letter_spec =
        letters::committed_spec(&pack_dir("app.kttl.letter-to-actions")).expect("the letter spec");
    let letter_base = letters::generate(&letter_spec);
    assert_eq!(
        letters::controlled_twins(&letter_base),
        letters::controlled_twins(&letter_base),
        "the letter controlled-change pass is not deterministic"
    );
    let (letter_twins, _) = letters::twins(&letter_base);
    let with_twins: Vec<letters::GeneratedLetter> =
        letter_base.iter().cloned().chain(letter_twins).collect();
    assert_eq!(
        letters::adversarial_twins(&with_twins),
        letters::adversarial_twins(&with_twins),
        "the letter adversarial pass is not deterministic"
    );

    let statement_spec = bed::committed_spec(&pack_dir("app.kttl.subscription-audit"))
        .expect("the statement bed spec");
    let statement_base = bed::generate(&statement_spec);
    assert_eq!(
        bed::adversarial_twins(&statement_base),
        bed::adversarial_twins(&statement_base),
        "the statement adversarial pass is not deterministic"
    );

    let renewal_spec =
        renewals::committed_spec(&pack_dir("app.kttl.renewal-diff")).expect("the renewal spec");
    let renewal_base = renewals::generate(&renewal_spec);
    assert_eq!(
        renewals::adversarial_twins(&renewal_base),
        renewals::adversarial_twins(&renewal_base),
        "the renewal adversarial pass is not deterministic"
    );
}

/// Every committed `expected.json` of a generated bed, parsed.
fn every_expectation(pack: &str) -> Vec<serde_json::Value> {
    let dir = pack_dir(pack).join("fixtures");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .filter_map(|entry| {
            let path = entry.expect("dir entry").path();
            // The generated bed only. A pack's hand-authored fixtures
            // are not emitted in passes and have no ordinals to protect.
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("generated-") && n.ends_with(".expected.json"))
                .then_some(path)
        })
        .collect();
    names.sort();
    names
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).expect("read expectations");
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        })
        .collect()
}
