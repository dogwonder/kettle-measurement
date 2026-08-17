//! #356: a comparison run is scored on its extraction.
//!
//! #350 shipped `Payload::Comparison` with two honest stubs — no scored
//! items, no step scores — because #351 had not yet decoupled the
//! payload. Both are closed here, and the pack can be measured rather
//! than asserted (#348).
//!
//! **The diff is deliberately not scored.** It is deterministic Rust
//! over the extracted terms (#350, `tests/terms.rs`), so a bed for it
//! would be scoring `rust_decimal` and a `BTreeMap` — and a bed that
//! cannot disagree measures nothing (`AUTHORING.md`). What the model is
//! on trial for is which named value a passage states, verbatim, with a
//! quote that exists.

mod support;

use runner::eval::fixture::{fixtures_in, FixtureEvaluator};
use runner::eval::{
    EvalMetric, Extracted, ExtractionOutcome, HarmClass, MachineInfo, MetricReport,
};
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::PathBuf;
use support::{completion_envelope, MockModel};

/// Two wholly invented policy schedules (CLAUDE.md). Each has a dateline
/// that states no modelled term — a first-class expectation, because it
/// is exactly what a keen extractor invents from. The previous year's
/// carries an arrangement fee: a real amount that is neither the premium
/// nor the excess, so an invention quoting it survives the quote
/// guardrail (#460 requires the quote to contain its value — a passage
/// with no amount at all can no longer source an assertion).
const PREVIOUS: &str = "Your policy schedule for the year to 31 August 2026, \
issued with a £25.00 arrangement fee.\n\n\
Compulsory excess: £250 per claim.\n\nTotal annual premium: £480.00.";

const RENEWAL: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.\n\nTotal annual premium: £612.50.";

fn segments(document: &str) -> Vec<&str> {
    document.split("\n\n").collect()
}

/// A comparison pack whose fixture states, per passage and per role,
/// the named value a correct run reads out of it.
fn comparison_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kettle-compare-score-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write pack file");
    };
    write(
        "pack.json",
        r#"{
          "id": "app.kttl.test-renewal-scoring",
          "name": "Renewal scoring test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [
            { "role": "previous", "label": "Last year's policy", "accept": ["text/plain"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["text/plain"], "multiple": false }
          ],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "policy-terms", "prompt": "prompts/terms.md", "schema": "schemas/terms.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:term-diff" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "value_kinds": { "compulsory_excess": "money", "total_excess": "money", "premium": "money" },
          "term_families": { "excess": ["compulsory_excess", "total_excess"] },
          "eval_metrics": ["extraction"],
          "outputs": ["report.html"],
          "eval_costs": {
            "review_rate": {
              "reason": "How many passages a person reads themselves. Surfacing uncertainty is a cost, never a wrong answer.",
              "date": "2026-08-03"
            }
          }
        }"#,
    );
    write(
        "prompts/terms.md",
        "Which named terms does each passage state?\n{{ batch_json }}\n",
    );
    write(
        "schemas/terms.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "terms": { "type": "array", "items": { "type": "object", "properties": {
                    "term": { "enum": ["compulsory_excess", "total_excess", "premium", "other"] },
                    "basis": { "enum": ["per_claim", "annual", "per_policy"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/renewal-01-previous.txt", PREVIOUS);
    write("fixtures/renewal-01-renewal.txt", RENEWAL);

    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    let expected = serde_json::json!({
        "fixture_id": "renewal-01",
        "eval_set": "development",
        "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "renewal-01-previous-dateline", "strata": ["states-nothing"],
              "role": "previous", "segment": previous[0], "expect": null },
            { "id": "renewal-01-previous-excess", "strata": ["named-value"],
              "role": "previous", "segment": previous[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250", "quote": "Compulsory excess: £250 per claim." } },
            { "id": "renewal-01-previous-premium", "strata": ["named-value"],
              "role": "previous", "segment": previous[2],
              "expect": { "term": "premium", "basis": "annual",
                          "value": "£480.00", "quote": "Total annual premium: £480.00." } },
            { "id": "renewal-01-renewal-dateline", "strata": ["states-nothing"],
              "role": "renewal", "segment": renewal[0], "expect": null },
            { "id": "renewal-01-renewal-excess", "strata": ["named-value"],
              "role": "renewal", "segment": renewal[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500", "quote": "Compulsory excess: £500 per claim." } },
            { "id": "renewal-01-renewal-premium", "strata": ["named-value"],
              "role": "renewal", "segment": renewal[2],
              "expect": { "term": "premium", "basis": "annual",
                          "value": "£612.50", "quote": "Total annual premium: £612.50." } }
        ]
    });
    write(
        "fixtures/renewal-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

fn result(id: usize, segment: &str, terms: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id, "segment": segment, "confidence": "high", "terms": terms,
    })
}

fn term(name: &str, basis: &str, value: &str, quote: &str) -> serde_json::Value {
    serde_json::json!({ "term": name, "basis": basis, "value": value, "quote": quote })
}

/// A perfect reading: every passage answered as the bed says.
fn perfect_answer() -> String {
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£500", "Compulsory excess: £500 per claim."
                )])),
                result(5, renewal[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )])),
            ]
        })
        .to_string(),
    )
}

/// One miss (the renewal's excess read as nothing) and one invention
/// (the previous year's arrangement fee asserted as the premium — the
/// quote is real and contains its value, so the guardrail cannot refuse
/// it; only the bed knows the passage states no modelled term).
fn one_miss_one_invention() -> String {
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([term(
                    "premium", "annual", "£25.00", "issued with a £25.00 arrangement fee"
                )])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([])),
                result(5, renewal[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )])),
            ]
        })
        .to_string(),
    )
}

fn evaluator(answer: String) -> (FixtureEvaluator, MockModel) {
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);
    (
        FixtureEvaluator {
            answers: Answers::FromModel(mock.endpoint()),
            model: None,
            machine: MachineInfo {
                cpu: "Apple M1 Pro".to_owned(),
                ram_gb: 16,
                os: "macOS 15.5".to_owned(),
            },
            sidecar: None,
            peak_rss: None,
            fixtures_dir: None,
            runs_dir: None,
            resume_dir: None,
        },
        mock,
    )
}

/// The test #356 names.
#[test]
fn a_comparison_fixture_scores_its_terms() {
    let dir = comparison_pack("scores-terms");
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(perfect_answer());

    let report = evaluator.evaluate(&pack).expect("the run scores");

    let fixture = &report.fixtures[0];
    assert_eq!(fixture.items.len(), 6, "one decision per passage");
    assert!(
        fixture
            .items
            .iter()
            .all(|item| item.decision.metric() == EvalMetric::Extraction),
        "a term decision is an extraction decision (#351)"
    );

    // A passage that states a value was read as stating it.
    let excess = fixture
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-renewal-excess")
        .expect("the renewal's excess is a scored item");
    let (expected, actual) = excess.decision.as_extraction().expect("an extraction");
    let Some(Extracted::Term(want)) = expected else {
        panic!("a comparison pack expects a term: {expected:?}");
    };
    assert_eq!(want.value, "£500");
    assert_eq!(
        actual,
        &ExtractionOutcome::Found {
            extracted: Extracted::Term(want.clone()),
        },
        "read exactly as the bed authored it"
    );
    assert!(
        !excess.trace_ids.is_empty(),
        "the scored value links back to its model claim lifecycle"
    );

    // A passage that states nothing, read as stating nothing, is a
    // scored decision rather than an absence.
    let dateline = fixture
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-previous-dateline")
        .expect("the dateline is a scored item");
    let (expected, actual) = dateline.decision.as_extraction().expect("an extraction");
    assert_eq!(expected, &None);
    assert_eq!(actual, &ExtractionOutcome::Absent);
    assert!(
        !dateline.trace_ids.is_empty(),
        "an empty answer still links to the parent model decision"
    );

    // The step the pack would set a bar for is named for the role that
    // answers it, and a perfect reading scores 1.0.
    assert_eq!(
        fixture.step_scores.get("policy-terms").map(|s| s.score),
        Some(1.0),
        "{:?}",
        fixture.step_scores
    );
    assert_eq!(fixture.end_to_end, 1.0);
    assert!(fixture.containment.candidates > 0);
    assert_eq!(fixture.containment.escaped, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The miss/invent lens, in the comparison pack's vocabulary. A value
/// the run failed to read is a miss; one read from a passage that
/// states none is an invention. Both are confident-wrong, in opposite
/// classes, exactly as they are for a letter.
#[test]
fn a_miss_and_an_invention_land_in_opposite_classes() {
    let dir = comparison_pack("miss-and-invention");
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(one_miss_one_invention());

    let report = evaluator.evaluate(&pack).expect("the run scores");
    let items: Vec<runner::eval::ScoredItem> = report
        .fixtures
        .iter()
        .flat_map(|fixture| fixture.items.iter().cloned())
        .collect();
    let metrics = runner::eval::extraction_metrics(&items);

    let carries = metrics
        .strata
        .get("named-value")
        .expect("the stratum of passages that state a value");
    assert_eq!(
        carries
            .harm_classes
            .get(&HarmClass::Obligation)
            .expect("the carries-a-value class")
            .confident_wrong_rate
            .successes,
        1,
        "the renewal's excess was missed: {carries:#?}"
    );

    let carries_none = metrics
        .strata
        .get("states-nothing")
        .expect("the stratum of passages that state none");
    assert_eq!(
        carries_none
            .harm_classes
            .get(&HarmClass::NoObligation)
            .expect("the carries-nothing class")
            .confident_wrong_rate
            .successes,
        1,
        "the arrangement fee was invented into a premium: {carries_none:#?}"
    );

    assert!(
        report.fixtures[0].end_to_end < 1.0,
        "a run that missed one and invented one is not perfect"
    );
    assert_eq!(
        report.fixtures[0].containment.escaped, 2,
        "both wrong assertions escaped the lifecycle into scored output"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Attribution is scored, not assumed — the document half.
///
/// An expectation about the renewal, pointed at a passage that is in
/// last year's policy, must not be satisfied by the term the run read
/// out of that passage. The reading is real and correct; it is about
/// the other year, and a renewal diff that accepts it turns a rise into
/// a cut. Authored this way round because it is the only way to put a
/// document mismatch in front of the scorer without two identically
/// worded passages, which a batch cannot tell apart to echo.
#[test]
fn a_term_from_the_other_document_does_not_satisfy_this_ones_expectation() {
    let dir = comparison_pack("wrong-document");
    // The renewal's own expectation, pointed at the previous year's
    // excess line. Everything else in the bed is untouched.
    let previous = segments(PREVIOUS);
    let raw = std::fs::read_to_string(dir.join("fixtures/renewal-01.expected.json"))
        .expect("read expectations");
    let mut expected: serde_json::Value = serde_json::from_str(&raw).expect("parse expectations");
    expected["policy-terms"][4]["segment"] = serde_json::Value::String(previous[1].to_owned());
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&expected).expect("serialise"),
    )
    .expect("rewrite expectations");

    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(perfect_answer());

    let report = evaluator.evaluate(&pack).expect("the run scores");
    let renewal_excess = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-renewal-excess")
        .expect("the renewal's excess");
    let (_, actual) = renewal_excess
        .decision
        .as_extraction()
        .expect("an extraction");

    assert_eq!(
        actual,
        &ExtractionOutcome::Absent,
        "a term read out of last year's document is not this year's answer"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The value half: the right passage, read as saying the wrong thing.
#[test]
fn last_years_number_in_this_years_passage_is_a_wrong_reading() {
    let dir = comparison_pack("attribution");
    let pack = load_pack(&dir).expect("the pack loads");
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    // Both excesses read as £250: the renewal's passage answered with
    // last year's number.
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £500 per claim."
                )])),
                result(5, renewal[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )])),
            ]
        })
        .to_string(),
    );
    let (evaluator, _mock) = evaluator(answer);

    let report = evaluator.evaluate(&pack).expect("the run scores");
    let fixture = &report.fixtures[0];
    let renewal_excess = fixture
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-renewal-excess")
        .expect("the renewal's excess");
    let (expected, actual) = renewal_excess
        .decision
        .as_extraction()
        .expect("an extraction");

    assert_ne!(
        Some(actual),
        expected
            .clone()
            .map(|want| ExtractionOutcome::Found { extracted: want })
            .as_ref(),
        "last year's number in this year's passage is a wrong reading"
    );
    assert!(fixture.end_to_end < 1.0);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A bed typo must not read as a model failure. An expectation naming a
/// role the pack never declared matches nothing, so every item under it
/// would be a miss — a table saying the model could not read a document
/// at all. It is refused at discovery instead, before a sidecar is up.
#[test]
fn a_term_expectation_naming_an_undeclared_role_is_refused() {
    let dir = comparison_pack("undeclared-role");
    let raw = std::fs::read_to_string(dir.join("fixtures/renewal-01.expected.json"))
        .expect("read expectations");
    let mut expected: serde_json::Value = serde_json::from_str(&raw).expect("parse");
    expected["policy-terms"][4]["role"] = serde_json::Value::String("this-year".to_owned());
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&expected).expect("serialise"),
    )
    .expect("rewrite expectations");
    let pack = load_pack(&dir).expect("the pack loads");

    let error = fixtures_in(&pack).expect_err("a role the pack has nothing called is refused");
    assert!(error.contains("this-year"), "{error}");
    assert!(
        error.contains("renewal-01-renewal-excess"),
        "the refusal names the expectation, so it can be found: {error}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The item record must say which prompt produced it. A comparison
/// pack's asking surface is its policy-terms prompt, and a bed that
/// recorded "not-applicable" would make every prompt edit unreviewable
/// — the one change CLAUDE.md says this project cannot review
/// unmeasured.
#[test]
fn a_scored_term_records_the_prompt_that_produced_it() {
    let dir = comparison_pack("prompt-version");
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(perfect_answer());

    let report = evaluator.evaluate(&pack).expect("the run scores");

    assert!(
        report.fixtures[0]
            .items
            .iter()
            .all(|item| item.prompt_version.starts_with("blake3:")),
        "{:?}",
        report.fixtures[0].items.first().map(|i| &i.prompt_version)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #445: a bed can expect a referral. The unmodelled_term shape exists
/// to verify Kettle surfaces what it does not model, and until now the
/// scorer could not reward exactly that: an expected `other` term was
/// structurally expected-but-review-routed, capping a perfect pooled
/// run at ~0.984. An item declaring `"review": true` scores a
/// review-routed reading as correct.
#[test]
fn an_expected_referral_scores_review_as_correct() {
    let dir = comparison_pack("expected-referral");
    // Rewrite one renewal expectation as an expected referral for a
    // term the pack does not model, and put the passage in the letter.
    let renewal = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.\n\nTotal annual premium: £612.50.\n\n\
Windscreen repair contribution: £75.00 per claim.";
    std::fs::write(dir.join("fixtures/renewal-01-renewal.txt"), renewal).expect("write");
    let renewal_segments: Vec<&str> = renewal.split("\n\n").collect();
    let expected = std::fs::read_to_string(dir.join("fixtures/renewal-01.expected.json"))
        .expect("expectations read");
    let mut parsed: serde_json::Value = serde_json::from_str(&expected).expect("valid json");
    parsed["policy-terms"]
        .as_array_mut()
        .expect("items")
        .push(serde_json::json!({
            "id": "renewal-01-renewal-windscreen",
            "strata": ["named-value"],
            "role": "renewal",
            "segment": renewal_segments[3],
            "review": true,
            "expect": { "term": "other", "basis": "per_claim",
                        "value": "£75.00",
                        "quote": "Windscreen repair contribution: £75.00 per claim." }
        }));
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&parsed).expect("serialises"),
    )
    .expect("write");

    // The model reads the windscreen line as `other`; the #380
    // guardrail routes it to a person, which is the win.
    let previous = segments(PREVIOUS);
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal_segments[0], serde_json::json!([])),
                result(4, renewal_segments[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£500", "Compulsory excess: £500 per claim."
                )])),
                result(5, renewal_segments[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )])),
                result(6, renewal_segments[3], serde_json::json!([term(
                    "other", "per_claim", "£75.00", "Windscreen repair contribution: £75.00 per claim."
                )]))
            ]
        })
        .to_string(),
    );
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(answer);
    let report = evaluator.evaluate(&pack).expect("the run scores");

    let fixture = &report.fixtures[0];
    assert_eq!(
        fixture.step_scores["policy-terms"].score, 1.0,
        "the referral is the correct outcome, not a shortfall: {:?}",
        fixture.step_scores
    );
    let windscreen = fixture
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-renewal-windscreen")
        .expect("the referral is a scored item");
    let (_, actual) = windscreen.decision.as_extraction().expect("an extraction");
    assert!(
        matches!(actual, ExtractionOutcome::NeedsReview { .. }),
        "the reading went to a person: {actual:?}"
    );

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let missed = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        missed.confident_wrong_rate.successes, 0,
        "an expected referral that referred is not a wrong answer: {missed:?}"
    );
}

/// The failing directions: swallowing the line, or asserting a value
/// for it, are both failures to refer — bounded by the ceiling on the
/// class that expected something.
#[test]
fn a_swallowed_referral_counts_confident_wrong() {
    let dir = comparison_pack("swallowed-referral");
    let renewal = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.\n\nTotal annual premium: £612.50.\n\n\
Windscreen repair contribution: £75.00 per claim.";
    std::fs::write(dir.join("fixtures/renewal-01-renewal.txt"), renewal).expect("write");
    let renewal_segments: Vec<&str> = renewal.split("\n\n").collect();
    let expected = std::fs::read_to_string(dir.join("fixtures/renewal-01.expected.json"))
        .expect("expectations read");
    let mut parsed: serde_json::Value = serde_json::from_str(&expected).expect("valid json");
    parsed["policy-terms"]
        .as_array_mut()
        .expect("items")
        .push(serde_json::json!({
            "id": "renewal-01-renewal-windscreen",
            "strata": ["named-value"],
            "role": "renewal",
            "segment": renewal_segments[3],
            "review": true,
            "expect": { "term": "other", "basis": "per_claim",
                        "value": "£75.00",
                        "quote": "Windscreen repair contribution: £75.00 per claim." }
        }));
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&parsed).expect("serialises"),
    )
    .expect("write");

    // The model reads nothing at all from the windscreen line.
    let previous = segments(PREVIOUS);
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal_segments[0], serde_json::json!([])),
                result(4, renewal_segments[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£500", "Compulsory excess: £500 per claim."
                )])),
                result(5, renewal_segments[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )])),
                result(6, renewal_segments[3], serde_json::json!([]))
            ]
        })
        .to_string(),
    );
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(answer);
    let report = evaluator.evaluate(&pack).expect("the run scores");

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let missed = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        missed.confident_wrong_rate.successes, 1,
        "a swallowed referral is a confident wrong answer: {missed:?}"
    );
}

/// The test #461's read side names. When the two documents label the
/// same kind of value differently, `diff_terms` refuses the family and
/// every reading goes to a person — Rust's act, recorded in
/// `not_compared`, deliberately apart from `needs_review` (what the
/// model could not answer). A bed authoring that referral as the
/// correct outcome must be able to score it: the readings are still
/// read (#378 — the bed scores reading, not pairing), and the referral
/// expectations score review-as-correct rather than as a swallowed
/// referral. The join is structural — `(term, basis)` from
/// `not_compared` to the segments the run's own terms carry — never
/// re-derived from quote text (#457).
#[test]
fn a_rust_derived_referral_scores_review_as_correct() {
    let dir = comparison_pack("rust-derived-referral");
    // This year's schedule names its excess a total excess where last
    // year's said compulsory: disjoint labels in one family, so the
    // mechanism refuses the comparison and refers both readings.
    let renewal = "Your renewal schedule for the year to 31 August 2027.\n\n\
Total excess payable: £300.00 per claim.\n\nTotal annual premium: £612.50.";
    std::fs::write(dir.join("fixtures/renewal-01-renewal.txt"), renewal).expect("write");
    let renewal_segments: Vec<&str> = renewal.split("\n\n").collect();
    let previous = segments(PREVIOUS);

    let expected = std::fs::read_to_string(dir.join("fixtures/renewal-01.expected.json"))
        .expect("expectations read");
    let mut parsed: serde_json::Value = serde_json::from_str(&expected).expect("valid json");
    let items = parsed["policy-terms"].as_array_mut().expect("items");
    // The renewal's excess reading, as this document actually words it.
    items[4] = serde_json::json!({
        "id": "renewal-01-renewal-excess", "strata": ["named-value"],
        "role": "renewal", "segment": renewal_segments[1],
        "expect": { "term": "total_excess", "basis": "per_claim",
                    "value": "£300.00",
                    "quote": "Total excess payable: £300.00 per claim." }
    });
    // The referral is also expected, on both passages: the correct run
    // reads each line and then declines to compare them.
    items.push(serde_json::json!({
        "id": "renewal-01-previous-excess-referred", "strata": ["named-value"],
        "role": "previous", "segment": previous[1],
        "review": true,
        "expect": { "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£250",
                    "quote": "Compulsory excess: £250 per claim." }
    }));
    items.push(serde_json::json!({
        "id": "renewal-01-renewal-excess-referred", "strata": ["named-value"],
        "role": "renewal", "segment": renewal_segments[1],
        "review": true,
        "expect": { "term": "total_excess", "basis": "per_claim",
                    "value": "£300.00",
                    "quote": "Total excess payable: £300.00 per claim." }
    }));
    std::fs::write(
        dir.join("fixtures/renewal-01.expected.json"),
        serde_json::to_string_pretty(&parsed).expect("serialises"),
    )
    .expect("write");

    // The model reads every line correctly, in each document's own
    // vocabulary. Nothing here is the model's mistake.
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([term(
                    "compulsory_excess", "per_claim", "£250", "Compulsory excess: £250 per claim."
                )])),
                result(2, previous[2], serde_json::json!([term(
                    "premium", "annual", "£480.00", "Total annual premium: £480.00."
                )])),
                result(3, renewal_segments[0], serde_json::json!([])),
                result(4, renewal_segments[1], serde_json::json!([term(
                    "total_excess", "per_claim", "£300.00", "Total excess payable: £300.00 per claim."
                )])),
                result(5, renewal_segments[2], serde_json::json!([term(
                    "premium", "annual", "£612.50", "Total annual premium: £612.50."
                )]))
            ]
        })
        .to_string(),
    );
    let pack = load_pack(&dir).expect("the pack loads");
    let (evaluator, _mock) = evaluator(answer);
    let report = evaluator.evaluate(&pack).expect("the run scores");

    let fixture = &report.fixtures[0];
    assert_eq!(
        fixture.step_scores["policy-terms"].score, 1.0,
        "a Rust-derived referral the bed expected is the correct outcome, \
         not a shortfall: {:?}",
        fixture.step_scores
    );

    let referred = fixture
        .items
        .iter()
        .find(|item| item.item_id == "renewal-01-renewal-excess-referred")
        .expect("the referral expectation is a scored item");
    let (_, actual) = referred.decision.as_extraction().expect("an extraction");
    assert!(
        matches!(actual, ExtractionOutcome::NeedsReview { .. }),
        "the refused reading surfaced to a person: {actual:?}"
    );

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let missed = &metrics.overall.harm_classes[&HarmClass::Obligation];
    assert_eq!(
        missed.confident_wrong_rate.successes, 0,
        "a referral that referred is not a wrong answer: {missed:?}"
    );
}
