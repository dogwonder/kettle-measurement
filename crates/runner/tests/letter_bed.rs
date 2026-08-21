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
    // #465's two dateless-anchor letters and six controlled twins, and
    // #504's twenty-four invoices — hence 826.
    assert!(
        error.contains("20 decisions") && error.contains("of 826"),
        "the refusal must name the arithmetic, got: {error}"
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

/// #406: the invoice shape's reading is contested, so it must not gate.
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
/// Ungated is not unmeasured. Every item here still carries
/// `invoice-totals` and its passage tag, which is where the shape is
/// watched until `in-a-table` earns its promotion (#504).
#[test]
fn the_invoice_shape_is_measured_but_does_not_gate() {
    let spec = runner::eval::letters::committed_spec(&pack_dir()).expect("the committed spec");
    let letters = runner::eval::letters::generate(&spec);

    let mut invoices = 0;
    for letter in &letters {
        let invoice = letter.stem.contains("invoice_totals");
        for (id, strata) in strata_by_item(&letter.expected) {
            if invoice {
                invoices += 1;
                assert!(
                    !strata.contains(&"any-letter".to_owned()),
                    "{id}: a contested reading is inside the gated stratum (strata: {strata:?})"
                );
                assert!(
                    strata.contains(&"invoice-totals".to_owned()),
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
    assert!(invoices > 0, "the bed plants no invoices to check");
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

    // Both sets, every fixture, one dated obligation each.
    assert_eq!(checked, 24, "every committed invoice is checked");
}
