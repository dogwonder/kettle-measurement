//! #242: the letter bed must be its generator's output, and it must be
//! big enough to support the ceiling its pack declares.
//!
//! The second half is the part #237 was opened about. A bed that cannot
//! carry a gate's evidence does not make the gate lenient — it makes it
//! unmeetable, and a gate that fails for want of evidence reads exactly
//! like a gate that fails for being wrong.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.letter-to-actions")
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
fn regenerating_the_letter_bed_reproduces_it_byte_for_byte() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");

    let mut generated: BTreeMap<String, String> = BTreeMap::new();
    let letters = runner::eval::letters::generate(&spec);
    let (twins, mut relations) = runner::eval::letters::twins(&letters);
    let (controlled, controlled_relations) = runner::eval::letters::controlled_twins(&letters);
    let with_twins: Vec<_> = letters.into_iter().chain(twins).collect();
    let (adversarial, adversarial_relations) =
        runner::eval::letters::adversarial_twins(&with_twins);
    relations.extend(adversarial_relations);
    relations.extend(controlled_relations);
    for letter in with_twins.into_iter().chain(adversarial).chain(controlled) {
        generated.insert(format!("{}.txt", letter.stem), letter.text);
        generated.insert(format!("{}.expected.json", letter.stem), letter.expected);
    }
    generated.insert(
        "relations.json".to_owned(),
        runner::eval::renewals::relations_file(relations),
    );

    let committed = committed_bed();
    assert!(!committed.is_empty(), "the letter bed itself is missing");

    let generated_names: Vec<&String> = generated.keys().collect();
    let committed_names: Vec<&String> = committed.keys().collect();
    assert_eq!(
        generated_names, committed_names,
        "the generator emits a different set of letters than is committed"
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
fn generating_the_letter_bed_twice_gives_the_same_bytes() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    assert_eq!(
        runner::eval::letters::generate(&spec),
        runner::eval::letters::generate(&spec),
        "the letter generator is not deterministic"
    );
}

// `the_bed_carries_the_evidence_its_declared_ceilings_need` moved to
// `bed_sizing.rs` and now counts **distinct decisions** rather than rows
// (#310). It passed here for years of commits while the `no_obligation`
// ceiling rested on 23 sentences repeated across 415 rows — the rule was
// right, the unit was not. It also covers every pack now, not this one.

#[test]
fn every_letter_asking_nothing_is_still_scored() {
    // The invention ceiling is measured on passages that ask for
    // nothing, so a bed that only recorded obligations could not
    // measure inventions at all — it would score a model that invented
    // freely exactly as well as one that did not.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let courtesy: Vec<_> = letters
        .iter()
        .filter(|letter| {
            letter.stem.contains("courtesy-only") || letter.stem.contains("courtesy_only")
        })
        .collect();
    assert!(
        !courtesy.is_empty(),
        "the bed plants no letters that ask nothing"
    );

    for letter in courtesy {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations parse");
        let items = expected["obligations"].as_array().expect("items");
        assert!(!items.is_empty(), "{}: nothing scored", letter.stem);
        assert!(
            items.iter().all(|item| item["expect"].is_null()),
            "{}: a courtesy letter must oblige nothing",
            letter.stem
        );
    }
}

#[test]
fn the_exam_set_is_not_the_development_set_wearing_other_names() {
    // A sealed set exists to be evidence the development set cannot
    // give. Letter content is built from (shape, index) alone — `set`
    // and `family` reach only the file stem — so two sets declaring the
    // same shapes in the same counts generate the same letters twice.
    // That set cannot disagree with the one it was tuned on, and an
    // exam it always agrees with rubber-stamps whatever development
    // produced.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let text_of = |set: &str| -> BTreeMap<String, String> {
        letters
            .iter()
            .filter(|letter| letter.stem.starts_with(&format!("generated-{set}-")))
            .map(|letter| (letter.text.clone(), letter.stem.clone()))
            .collect()
    };
    let development = text_of("development");
    let exam = text_of("exam");
    assert!(
        !development.is_empty() && !exam.is_empty(),
        "a set is empty"
    );

    let shared: Vec<_> = exam
        .iter()
        .filter_map(|(text, stem)| development.get(text).map(|dev| (stem, dev)))
        .collect();
    assert!(
        shared.is_empty(),
        "{} of {} exam letters are byte-identical to a development letter, \
         e.g. {} == {}",
        shared.len(),
        exam.len(),
        shared[0].0,
        shared[0].1
    );
}

/// The shapes whose two voices put a **different** party in the actor
/// position, each with the reason it stands and the date it was
/// measured.
///
/// Staged rather than fixed, and the distinction is the whole point of
/// this list. On 21 August 2026 all seven were re-authored towards
/// development on the reasoning that an exam voice planting a *harder*
/// construction confounds every comparison the pack draws between the
/// sets. The reasoning was sound and the premise was never measured. A
/// full exam run on the corrected bed (RTX 5090, `--exam --runs 3`,
/// scoring 14) said:
///
/// | | before | after |
/// |---|---|---|
/// | gate `any-letter` / obligation | 0.00 PASS | **0.05 FAIL** |
/// | `payment_anchored` recall | 1.00 (36/36) | **0.67 (24/36)** |
/// | `payment_relative` recall | 1.00 (47/47) | **0.91 (43/47)** |
///
/// The actorless voice was never the harder one — those shapes scored
/// 36/36 and 47/47 *as actorless*, and `passive-voice`, the stratum
/// built to measure actorlessness, scores 30/30 in both voices.
///
/// Nor were the obligations missed. **All 36 were found**, `due`
/// identical on both sides, because `obligation_key` keys a dated
/// obligation on the date it resolves to and not on the phrase. What
/// demotes them is `same_assertion_as`, which compares `deadline`
/// verbatim — so against `"You must clear £12.00 within 45 days of 23
/// August 2026."` the answer `"within 45 days of 23 August 2026"` lands
/// in the confident-wrong cell while the obligation it describes is
/// recorded as found. Both strings are the words the letter uses, which
/// is the whole of what the prompt asks for; the old wording merely
/// elicited the bed's one 36 times running.
///
/// So the rewrite did not make a letter harder to read. It changed
/// which of two faithful copies the model produced, against a scorer
/// that answered "is the wording part of the claim" three different
/// ways. That was #554, fixed behind scoring version 15: one
/// `ObligationIdentity`, and the re-authored recording replayed under
/// it reads `payment_anchored` 36/36. What remains of the cost is
/// real — four `payment_relative` day miscounts (`"within 22 days"` for
/// a 21-day letter) and the gate failing on one decision in 252, whose
/// Wilson upper bound is 0.022 against a ceiling of 0.02.
///
/// So the divergence is real, it is an inconsistency worth knowing
/// about, and correcting it still costs a gate PASS for a reason that
/// has nothing to do with actors. It stays listed until a rewrite exists
/// that does not move those four answers — and that rewrite must be
/// measured before it lands, which is the rule this list was created by
/// breaking.
const STAGED_VOICE_DIVERGENCES: [(&str, &str); 7] = [
    (
        "appointment_absolute",
        "development \"You have an appointment…\", exam \"We have booked your … appointment…\" (2026-08-21)",
    ),
    (
        "payment_anchored",
        "development \"Please pay…\", exam \"The sum of £X falls due…\"; re-authoring cost 12 of 36 (2026-08-21)",
    ),
    (
        "payment_month_end",
        "development \"Please settle it…\", exam \"We ask that this is cleared…\" (2026-08-21)",
    ),
    (
        "payment_relative",
        "development \"Please pay…\", exam \"Payment must reach us…\"; re-authoring cost 4 of 47 (2026-08-21)",
    ),
    (
        "repeated_ask",
        "development \"Please pay…\", exam \"Payment of £X towards … is required…\" (2026-08-21)",
    ),
    (
        "three_asks",
        "development asks in the imperative throughout, exam makes one of its three actorless (2026-08-21)",
    ),
    (
        "undated_relative",
        "development \"Please return…\", exam \"The enclosed form … should be completed and returned…\" (2026-08-21)",
    ),
];

/// Who the ask sentence puts in the position of the one who must act.
///
/// Binary, and deliberately so: the prompt's own rule (#458) is *"ask
/// who is being told to act"*, which has two answers — the reader is
/// named, or the reader is not. A finer taxonomy would be a taxonomy of
/// this bed's prose rather than of the question the model is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Actor {
    /// *"Please pay £68.50…"*, *"You must attend…"*, *"We would ask you
    /// to send back…"* — the reader is addressed and is the one acting.
    ReaderNamed,
    /// *"Payment must reach us…"*, *"The enclosed form should be
    /// completed and returned…"* — nobody is named, and the reader has
    /// to work out that it is them.
    ReaderUnnamed,
}

/// Every opening this bed authors, and which actor it puts in the
/// sentence. Closed on purpose: a construction in neither list fails
/// the test rather than defaulting, because defaulting is how a new
/// exam sentence would enter the bed unclassified and unnoticed —
/// which is the defect this test exists to catch.
const CONSTRUCTIONS: [(&str, Actor); 22] = [
    ("Please ", Actor::ReaderNamed),
    ("You have ", Actor::ReaderNamed),
    ("You must ", Actor::ReaderNamed),
    ("You are ", Actor::ReaderNamed),
    ("We would ask you to ", Actor::ReaderNamed),
    ("We would be grateful if you could ", Actor::ReaderNamed),
    ("A payment of ", Actor::ReaderUnnamed),
    ("A written response about ", Actor::ReaderUnnamed),
    ("Confirmation that ", Actor::ReaderUnnamed),
    ("Payment of ", Actor::ReaderUnnamed),
    ("Payment must reach us ", Actor::ReaderUnnamed),
    ("Settlement of ", Actor::ReaderUnnamed),
    ("The amount shown as due ", Actor::ReaderUnnamed),
    ("The enclosed ", Actor::ReaderUnnamed),
    ("The sum of ", Actor::ReaderUnnamed),
    // #552, and the reason this taxonomy is not the whole story. Both
    // this and "Payment of " are ReaderUnnamed, so `invoice_totals`
    // reads as *agreeing* across the two voices and always has — yet
    // one scores 12 of 12 and the other 5 of 12. The binary cannot see
    // the difference between an ask stated deontically and the same ask
    // stated as an outcome, which is the difficulty the exam set plants
    // and development now plants too.
    ("The total shown opposite ", Actor::ReaderUnnamed),
    ("We ask that ", Actor::ReaderUnnamed),
    ("We have booked your ", Actor::ReaderUnnamed),
    // #399, 31 August 2026: a confirmation names the reader ("your
    // appointment") and tells nobody to do anything. Both voices of
    // `appointment_confirmed` open this way on purpose — the real
    // letter that missed did — so the shape plants no divergence.
    ("This letter confirms your ", Actor::ReaderNamed),
    ("We are writing to confirm your ", Actor::ReaderNamed),
    // #399, 1 September 2026: `appointment_preparation`'s booking
    // sentence, in both voices. A booking names the reader's own
    // appointment and instructs nobody, exactly as the two confirmation
    // openings above do, so the shape plants no divergence here either.
    ("Your appointment about your ", Actor::ReaderNamed),
    ("An appointment concerning your ", Actor::ReaderNamed),
];

/// The sentences of a passage, split on a full stop that ends one.
///
/// A decimal point never qualifies — `£1,205.00` is followed by a
/// digit, not by a space — so this needs no money-aware special case.
fn sentences(passage: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = passage;
    while let Some(cut) = rest.find(". ") {
        out.push(rest[..=cut].trim());
        rest = &rest[cut + 2..];
    }
    out.push(rest.trim());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

#[test]
fn both_voices_of_a_shape_ask_in_the_same_construction_unless_staged() {
    // #552. `Voice`'s own contract is that the two voices plant "the
    // same difficulty in different words", and nothing tested it. In
    // seven of twelve shapes development asks in the imperative and
    // exam asks with nobody named, so the contract is not met.
    //
    // What this asserts is narrower than the contract, and deliberately
    // so: the divergence set must be *exactly* what
    // STAGED_VOICE_DIVERGENCES lists. Read that list before assuming
    // the gap should simply be closed — closing it was tried, measured,
    // and cost the obligation gate.
    //
    // The sentence that carries the ask is derived from the bed's own
    // expectation — it is the one containing the `deadline` the
    // expectation demands be copied — so which sentence counts is not
    // a judgement made here.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let mut by_shape: BTreeMap<(String, String), std::collections::BTreeSet<Actor>> =
        BTreeMap::new();

    for letter in &letters {
        let Some(rest) = letter.stem.strip_prefix("generated-") else {
            continue;
        };
        let Some((set, rest)) = rest.split_once('-') else {
            continue;
        };
        let shape = rest.split('-').next().expect("a shape segment").to_owned();

        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations parse");
        for item in expected["obligations"].as_array().expect("items") {
            let Some(deadline) = item["expect"]["deadline"].as_str() else {
                continue;
            };
            let segment = item["segment"].as_str().expect("a segment");

            let carrying: Vec<&str> = sentences(segment)
                .into_iter()
                .filter(|s| s.contains(deadline))
                .collect();
            assert_eq!(
                carrying.len(),
                1,
                "{}: the deadline {deadline:?} must fall in exactly one sentence of \
                 {segment:?}, or the ask cannot be located without a judgement",
                letter.stem
            );

            let ask = carrying[0];
            let actor = CONSTRUCTIONS
                .iter()
                .find(|(opening, _)| ask.starts_with(opening))
                .map(|(_, actor)| *actor)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: the ask {ask:?} is a construction this test does not \
                         classify. Add it to CONSTRUCTIONS with the actor it names — \
                         an unclassified sentence must not default into agreement.",
                        letter.stem
                    )
                });
            by_shape
                .entry((shape.clone(), set.to_owned()))
                .or_default()
                .insert(actor);
        }
    }

    assert!(!by_shape.is_empty(), "no asks were classified at all");

    let shapes: std::collections::BTreeSet<String> =
        by_shape.keys().map(|(shape, _)| shape.clone()).collect();
    let mut diverging = std::collections::BTreeSet::new();
    for shape in &shapes {
        let development = by_shape.get(&(shape.clone(), "development".to_owned()));
        let exam = by_shape.get(&(shape.clone(), "exam".to_owned()));
        if development != exam {
            diverging.insert(shape.clone());
        }
    }

    let staged: std::collections::BTreeSet<String> = STAGED_VOICE_DIVERGENCES
        .iter()
        .map(|(shape, _)| (*shape).to_owned())
        .collect();

    // Both directions fail, and the second is the one that keeps this
    // list honest. A shape that stopped diverging while still listed
    // would leave a staged exception describing nothing — the way a
    // staged govuk component or an unused mixin rots — and the next
    // person would read a measurement that no longer applies as though
    // it still did.
    let unlisted: Vec<&String> = diverging.difference(&staged).collect();
    assert!(
        unlisted.is_empty(),
        "{} shape(s) put a different party in the actor position in each voice and \
         are not staged: {unlisted:?}.\nThe exam voice is meant to plant the same \
         difficulty in different words. Before correcting one, read \
         STAGED_VOICE_DIVERGENCES: the same edit was made on 21 August 2026 and \
         took the pack's obligation gate from 0.00 PASS to 0.05 FAIL, because it \
         moved the sentence around the deadline. Measure on a full exam run before \
         landing a fix, never after.",
        unlisted.len()
    );

    let stale: Vec<&String> = staged.difference(&diverging).collect();
    assert!(
        stale.is_empty(),
        "{} staged voice divergence(s) no longer diverge: {stale:?}. Remove them \
         from STAGED_VOICE_DIVERGENCES — a staged exception that describes nothing \
         is worse than none, because it reads as a known problem that is still there.",
        stale.len()
    );
}

#[test]
fn a_gate_that_cannot_read_this_bed_is_refused_rather_than_applied() {
    // #301's second half. The menu alone is not enough: a pack could
    // still declare `per_fixture` against single-decision fixtures and
    // reproduce the defect with a blessing. On a bed where most
    // fixtures carry one decision, a per-fixture bar above zero means
    // "no errors at all, anywhere" — a gate with no gradient, which
    // reads the same at 444/445 as at 0/445.
    //
    // The refusal has to name the arithmetic, because "gate does not
    // fit" is exactly as unreadable as the verdict it replaces.
    let pack = runner::packs::load_pack(&pack_dir()).expect("the letter pack loads");
    let fixtures = runner::eval::fixture::fixtures_in(&pack).expect("fixtures load");

    let per_fixture = pack
        .thresholds()
        .with_gate(runner::eval::Gate::PerFixture)
        .fits(&fixtures);
    let error = per_fixture.expect_err("per_fixture must be refused on a single-decision bed");
    // The bar is 0.95, so a fixture needs 20 obligations before one
    // wrong answer can still clear it. Both sets load, with the six
    // reorder twins (#427) and eighteen adversarial twins (#433, the
    // delimiter family included), #456's sixty passive letters,
    // #465's two dateless-anchor letters and six controlled twins,
    // #504's twenty-four invoices and #552's twelve more.
    //
    // The total is read off the bed rather than written here. What this
    // asserts is that the refusal *names its arithmetic* — a verdict a
    // person cannot check is as unreadable as the one it replaces — and
    // a literal turned that into a second assertion about the bed's
    // size, which every bed growth then failed.
    assert!(
        error.contains("20 decisions") && error.contains(&format!("of {}", fixtures.len())),
        "the refusal must name the arithmetic over all {} fixtures, got: {error}",
        fixtures.len()
    );

    // The gate this pack actually declares reads the bed it has.
    pack.thresholds()
        .fits(&fixtures)
        .expect("the declared gate fits this bed");
}

#[test]
fn no_set_plants_the_same_letter_twice() {
    // #300. Every varying input to a letter is a cycle in its index, so
    // a shape spending more families than the cycles' common period
    // repeats itself verbatim. Duplicates are not independent
    // observations, but the Wilson bounds the pack's ceilings are read
    // against assume they are, so a bed that counts one letter twice
    // reports a tighter interval than its evidence earns.
    //
    // `the_bed_carries_the_evidence_its_declared_ceilings_need` counts
    // items rather than distinct ones and cannot see this.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    for set in ["development", "exam"] {
        let prefix = format!("generated-{set}-");
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        let mut repeats = Vec::new();
        for letter in letters.iter().filter(|l| l.stem.starts_with(&prefix)) {
            if let Some(first) = seen.insert(&letter.text, &letter.stem) {
                repeats.push(format!("{} == {first}", letter.stem));
            }
        }
        assert!(
            repeats.is_empty(),
            "{set} plants {} letters it has already planted, e.g. {}",
            repeats.len(),
            repeats[0]
        );
    }
}

#[test]
fn every_expected_party_is_named_somewhere_in_its_own_letter() {
    // An expectation the letter never states is not a hard question, it
    // is an unanswerable one, and a model marked wrong for failing to
    // guess it tells us nothing about the model. `undated_relative`
    // dropped its letterhead to make the letter undated and took the
    // sender's only mention with it, so `party` could be reached from
    // nowhere in the text the model is shown.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");

    for letter in runner::eval::letters::generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations parse");
        for item in expected["obligations"].as_array().expect("items") {
            let Some(party) = item["expect"]["party"].as_str() else {
                continue;
            };
            assert!(
                letter.text.contains(party),
                "{}: expects party {party:?}, which its letter never names",
                letter.stem
            );
        }
    }
}

#[test]
fn the_letter_pack_loads_and_declares_a_pipeline_the_runner_can_run() {
    // #242's named first failing test. The pack is data; every builtin
    // and role it names must already exist in the runner, because a
    // pack needing a runner change to work is a pack-format bug and
    // goes back to parts 2-4 of #51.
    let pack = runner::packs::load_pack(&pack_dir()).expect("the letter pack loads");
    assert_eq!(pack.manifest.id, "app.kttl.letter-to-actions");
    assert_eq!(pack.manifest.capabilities, ["read"]);

    for step in &pack.manifest.pipeline {
        match step {
            runner::packs::PipelineStep::Preprocess { implementation } => assert!(
                runner::packs::PREPROCESS_BUILTINS.contains(&implementation.as_str()),
                "{implementation} is not a preprocess builtin"
            ),
            runner::packs::PipelineStep::Aggregate { implementation } => assert!(
                runner::packs::AGGREGATE_BUILTINS.contains(&implementation.as_str()),
                "{implementation} is not an aggregate builtin"
            ),
            runner::packs::PipelineStep::Model {
                role: Some(role), ..
            } => assert!(
                runner::run::ModelRole::declared(Some(role)).is_some(),
                "{role} is a role the runner cannot execute"
            ),
            _ => {}
        }
    }
}

#[test]
fn the_pack_directory_contains_no_executable_code() {
    // The tier-1 promise: a pack is data. Anything runnable in here
    // means the typology's steps are not general enough yet.
    fn walk(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read pack dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, found);
                continue;
            }
            let executable = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("rs" | "js" | "py" | "sh" | "wasm" | "ts")
            );
            if executable {
                found.push(path.display().to_string());
            }
        }
    }
    let mut found = Vec::new();
    walk(&pack_dir(), &mut found);
    assert!(
        found.is_empty(),
        "executable code in a data-only pack: {found:?}"
    );
}

/// #315: the invention ceiling is measured on passages that ask for
/// nothing, and a bed of easy ones flatters every model.
///
/// The 415 no-obligation rows were 23 distinct sentences, and every one
/// of them was a courtesy line — "we appreciate your prompt attention to
/// this", "a prepaid envelope is enclosed". Declining to invent an
/// obligation there is nearly free: there is no date to latch onto, no
/// amount, no imperative. A model that never looks past the surface
/// scores the same as one that reads.
///
/// The passage that actually tests the ceiling is the one that *looks*
/// like an ask and obliges nothing — a payment already received, a
/// change the sender will apply itself, a deadline someone else is
/// working to. Those carry the vocabulary an obligation carries, so
/// getting them right means having read them.
///
/// Asserted as a floor on the tempting ones rather than a ratio, so
/// adding more easy passages can never dilute the requirement.
#[test]
fn the_no_obligation_evidence_is_not_all_courtesy_lines() {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    // What makes a passage tempting: something an obligation would
    // hang on. A date, a sum of money, or a period of time.
    let tempting = |text: &str| {
        let lower = text.to_lowercase();
        MONTHS.iter().any(|month| lower.contains(month))
            || lower.contains('£')
            || lower.contains(" days")
            || lower.contains(" weeks")
    };

    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the letter bed spec");
    let mut by_set: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
    for letter in runner::eval::letters::generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations are json");
        let set = expected["eval_set"].as_str().expect("a set").to_owned();
        for item in expected["obligations"].as_array().expect("obligations") {
            if !item["expect"].is_null() {
                continue;
            }
            let segment = item["segment"].as_str().expect("a segment");
            by_set.entry(set.clone()).or_default().insert(
                segment.split_whitespace().collect::<Vec<_>>().join(" "),
                tempting(segment),
            );
        }
    }

    // 25 of the 73 a 5% ceiling needs: enough that the tempting cases
    // cannot be a rounding error in the evidence, without pretending
    // every no-obligation passage in a real letter looks like an ask.
    const WANTED: usize = 25;
    let mut failures: Vec<String> = Vec::new();
    for (set, passages) in &by_set {
        let tempting = passages.values().filter(|hard| **hard).count();
        if tempting < WANTED {
            failures.push(format!(
                "{set}: {tempting} of {} distinct no-obligation passages carry a date, a sum or \
                 a period — {WANTED} wanted. Declining to invent an obligation in a passage with \
                 nothing to latch onto is not evidence that the ceiling holds",
                passages.len(),
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n  "));
}

/// #456: every ceiling this pack has ever cleared was cleared on
/// imperatives.
///
/// All 462 expected obligations in the development set address the
/// reader by name (*you*/*your*) or by the imperative mood (*Please pay
/// £480.00 within 30 days*). Not one is passive — and passive is the
/// construction official correspondence uses most: *"Payment must be
/// received within 14 days"*, *"The form is to be returned by 3 March"*.
/// Council tax, HMRC, court and NHS letters are full of it.
///
/// This is #378's shape a second time: the bed looked green precisely
/// because it did not ask. It also leaves the #458 prompt edit — *"ask
/// who is being told to act"* — untested against the construction it
/// puts at risk, since a model may reasonably answer "nobody named
/// here" to a passive sentence and record nothing. That is a **miss**,
/// the unrecoverable harm, on the gate with zero headroom: one miss at
/// n=207 takes the Wilson upper bound to 0.0269 and fails 0.02.
///
/// Asserted per set, because the exam voice clearing this on its own is
/// the whole point of holding one out.
#[test]
fn letter_bed_carries_passive_obligations() {
    // Positive evidence, never absence. "Contains no you/your" would
    // pass on a courtesy line, a heading or a blank — and a stratum that
    // can be satisfied by accident measures nothing. These are the
    // constructions the issue names, each one an ask with no actor in
    // the sentence.
    const PASSIVE: [&str; 6] = [
        "must be received",
        "must be returned",
        "is to be returned",
        "is required",
        "are required",
        "should be received",
    ];
    let addressed = |lower: &str| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| matches!(word, "you" | "your"))
    };
    let passive = |text: &str| {
        let lower = text.to_lowercase();
        PASSIVE.iter().any(|form| lower.contains(form)) && !addressed(&lower)
    };

    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the letter bed spec");
    let mut by_set: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
    for letter in runner::eval::letters::generate(&spec) {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations are json");
        let set = expected["eval_set"].as_str().expect("a set").to_owned();
        for item in expected["obligations"].as_array().expect("obligations") {
            // Expectations only: a passive sentence that asks nothing is
            // the near-miss counter-case, and it is counted by
            // `the_no_obligation_evidence_is_not_all_courtesy_lines`.
            if item["expect"].is_null() {
                continue;
            }
            let segment = item["segment"].as_str().expect("a segment");
            by_set.entry(set.clone()).or_default().insert(
                segment.split_whitespace().collect::<Vec<_>>().join(" "),
                passive(segment),
            );
        }
    }

    // 25 per set, matching the no-obligation stratum's reasoning: enough
    // that passive readings cannot be a rounding error in the evidence,
    // without claiming a bounded passive-specific rate, which n=25
    // cannot support and this test does not assert.
    const WANTED: usize = 25;
    let mut failures: Vec<String> = Vec::new();
    for (set, passages) in &by_set {
        let passive = passages.values().filter(|hard| **hard).count();
        if passive < WANTED {
            failures.push(format!(
                "{set}: {passive} of {} distinct expected-obligation passages are passive — \
                 {WANTED} wanted. Every ceiling this pack has cleared was cleared on \
                 imperatives, so a green run says nothing about the construction real \
                 letters use most",
                passages.len(),
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n  "));
}

/// #390: the order letters are generated in is written down, not
/// alphabetical accident. The running ordinal decides each letter's
/// planted values and scored item ids, so a shape added to the spec but
/// not to the written order would be silently skipped — and one
/// inserted anywhere but the end of the order would renumber the
/// 1,421-file bed and orphan every recorded letter baseline, which is
/// what #378's renewals fix was for. The byte-for-byte tests above hold
/// the freeze itself: today's written order reproduces the committed
/// bed exactly.
#[test]
fn every_spec_shape_is_in_the_written_order() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    for (name, set) in [
        ("development", &spec.sets.development),
        ("exam", &spec.sets.exam),
    ] {
        for shape in set.shapes.keys() {
            assert!(
                runner::eval::letters::SHAPE_ORDER.contains(&shape.as_str()),
                "{name}: the spec plants {shape:?} but SHAPE_ORDER does not know it — \
                 generation would silently skip every one of its letters. Append it to \
                 SHAPE_ORDER (append only, #390)"
            );
        }
    }
}

/// #456, found by the first run that could see it: a bed expectation
/// must be answerable from the passage it is asked about.
///
/// Two defects shipped in the passive family and both are the #457
/// pattern — a genuine ambiguity authored into the bed and then scored
/// as a model error.
///
/// **A sender with no sum does not demand payment.** Two of the twelve
/// carry `£0.00` on purpose, and the generator dropped that into a
/// payment construction: *"Settlement of £0.00 must be received within
/// 35 days"*. The model answered `response`, the bed demanded
/// `payment`, and that one disagreement took the obligation gate from
/// PASS to FAIL — a gate with zero headroom failing on a sentence that
/// should never have been written.
///
/// **An anchor must be in the sentence.** 24 of 30 expectations wanted
/// `"the date of this letter"` from passages that never said it. Free
/// today, because `anchor` sits outside the extraction key; not free
/// the day it moves inside (#452).
#[test]
fn every_passive_expectation_is_answerable_from_its_own_passage() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the letter bed spec");
    let mut failures: Vec<String> = Vec::new();

    for letter in runner::eval::letters::generate(&spec) {
        if !letter.stem.contains("passive_obligation") {
            continue;
        }
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations are json");
        for item in expected["obligations"].as_array().expect("obligations") {
            let Some(expect) = item["expect"].as_object() else {
                continue;
            };
            let segment = item["segment"].as_str().expect("a segment");
            let kind = expect["kind"].as_str().expect("a kind");
            let anchor = expect["anchor"].as_str().expect("an anchor");

            // A payment is a demand for a sum, so the passage has to
            // name one that is worth demanding.
            if kind == "payment" && segment.contains("£0.00") {
                failures.push(format!(
                    "{}: expects a payment from \"{segment}\" — a demand for nothing is not \
                     a demand, and the model calling it something else is not an error",
                    letter.stem,
                ));
            }
            // The anchor is copied from the letter's own words, so the
            // words have to be there to copy.
            if !segment.contains(anchor) {
                failures.push(format!(
                    "{}: expects anchor {anchor:?}, which does not appear in \"{segment}\"",
                    letter.stem,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} passive expectation(s) cannot be answered from their passage:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The strata a fixture's scored items declare, keyed by item id.
fn strata_by_item(expected: &str) -> BTreeMap<String, Vec<String>> {
    let expected: serde_json::Value = serde_json::from_str(expected).expect("expectations parse");
    expected["obligations"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| {
            (
                item["id"].as_str().expect("an item id").to_owned(),
                item["strata"]
                    .as_array()
                    .expect("a strata list")
                    .iter()
                    .map(|s| s.as_str().expect("a stratum name").to_owned())
                    .collect(),
            )
        })
        .collect()
}

/// #406, #399: a shape whose reading is contested must not gate.
///
/// The bed puts the payment obligation on the table row carrying the
/// due date; the v14 run put it on the prose that says *pay*. Both are
/// defensible readings of the same letter, and a gate encodes a
/// settled judgement — so an open authoring question sitting inside
/// `any-letter` fails a ceiling for a reason nobody can act on. It did:
/// twelve invoices took `obligation` to 0.05 against a 0.02 ceiling and
/// `no_obligation` to 0.11 against 0.05, from the two sides of the same
/// disagreement, while every mature stratum in the bed answered 1.00.
///
/// Ungated is not unmeasured. Every item still carries its own shape's
/// stratum and its passage tag, which is where the shape is watched
/// until it earns promotion — `in-a-table` under #504, and #399's pair
/// once the prompt work lands.
///
/// The list below is the whole point: opting out is a deliberate,
/// named act, and a shape that dropped out of the ceilings by being
/// forgotten would weaken them silently.
#[test]
fn a_contested_shape_is_measured_but_does_not_gate() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    // Every shape that deliberately does not gate, with the reason it
    // does not and the date it was staged. A list rather than a rule
    // read off `Shape::gates()`, on purpose and for the same reason
    // `STAGED_GOVUK_COMPONENTS` is a list: a shape that opted out of
    // the ceilings by being *forgotten* would weaken them silently,
    // and this test is what makes that impossible. Adding a name here
    // is the deliberate act.
    const UNGATED: [(&str, &str); 4] = [
        // #406/#504, 13 August 2026: the bed and the v14 run disagree
        // about which passage of an invoice carries the obligation.
        ("invoice_totals", "invoice-totals"),
        // #399, 29 August 2026: whether a conditional ask is an
        // obligation is a contested reading, not a settled one.
        ("conditional_advisory", "conditional-advisory"),
        // #399, 31 August 2026: an appointment stated as a confirmation,
        // from `gp_appointment-025-p1.jpg` reading as no ask at high
        // confidence. Selected for being hard by a real letter that
        // failed, which is #581's reason: a pooled bar that falls each
        // time a harm is measured inverts the incentive. Promotes on the
        // condition CHECKLIST.md names.
        ("appointment_confirmed", "appointment-confirmed"),
        // #399, 1 September 2026: where the line between "how to
        // attend" and "what to do before you do" falls is contested by
        // construction — photographic identification sits close enough
        // to it that a reasonable reader would put it on the other
        // side. Staged for that reason and not only for being new.
        ("appointment_preparation", "appointment-preparation"),
    ];

    let mut ungated_items = 0;
    for letter in &letters {
        let staged = UNGATED
            .iter()
            .find(|(shape, _)| letter.stem.contains(shape));
        for (id, strata) in strata_by_item(&letter.expected) {
            if let Some((_, stratum)) = staged {
                ungated_items += 1;
                assert!(
                    !strata.contains(&"any-letter".to_owned()),
                    "{id}: a contested reading is inside the gated stratum (strata: {strata:?})"
                );
                assert!(
                    strata.contains(&(*stratum).to_owned()),
                    "{id}: ungated and untagged is unmeasured (strata: {strata:?})"
                );
            } else {
                assert!(
                    strata.contains(&"any-letter".to_owned()),
                    "{id}: scored but outside the gated stratum (strata: {strata:?})"
                );
            }
        }
    }
    assert!(
        ungated_items > 0,
        "the bed plants none of the staged ungated shapes to check"
    );
}

/// #406, settled by #544: both passages of an invoice are scored, and
/// each carries the decision it can actually support.
///
/// The **pointing sentence** — *"Payment of the total is due by the
/// date shown beside it"* — is where the ask is made, so it carries the
/// obligation, and its deadline is the pointing words themselves. Its
/// `due` is the table's date: `timeline` resolves the pointer against
/// the row, so the bed can expect the date a person is actually given
/// without expecting the model to have worked anything out.
///
/// The **due-date row** — *"Due date 6 March 2026"* — expects nothing.
/// It names no action and no party, so a closed question about that
/// passage alone cannot yield a payment obligation, and one read out of
/// it was invented. That is now where the invention is measured.
///
/// Until #544 these were the other way round, and the v14 run was
/// marked wrong on both, twelve times out of twelve, for giving the
/// answer the prompt asks it for. Both passages stay scored either way,
/// for #442's reason: an assertion on a passage the bed does not score
/// is synthesised as an unauthored item, and an unauthored item is
/// tagged into **every gated stratum of the pack**, having no fixture
/// strata to inherit.
#[test]
fn an_invoice_scores_the_ask_where_it_is_made_and_the_row_as_asking_nothing() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let invoices: Vec<_> = letters
        .iter()
        .filter(|letter| letter.stem.contains("invoice_totals"))
        .collect();
    assert!(!invoices.is_empty(), "the bed plants no invoices to check");

    for letter in invoices {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations parse");
        let items = expected["obligations"].as_array().expect("items");
        let tagged = |stratum: &str| -> Vec<&serde_json::Value> {
            items
                .iter()
                .filter(|item| {
                    item["strata"]
                        .as_array()
                        .expect("a strata list")
                        .iter()
                        .any(|s| s == stratum)
                })
                .collect()
        };

        let pointing = tagged("points-at-a-table");
        assert_eq!(
            pointing.len(),
            1,
            "{}: the passage pointing at the table is not scored",
            letter.stem
        );
        let ask = &pointing[0]["expect"];
        assert!(
            !ask.is_null(),
            "{}: the ask is expected where the letter makes it",
            letter.stem
        );
        assert_eq!(
            ask["kind"], "payment",
            "{}: an invoice asks for payment",
            letter.stem
        );
        let deadline = ask["deadline"].as_str().expect("a deadline phrase");
        assert!(
            pointing[0]["segment"]
                .as_str()
                .expect("a segment")
                .contains(deadline),
            "{}: the expected deadline is the letter's own words: {deadline:?}",
            letter.stem
        );
        assert!(
            ask["due"].is_string(),
            "{}: the pointer resolves, so a person is given the date",
            letter.stem
        );

        let row = tagged("in-a-table");
        assert_eq!(
            row.len(),
            1,
            "{}: the due-date row is not scored",
            letter.stem
        );
        assert!(
            row[0]["expect"].is_null(),
            "{}: a due-date row names no action and no party, so it asks nothing",
            letter.stem
        );
        assert!(
            row[0]["segment"]
                .as_str()
                .expect("a segment")
                .starts_with("Due date"),
            "{}: the scored row is the one carrying the date",
            letter.stem
        );

        // The prose only. The row's segment is the shape's `reads_as`
        // projection — the left column read down — which is what the
        // letter *means* rather than the run of characters it prints,
        // the columns being interleaved on the page.
        assert!(
            letter
                .text
                .contains(pointing[0]["segment"].as_str().expect("a segment")),
            "{}: the scored segment is not in the letter",
            letter.stem
        );
    }
}

/// An invoice must expect the date it prints (#544).
///
/// The due date is computed from the letter's own date and then
/// written out for the page, and the writing-out is where these two
/// can come apart. When they do, the bed prints one date and expects
/// another — so a run that reads the page perfectly is scored wrong,
/// and the fault is invisible because both halves look reasonable
/// separately.
///
/// It was not hypothetical. Replaying the v14 letter run against the
/// settled expectations left exactly two invoices failing, `verge-11`
/// and `wainscot-12` — the two whose due date falls past the year end,
/// printed as 2026 and expected as 2027.
#[test]
fn an_invoice_expects_the_due_date_it_prints() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let mut checked = 0usize;
    for letter in letters
        .iter()
        .filter(|letter| letter.stem.contains("invoice_totals"))
    {
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations parse");
        for item in expected["obligations"].as_array().expect("items") {
            let Some(due) = item["expect"]["due"].as_str() else {
                continue;
            };
            let due = chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d").expect("an ISO date");
            let printed = format!(
                "{} {} {}",
                due.format("%-d"),
                due.format("%B"),
                due.format("%Y")
            );
            assert!(
                letter.text.contains(&printed),
                "{}: expects {printed}, which the letter never prints:\n{}",
                letter.stem,
                letter.text
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the bed plants no dated invoices to check");
}

/// Every committed invoice resolves its pointer, both sets (#544).
///
/// The model half and the Rust half of this shape fail differently and
/// should be asked about separately. Whether a model copies the
/// pointing words is a bed run and costs GPU; whether `timeline` can
/// then reach the row is arithmetic over committed files, and there is
/// no reason to learn it from a rented box.
///
/// So this feeds each fixture's own authored deadline and anchor —
/// what a model answering correctly would have said — through the real
/// segmentation of the real letter, and asks whether the date comes
/// back. It covers the **exam** invoices, which no model run has ever
/// scored on this shape, and it is what would catch a pointer wording
/// or a table layout that only one set uses.
#[test]
fn every_committed_invoice_resolves_its_pointer() {
    let fixtures = pack_dir().join("fixtures");
    let mut checked = 0usize;

    for entry in std::fs::read_dir(&fixtures).expect("the letter pack's fixtures") {
        let path = entry.expect("a fixture").path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.contains("invoice_totals") || !name.ends_with(".expected.json") {
            continue;
        }
        let letter =
            std::fs::read_to_string(path.with_file_name(name.replace(".expected.json", ".txt")))
                .expect("the letter beside its expectations");
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("expectations"))
                .expect("expectations parse");

        let segments = runner::document::segments_from_text(&letter);
        for item in expected["obligations"].as_array().expect("items") {
            let Some(want) = item["expect"].as_object() else {
                continue;
            };
            let segment_text = item["segment"].as_str().expect("a segment");
            let evidence = segments
                .iter()
                .find(|segment| segment.text == segment_text)
                .unwrap_or_else(|| panic!("{name}: no segment reads {segment_text:?}"))
                .clone();

            let obligation = runner::run::Obligation {
                kind: want["kind"].as_str().expect("a kind").to_owned(),
                party: want["party"].as_str().expect("a party").to_owned(),
                ask: "Pay the total".to_owned(),
                deadline: want["deadline"].as_str().expect("a deadline").to_owned(),
                anchor: want["anchor"].as_str().expect("an anchor").to_owned(),
                confidence: "high".to_owned(),
                due: None,
                evidence: vec![evidence],
                dated_by: None,
                disputed: vec![],
            };

            let sorted = runner::timeline::sort_timeline(vec![obligation], &segments);
            let want_due = want["due"].as_str().expect("an expected due date");
            assert_eq!(
                sorted[0].due.map(|resolved| resolved.date.to_string()),
                Some(want_due.to_owned()),
                "{name}: {:?} does not reach {want_due}",
                want["deadline"]
            );
            assert!(
                sorted[0]
                    .dated_by
                    .as_ref()
                    .is_some_and(|row| row.text.contains(&format!(
                        "{}",
                        chrono::NaiveDate::parse_from_str(want_due, "%Y-%m-%d")
                            .expect("an ISO date")
                            .format("%-d %B %Y")
                    ))),
                "{name}: the row carrying the date is not cited for it"
            );
            checked += 1;
        }
    }

    // Both sets, every fixture, one dated obligation each. Counted from
    // the spec rather than written down, so appending invoice families
    // (#552) grows what this checks instead of failing it.
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let invoices: usize = [&spec.sets.development, &spec.sets.exam]
        .iter()
        .map(|set| set.shapes.get("invoice_totals").map_or(0, Vec::len))
        .sum();
    assert_eq!(checked, invoices, "every committed invoice is checked");
}

/// #399: a conditional ask and standing advice are not obligations, and
/// the bed had no way to say so.
///
/// The first real photographed letter through the app — a housing
/// association's *"for information only"* notice — read cleanly and
/// then produced two asks, both at `high` confidence, that the letter
/// never made: a conditional (*"if you rent it out, notify your
/// tenants"*) and standing advice (*"ask to see their ID"*). Neither is
/// something this reader must do because of this letter.
///
/// The prompt is why. Its worked example 903 — *"Please send us a
/// reading from your meter at your earliest convenience"* — teaches
/// that a request with no date is still an obligation, which is true
/// and too broad: it generalises to any sentence in the imperative,
/// including one guarded by a condition the letter cannot know the
/// answer to, and one offering general advice to anybody reading.
///
/// Every ceiling this pack has cleared was cleared on a bed containing
/// neither construction, so a green run said nothing about either. This
/// shape is the counter-example set, and it is deliberately **ungated**:
/// whether a conditional is an obligation is a contested reading, not a
/// settled one, and a gate encodes a settled judgement (#504's
/// precedent). It is measured in its own strata until the prompt work
/// lands and a full run gives a Wilson bound something to say.
#[test]
fn a_conditional_ask_and_standing_advice_are_scored_as_asking_nothing() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let mut by_set: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut checked = 0usize;
    for letter in &letters {
        if !letter.stem.contains("conditional_advisory") {
            continue;
        }
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations are json");
        let set = expected["eval_set"].as_str().expect("a set").to_owned();
        for item in expected["obligations"].as_array().expect("obligations") {
            let id = item["id"].as_str().expect("an item id");
            let strata: Vec<&str> = item["strata"]
                .as_array()
                .expect("a strata list")
                .iter()
                .map(|s| s.as_str().expect("a stratum name"))
                .collect();

            // Every passage of this shape asks nothing. A conditional
            // whose condition the letter cannot resolve, and advice
            // addressed to anyone at all, are both answers of "no
            // obligation" — asserting one is the invention the
            // `no_obligation` ceiling exists to bound.
            assert!(
                item["expect"].is_null(),
                "{id}: this shape plants no obligations, and this item expects one"
            );
            // Contested, so it must not gate — the #504 rule.
            assert!(
                !strata.contains(&"any-letter"),
                "{id}: a contested reading is inside the gated stratum (strata: {strata:?})"
            );
            assert!(
                strata.contains(&"conditional-advisory"),
                "{id}: ungated and untagged is unmeasured (strata: {strata:?})"
            );
            for tag in ["conditional-ask", "conditional-done", "standing-advice"] {
                if strata.contains(&tag) {
                    *by_set
                        .entry(set.clone())
                        .or_default()
                        .entry(tag)
                        .or_default() += 1;
                }
            }
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "the bed plants no conditional or advisory letters"
    );

    // 60 decisions per construction per set. Sized to what it must be
    // able to do rather than to what looks tidy: 12 was enough to
    // *show* #552's failure and not enough to *measure* its fix, and a
    // 5% Wilson upper bound — the `no_obligation` ceiling this stratum
    // would be promoted into — needs 59 clean decisions before it can
    // say anything at all.
    const WANTED: usize = 60;
    let mut failures: Vec<String> = Vec::new();
    // `conditional-done` (#614, 3 September 2026): the first real
    // letter through the packaged app conditioned an ask on something
    // the reader may already have done — *"if you have recently paid
    // this invoice, please complete our form"* — and it was recorded
    // as a task at high confidence, on clean text as well as on the
    // OCR'd page. The bed's twenty conditionals held one of that shape
    // and the rule measured well on them; this stratum holds only that
    // shape, so the rate is readable on its own.
    for set in ["development", "exam"] {
        let counts = by_set.get(set).cloned().unwrap_or_default();
        for tag in ["conditional-ask", "conditional-done", "standing-advice"] {
            let seen = counts.get(tag).copied().unwrap_or(0);
            if seen < WANTED {
                failures.push(format!(
                    "{set}/{tag}: {seen} decisions, {WANTED} wanted — too few to bound the \
                     rate this stratum exists to measure"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n  "));
}

/// #399, 31 August 2026: the bed carries the construction the real
/// letter used. `gp_appointment-025-p1.jpg` through the packaged app:
/// OCR delivered *"This letter confirms your appointment with the
/// practice nurse on 9 March 2026 at 3.50pm"* verbatim and the model
/// answered no obligation at high confidence — the one dated ask in the
/// letter. The paired 30 August archive reads the same template 1 of 4
/// across text and photo. Every appointment this bed planted before
/// today said *"You have an appointment"* or *"We have booked"*; none
/// confirmed one.
///
/// Positive evidence in both voices — an appointment stated as a
/// confirmation, with its time, and no imperative anywhere in the
/// sentence that carries the deadline.
#[test]
fn letter_bed_carries_confirmation_phrased_appointments() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let mut by_set: BTreeMap<String, usize> = BTreeMap::new();
    for letter in runner::eval::letters::generate(&spec) {
        if !letter.stem.contains("-appointment_confirmed-") {
            continue;
        }
        let expected: serde_json::Value =
            serde_json::from_str(&letter.expected).expect("expectations are json");
        let set = expected["eval_set"].as_str().expect("a set").to_owned();
        for item in expected["obligations"].as_array().expect("obligations") {
            let Some(deadline) = item["expect"]["deadline"].as_str() else {
                continue;
            };
            let segment = item["segment"].as_str().expect("a segment");
            let lower = segment.to_lowercase();
            assert!(
                lower.contains("confirm"),
                "{}: the ask is not a confirmation: {segment:?}",
                letter.stem
            );
            assert!(
                !lower.contains("please") && !lower.contains("you must"),
                "{}: the ask sentence tells the reader to act, which is the shape \
                 the bed already had: {segment:?}",
                letter.stem
            );
            assert!(
                deadline.contains(" at ") && segment.contains(deadline),
                "{}: the deadline {deadline:?} must name the time and be copied from \
                 the passage {segment:?}",
                letter.stem
            );
            assert_eq!(item["expect"]["kind"], "attendance", "{}", letter.stem);
            *by_set.entry(set.clone()).or_default() += 1;
        }
    }
    for set in ["development", "exam"] {
        let n = by_set.get(set).copied().unwrap_or(0);
        assert!(
            n >= 12,
            "{set}: {n} confirmation-phrased appointments, 12 wanted — enough to see \
             the #399 miss again, not enough to claim a bounded rate"
        );
    }
}

/// A shape that spends its families on two layouts must spend them
/// evenly, in both voices (#399, 1 September 2026).
///
/// `appointment_preparation` splits its 60 families in half: the first
/// 30 give the manner line and the preparation ask as separate
/// passages, the second 30 join them into the one sentence the real
/// letter used. The halves are the comparison the shape exists to
/// make, so an uneven split does not merely lose decisions — it makes
/// the two numbers incomparable while both still print.
///
/// It went in uneven. The parameter `passages` calls `index` is really
/// the *seed*, which carries `Voice::seed_offset()` — five in the exam
/// voice — so `index >= 30` cut the exam set 35/25 and the development
/// set 30/30. Nothing failed: every fixture generated, every stratum
/// was declared, the byte-for-byte test passed on a bed that was wrong
/// in one voice only. A census caught it, and a census is not a
/// control.
#[test]
fn a_shape_plants_each_of_its_constructions_evenly() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    // Constructions that must be planted in equal number, per set.
    const PAIRED: [(&str, &str); 1] = [("preparation-ask", "compound-ask")];

    let mut counted: BTreeMap<(String, String), usize> = BTreeMap::new();
    for letter in &letters {
        let set = if letter.stem.starts_with("generated-exam-") {
            "exam"
        } else {
            "development"
        };
        for (_, strata) in strata_by_item(&letter.expected) {
            for stratum in strata {
                *counted.entry((set.to_owned(), stratum)).or_default() += 1;
            }
        }
    }

    for (left, right) in PAIRED {
        for set in ["development", "exam"] {
            let a = counted
                .get(&(set.to_owned(), left.to_owned()))
                .copied()
                .unwrap_or(0);
            let b = counted
                .get(&(set.to_owned(), right.to_owned()))
                .copied()
                .unwrap_or(0);
            assert!(a > 0, "{set}: {left} plants nothing");
            assert_eq!(
                a, b,
                "{set}: {left} plants {a} decisions and {right} plants {b}. \
                 These are the two halves of one comparison, so an uneven \
                 split makes both numbers unreadable. Check the layout \
                 arithmetic against `Voice::seed_offset` — the `index` \
                 `passages` receives is a seed, not a family position."
            );
        }
    }
}
