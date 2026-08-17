//! #350: the `policy-terms` model role and `builtin:term-diff` — the
//! runner arm a two-document comparison needs (#66).
//!
//! Two documents in, named values out of each, one diff across them.
//! The model reads values verbatim and cites the passage it read them
//! from; every comparison and every subtraction below is Rust's.

mod support;

use runner::packs::load_pack;
use runner::run::{run_pack_bound, Answers, Payload};
use runner::run_dir::NoLog;
use runner::terms::TermChange;
use rust_decimal::Decimal;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

/// Two wholly invented policy documents (CLAUDE.md) — no insurer, no
/// person, no policy number. Each states the same two named terms, and
/// both moved between the years.
const PREVIOUS: &str = "Your policy schedule for the year to 31 August 2026.\n\n\
Compulsory excess: £250 per claim.\n\nTotal annual premium: £480.00.";

const RENEWAL: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim.\n\nTotal annual premium: £612.50.";

/// The smallest comparison pack: two documents, one extraction step,
/// one diff. The declared input order is what says which year is which.
fn comparison_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-compare-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-renewal",
          "name": "Renewal test",
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
          "value_kinds": {
            "compulsory_excess": "money",
            "premium": "money",
            "cover_limit": "money"
          },
          "outputs": ["report.html"]
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
                    "term": { "enum": ["compulsory_excess", "premium", "cover_limit", "other"] },
                    "basis": { "enum": ["per_claim", "annual", "monthly", "per_policy"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/previous.txt", PREVIOUS);
    write("fixtures/renewal.txt", RENEWAL);
    dir
}

/// One segment's answer, in the shape the schema above constrains.
fn result(id: usize, segment: &str, terms: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "segment": segment,
        "confidence": "high",
        "terms": terms,
    })
}

fn segments(document: &str) -> Vec<&str> {
    document.split("\n\n").collect()
}

/// Both documents' segments in one batch, each named value quoted from
/// the passage it was read out of.
fn terms_answer() -> String {
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£250", "quote": "Compulsory excess: £250 per claim."
                }])),
                result(2, previous[2], serde_json::json!([{
                    "term": "premium", "basis": "annual",
                    "value": "£480.00", "quote": "Total annual premium: £480.00."
                }])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£500", "quote": "Compulsory excess: £500 per claim."
                }])),
                result(5, renewal[2], serde_json::json!([{
                    "term": "premium", "basis": "annual",
                    "value": "£612.50", "quote": "Total annual premium: £612.50."
                }])),
            ]
        })
        .to_string(),
    )
}

/// Run the pack against both fixtures, bound by role (#332).
fn run(dir: &Path, answer: String) -> runner::run::RunOutcome {
    let pack = load_pack(dir).expect("a comparison pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);
    run_pack_bound(
        &pack,
        &[
            ("previous", dir.join("fixtures/previous.txt")),
            ("renewal", dir.join("fixtures/renewal.txt")),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the run completes")
}

#[test]
fn a_policy_terms_step_and_a_term_diff_compare_two_documents() {
    let dir = comparison_pack("compares");

    let outcome = run(&dir, terms_answer());

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("a comparison pipeline produces the Comparison payload: {outcome:?}");
    };
    assert_eq!(comparison.terms.len(), 4, "{:?}", comparison.terms);

    // Which document a term came from is Rust's fact, never the
    // model's answer (#330): it follows from the segment.
    let excesses: Vec<&runner::terms::Term> = comparison
        .terms
        .iter()
        .filter(|t| t.term == "compulsory_excess")
        .collect();
    assert_eq!(excesses.len(), 2);
    assert_eq!(excesses[0].document, 0, "read out of last year's policy");
    assert_eq!(excesses[1].document, 1, "read out of the renewal");

    // The diff is ordered and both terms rose.
    let keys: Vec<&str> = comparison.diff.iter().map(|d| d.term.as_str()).collect();
    assert_eq!(keys, vec!["compulsory_excess", "premium"]);
    assert_eq!(
        comparison.diff[0].change,
        TermChange::Changed {
            from: "£250".to_owned(),
            to: "£500".to_owned(),
            delta: Some(Decimal::from(250)),
        },
    );
    // The premium's delta is exact money, computed in Rust.
    let TermChange::Changed { delta, .. } = &comparison.diff[1].change else {
        panic!("the premium changed: {:?}", comparison.diff[1]);
    };
    assert_eq!(*delta, Some("132.50".parse::<Decimal>().expect("decimal")));

    // Every row carries the passages behind it, so a claim about the
    // documents can be checked against them (#258).
    assert_eq!(comparison.diff[0].quotes.len(), 2);
    assert!(comparison.diff[0]
        .quotes
        .iter()
        .all(|quote| quote.text.contains("Compulsory excess")));

    assert!(
        outcome.needs_review.is_empty(),
        "{:?}",
        outcome.needs_review
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_quote_that_is_not_in_the_document_is_not_a_finding() {
    // The guardrail #258 names: a claim about a source document must
    // trace to a passage Rust verifies exists. A value whose quote is
    // not in the passage it was read from is an invention, and the
    // whole point is that invention is checkable. It goes to a person
    // rather than into the diff.
    let dir = comparison_pack("unverified-quote");
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£250", "quote": "Compulsory excess: £250 per claim."
                }])),
                result(2, previous[2], serde_json::json!([{
                    "term": "premium", "basis": "annual",
                    "value": "£480.00", "quote": "A protected no-claims discount is included."
                }])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£500", "quote": "Compulsory excess: £500 per claim."
                }])),
                result(5, renewal[2], serde_json::json!([{
                    "term": "premium", "basis": "annual",
                    "value": "£612.50", "quote": "Total annual premium: £612.50."
                }])),
            ]
        })
        .to_string(),
    );

    let outcome = run(&dir, answer);

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("a comparison pipeline produces the Comparison payload");
    };
    assert!(
        !comparison
            .diff
            .iter()
            .any(|d| matches!(d.change, TermChange::Changed { .. }) && d.term == "premium"),
        "an unverifiable value must not become a compared finding: {:?}",
        comparison.diff
    );
    assert_eq!(
        outcome.needs_review.len(),
        1,
        "it reaches a person instead: {:?}",
        outcome.needs_review
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #380, found on the first real comparison report (4 August 2026).
///
/// A `cover_limit` was read off a passage stating the policy period,
/// and the report paired "From <date> to <date> both days inclusive"
/// against a monetary figure from the other document. The quote was
/// real — the words are in the document — so #258's guardrail passed it
/// through, and `Changed { delta: None }` is indistinguishable from an
/// honest phrase change by the time it reaches the diff.
///
/// The pack says a cover limit is money. A value that cannot be money
/// is therefore not an answer to the question that was asked, whatever
/// passage it came from, and it goes to a person carrying the quote —
/// exactly as a term the pack does not model does.
#[test]
fn a_value_the_term_cannot_hold_is_not_a_finding() {
    let dir = comparison_pack("value-shape");
    // Both documents state a policy period beside the modelled terms:
    // the passage a cover limit was read off in the real run.
    let previous = format!(
        "{PREVIOUS}\n\nPeriod of insurance: From 1 September 2025 to 31 August 2026 both \
         days inclusive."
    );
    let renewal = format!(
        "{RENEWAL}\n\nPeriod of insurance: From 1 September 2026 to 31 August 2027 both \
         days inclusive."
    );
    std::fs::write(dir.join("fixtures/previous.txt"), &previous).expect("write previous");
    std::fs::write(dir.join("fixtures/renewal.txt"), &renewal).expect("write renewal");

    let previous_segments = segments(&previous);
    let renewal_segments = segments(&renewal);
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous_segments[0], serde_json::json!([])),
                result(1, previous_segments[1], serde_json::json!([])),
                result(2, previous_segments[2], serde_json::json!([])),
                // The misreading, with a quote Rust can find: the words
                // are on the page, they are simply not an amount.
                result(3, previous_segments[3], serde_json::json!([{
                    "term": "cover_limit", "basis": "per_policy",
                    "value": "From 1 September 2025 to 31 August 2026 both days inclusive",
                    "quote": "Period of insurance: From 1 September 2025 to 31 August 2026 both days inclusive."
                }])),
                result(4, renewal_segments[0], serde_json::json!([])),
                result(5, renewal_segments[1], serde_json::json!([])),
                // The other side is money, so without the guard these
                // two pair and the report asserts a cover limit moved.
                result(6, renewal_segments[2], serde_json::json!([{
                    "term": "cover_limit", "basis": "per_policy",
                    "value": "£612.50", "quote": "Total annual premium: £612.50."
                }])),
                result(7, renewal_segments[3], serde_json::json!([])),
            ]
        })
        .to_string(),
    );

    let outcome = run(&dir, answer);

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("a comparison pipeline produces the Comparison payload");
    };
    assert!(
        !comparison
            .diff
            .iter()
            .any(|d| d.term == "cover_limit" && matches!(d.change, TermChange::Changed { .. })),
        "a date range and an amount are not a change in the same value: {:?}",
        comparison.diff
    );
    // Not merely dropped: a value Kettle could not use is a passage a
    // person should see, and it arrives with the words behind it.
    let referred: Vec<&runner::run::ReviewItem> = outcome
        .needs_review
        .iter()
        .filter(|item| item.reason.contains("amount"))
        .collect();
    assert_eq!(
        referred.len(),
        1,
        "the passage reaches a person: {:?}",
        outcome.needs_review
    );
    assert!(
        referred[0].subject.contains("Period of insurance"),
        "carrying the passage it came from: {:?}",
        referred[0]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #377: a refusal to compare is not a failure to read.
///
/// Both reach a person, and the report shows them under one heading —
/// but they are different facts about a run, and the bed scores them
/// differently. `comparison_items` treats any passage found in
/// `needs_review` as surfaced rather than read, so routing the floor's
/// refusals through that list would score a model's *own* reading as a
/// referral.
///
/// The perverse case is what settles it. A model that invents a second
/// `premium` from a dateline triggers the repetition rule — and would
/// then have its invention scored as a referral rather than as an
/// invention, while the correct reading beside it stopped counting too.
/// Inventing more would improve the score. Found by
/// `eval_comparison_scoring` going red against the first version of
/// this change, which is the bed doing its job.
#[test]
fn a_refusal_to_compare_is_not_a_failure_to_read() {
    let dir = comparison_pack("refusal-not-review");
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    // The previous document states its excess twice — one section's
    // worth of the shape #378 put in the bed.
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£250", "quote": "Compulsory excess: £250 per claim."
                }])),
                result(2, previous[2], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£480.00", "quote": "Total annual premium: £480.00."
                }])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([{
                    "term": "compulsory_excess", "basis": "per_claim",
                    "value": "£500", "quote": "Compulsory excess: £500 per claim."
                }])),
                result(5, renewal[2], serde_json::json!([])),
            ]
        })
        .to_string(),
    );

    let outcome = run(&dir, answer);

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("a comparison pipeline produces the Comparison payload");
    };
    // Nothing was compared for that term…
    assert!(
        comparison.diff.is_empty(),
        "an arbitrary pairing is not a finding: {:?}",
        comparison.diff
    );
    assert_eq!(comparison.not_compared.len(), 1);
    assert_eq!(comparison.not_compared[0].readings, 2);

    // …and every reading is still a reading. The model answered for
    // these passages; the pipeline declined to pair them.
    assert_eq!(comparison.terms.len(), 3, "{:?}", comparison.terms);
    assert!(
        outcome.needs_review.is_empty(),
        "a pipeline refusal must not be recorded as the model failing to \
         answer: {:?}",
        outcome.needs_review
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_term_the_pack_does_not_model_goes_to_a_person() {
    // `other` is the model's honest place for a term it recognises and
    // the pack does not model. It never pairs and never reaches the
    // diff — it is a routing answer, so the passage goes to a person.
    let dir = comparison_pack("other-routes-to-review");
    let previous = segments(PREVIOUS);
    let renewal = segments(RENEWAL);
    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                result(0, previous[0], serde_json::json!([])),
                result(1, previous[1], serde_json::json!([{
                    "term": "other", "basis": "per_policy",
                    "value": "£250", "quote": "Compulsory excess: £250 per claim."
                }])),
                result(2, previous[2], serde_json::json!([])),
                result(3, renewal[0], serde_json::json!([])),
                result(4, renewal[1], serde_json::json!([{
                    "term": "other", "basis": "per_policy",
                    "value": "£500", "quote": "Compulsory excess: £500 per claim."
                }])),
                result(5, renewal[2], serde_json::json!([])),
            ]
        })
        .to_string(),
    );

    let outcome = run(&dir, answer);

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("a comparison pipeline produces the Comparison payload");
    };
    assert!(
        comparison.diff.is_empty(),
        "`other` never reaches the diff: {:?}",
        comparison.diff
    );
    assert_eq!(
        outcome.needs_review.len(),
        2,
        "both passages reach a person: {:?}",
        outcome.needs_review
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_model_no_terms_are_invented() {
    // The floor's honest arm, as the letter pack's is (#240): no model
    // means nothing was read, and every passage goes to a person rather
    // than a diff of terms nobody extracted.
    let dir = comparison_pack("no-model");
    let pack = load_pack(&dir).expect("pack loads");

    let outcome = run_pack_bound(
        &pack,
        &[
            ("previous", dir.join("fixtures/previous.txt")),
            ("renewal", dir.join("fixtures/renewal.txt")),
        ],
        &Answers::WithoutModel,
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the floor runs");

    let Payload::Comparison(comparison) = &outcome.payload else {
        panic!("the floor still produces the typology's payload");
    };
    assert!(comparison.terms.is_empty(), "{:?}", comparison.terms);
    assert!(comparison.diff.is_empty(), "{:?}", comparison.diff);
    assert_eq!(
        outcome.needs_review.len(),
        6,
        "every passage reaches a person"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// #373: the document preprocess serves both the letter typology and
/// this one, and it announced "Reading your letter" for both. A person
/// comparing two policy schedules is not reading a letter, and the
/// progress screen is where they would first notice the app is not
/// sure what it is doing.
#[test]
fn a_comparison_run_is_never_told_it_is_reading_a_letter() {
    let dir = comparison_pack("labels");
    let pack = load_pack(&dir).expect("a comparison pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", terms_answer())]);
    let mut labels: Vec<String> = Vec::new();
    run_pack_bound(
        &pack,
        &[
            ("previous", dir.join("fixtures/previous.txt")),
            ("renewal", dir.join("fixtures/renewal.txt")),
        ],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |progress| labels.push(progress.step.to_owned()),
        &NoLog,
    )
    .expect("the run completes");

    assert!(
        !labels.iter().any(|label| label.contains("letter")),
        "a comparison run says what it is really doing: {labels:?}"
    );
}
