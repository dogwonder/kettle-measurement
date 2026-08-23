//! #430: score whether evidence supports a claim, not only whether the
//! quote exists.
//!
//! Rust refusing a quote it cannot find proves source fidelity, not
//! semantic support. A verbatim passage can still negate the claim,
//! belong to the wrong document, or omit the qualifier that changes its
//! meaning — and each of those must be a distinct recorded outcome, not
//! a bare miss. Ground truth here is human-authored fixture data; no
//! model judgement enters a baseline.

mod support;

use runner::eval::evidence::{DimensionOutcome, EvidenceDimension};
use runner::eval::fixture::{model_info, FixtureEvaluator};
use runner::eval::MachineInfo;
use runner::packs::load_pack;
use runner::run::Answers;
use std::path::{Path, PathBuf};
use support::{completion_envelope, MockModel};

/// A letter whose middle passage says, in so many words, that nothing
/// is owed. The exact words a keen extractor would quote back as
/// evidence for a payment obligation. Wholly invented.
const LETTER: &str = "3 March 2026\n\nYour account with Harborne Parking \
Services is settled and you do not need to pay anything at this \
time.\n\nWe are sorry for any inconvenience and thank you for your \
co-operation.";

fn machine() -> MachineInfo {
    MachineInfo {
        cpu: "Apple M1 Pro".to_owned(),
        ram_gb: 16,
        os: "macOS 15.5".to_owned(),
    }
}

/// A letter pack that declares the evidence dimensions it can score,
/// with a fixture authoring why a payment claim on the settled passage
/// would be unsupported.
fn letter_pack(name: &str) -> PathBuf {
    letter_pack_with(name, LETTER, SETTLED_EXPECTED)
}

/// The settled-account fixture's expectations, authored beside it.
const SETTLED_EXPECTED: &str = r#"{
  "fixture_id": "settled-account-01",
  "eval_set": "development",
  "obligations": [
    {
      "id": "settled-account-no-payment-01",
      "strata": [],
      "segment": "Your account with Harborne Parking Services is settled and you do not need to pay anything at this time.",
      "expect": null,
      "evidence": {
        "unsupported": [
          { "claim": { "kind": "payment" }, "why": "the passage states no payment is needed" }
        ]
      }
    }
  ]
}"#;

/// The same pack over a different letter and its expectations.
fn letter_pack_with(name: &str, letter: &str, expected: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-evidence-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-evidence",
          "name": "Evidence test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "multiple": false }],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:timeline-sort" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Tracks how many passages a person reads.", "date": "2026-08-07" } },
          "eval_evidence": {
            "existence": { "reason": "The quoted words must be in the letter.", "date": "2026-08-07" },
            "attribution": { "reason": "The words must come from the passage the claim is about.", "date": "2026-08-07" },
            "support": { "reason": "A verbatim passage can still negate the claim it is offered for.", "date": "2026-08-07" }
          },
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "enum": ["payment", "response", "attendance", "other"] },
                    "party": { "type": "string" },
                    "ask": { "type": "string" },
                    "deadline": { "type": "string" },
                    "anchor": { "type": "string" }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/letter-01.txt", letter);
    write("fixtures/letter-01.expected.json", expected);
    dir
}

/// A letter whose one ask is relative to the letter's own date. Wholly
/// invented.
const RELATIVE_LETTER: &str = "3 March 2026\n\nPlease pay the balance of £40.00 on your \
account with Harborne Parking Services within 28 days.\n\nThank you for \
your co-operation.";

/// The bed's authored reading of it: counted from the date of the
/// letter, due on 31 March.
const RELATIVE_EXPECTED: &str = r#"{
  "fixture_id": "relative-payment-01",
  "eval_set": "development",
  "obligations": [
    {
      "id": "relative-payment-01",
      "strata": [],
      "segment": "Please pay the balance of £40.00 on your account with Harborne Parking Services within 28 days.",
      "expect": {
        "kind": "payment",
        "party": "Harborne Parking Services",
        "deadline": "within 28 days",
        "anchor": "the date of this letter",
        "due": "2026-03-31"
      }
    }
  ]
}"#;

/// The model reads the ask correctly and names the anchor with the
/// deadline's own words — the form the 21 August v14 runs returned
/// 78 times per exam run and 79 per development run.
fn answer_naming_the_anchor_by_the_deadline() -> String {
    let segments: Vec<&str> = RELATIVE_LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay the balance of £40.00",
                        "deadline": "within 28 days",
                        "anchor": "within 28 days"
                    }]
                },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

/// #552's standing defect: `support` compared the anchor verbatim while
/// the harm lens compared it by the date it names (#452) and the pooled
/// join ignored it (#287). On the 21 August v14 letter runs that was
/// 78 support failures per exam run and 79 per development run, every
/// one on `anchor` alone with the right date — the `month-end` stratum
/// read 78 expected, 78 found, 0 support, and reported recall 1.00,
/// because nothing consumed the dimension. Three comparisons of one
/// answer must give one verdict.
#[test]
fn an_anchor_worded_as_the_deadline_supports_the_claim_it_resolves_with() {
    let dir = letter_pack_with("anchor-wording", RELATIVE_LETTER, RELATIVE_EXPECTED);
    let report = evaluate(&dir, answer_naming_the_anchor_by_the_deadline());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "relative-payment-01")
        .expect("the relative payment is a scored item");

    assert_eq!(
        item.evidence.get(&EvidenceDimension::Existence),
        Some(&DimensionOutcome::Pass),
        "the quoted passage is in the letter: {:?}",
        item.evidence
    );
    assert_eq!(
        item.evidence.get(&EvidenceDimension::Support),
        Some(&DimensionOutcome::Pass),
        "the same ask, on the same party, by the same day, is the supported claim \
         whatever words the anchor wears: {:?}",
        item.evidence
    );
}

/// The re-authored exam bed of 21 August (#552): the model recorded the
/// obligation every time and copied the letter's own words for the
/// deadline, date included — "within 45 days of 23 August 2026" where
/// the bed had split "within 45 days" from its anchor. `obligation_key`
/// found all 36; `same_assertion_as` demoted 12 of them to
/// confident-wrong on the wording. Both are faithful copies, both
/// resolve to 7 October, and the date is what the person acts on.
#[test]
fn a_deadline_carrying_its_own_anchor_is_the_same_assertion() {
    use chrono::NaiveDate;
    use runner::eval::{ExpectedObligation, Extracted};

    let obligation = |deadline: &str, anchor: &str, due: Option<&str>| {
        Extracted::Obligation(ExpectedObligation {
            kind: "payment".to_owned(),
            party: "Elmswood Lettings".to_owned(),
            deadline: deadline.to_owned(),
            anchor: anchor.to_owned(),
            due: due.map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").expect("a date")),
        })
    };

    let split = obligation("within 45 days", "23 August 2026", Some("2026-10-07"));
    let whole = obligation("within 45 days of 23 August 2026", "", Some("2026-10-07"));
    assert!(
        split.same_assertion_as(&whole),
        "two faithful copies of one deadline resolve to one day"
    );

    let off_by_one = obligation("within 46 days of 23 August 2026", "", Some("2026-10-08"));
    assert!(
        !split.same_assertion_as(&off_by_one),
        "a different day is a different assertion, however close the words"
    );

    // The day is right and the route is not: the letter never wrote
    // 7 October, the model counted to it, and the prompt forbids that.
    // Keyed on the day alone this scored clean on every comparator and
    // the report would have marked a computed date as read from the
    // page (#554 review).
    let computed = obligation("by 7 October 2026", "", Some("2026-10-07"));
    assert!(
        !split.same_assertion_as(&computed),
        "a date the model worked out is not the deadline the letter wrote"
    );

    // #544: the passage points at a table row and the row carries the
    // date. A reading that fuses the row's date into the prose claim
    // resolves to the same day by a route the passage does not support.
    let pointing = obligation("the date shown beside it", "", Some("2026-03-06"));
    let fused = obligation("by 6 March 2026", "", Some("2026-03-06"));
    assert!(
        !pointing.same_assertion_as(&fused),
        "a pointer and the date it points at are different readings of the passage"
    );

    let soon = obligation("at your earliest convenience", "", None);
    let when_you_can = obligation("when you can", "", None);
    assert!(
        !soon.same_assertion_as(&when_you_can),
        "an undated ask is shown in the letter's own words, so the words are the claim"
    );
    assert!(
        soon.same_assertion_as(&obligation("At your earliest convenience", "", None)),
        "as the pooled join reads them, case apart"
    );
}

/// The model reads a payment obligation into the passage that says the
/// account is settled, quoting the settled passage itself as evidence.
fn answer_asserting_payment_from_the_negation() -> String {
    let segments: Vec<&str> = LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                {
                    "id": 1,
                    "segment": segments[1],
                    "confidence": "high",
                    "obligations": [{
                        "kind": "payment",
                        "party": "Harborne Parking Services",
                        "ask": "Pay your account",
                        "deadline": "at this time",
                        "anchor": "the date of this letter"
                    }]
                },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

fn evaluate(dir: &Path, answer: String) -> runner::eval::EvalReport {
    let pack = load_pack(dir).expect("the evidence pack loads");
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);
    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(mock.endpoint()),
        model: Some(model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192)),
        machine: machine(),
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: None,
    };
    evaluator.evaluate(&pack).expect("the eval runs")
}

/// The issue's first test. The passage exists word for word and is the
/// very passage the claim was asked about — and it says the opposite of
/// what the claim asserts. Existence and attribution pass; support
/// fails; and the three are three recorded outcomes, not one score.
#[test]
fn a_verbatim_quote_that_negates_the_claim_passes_existence_and_fails_support() {
    let dir = letter_pack("negation");
    let report = evaluate(&dir, answer_asserting_payment_from_the_negation());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "settled-account-no-payment-01")
        .expect("the settled passage is a scored item");

    assert_eq!(
        item.evidence.get(&EvidenceDimension::Existence),
        Some(&DimensionOutcome::Pass),
        "the quoted words are in the letter: {:?}",
        item.evidence
    );
    assert_eq!(
        item.evidence.get(&EvidenceDimension::Attribution),
        Some(&DimensionOutcome::Pass),
        "the words come from the passage the claim is about: {:?}",
        item.evidence
    );
    let support = item
        .evidence
        .get(&EvidenceDimension::Support)
        .expect("support is a recorded outcome");
    let DimensionOutcome::Fail { reason } = support else {
        panic!("a claim quoting its own negation is unsupported: {support:?}");
    };
    assert!(
        reason.contains("no payment is needed"),
        "the failure carries the authored why: {reason}"
    );
}

/// Two wholly invented schedules whose excess passage states two
/// values — the compulsory and the voluntary excess side by side, which
/// is how real schedules print them.
const PREVIOUS: &str = "Your policy schedule for the year to 31 August 2026.\n\n\
Compulsory excess: £250 per claim. Voluntary excess: £100 per claim.";

const RENEWAL: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Compulsory excess: £500 per claim. Voluntary excess: £100 per claim.";

/// A comparison pack declaring the same three dimensions, whose excess
/// expectations author why the voluntary figure would be unsupported.
fn comparison_pack(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kettle-evidence-cmp-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-evidence-renewal",
          "name": "Renewal evidence test",
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
          "value_kinds": { "compulsory_excess": "money", "premium": "money" },
          "eval_costs": { "review_rate": { "reason": "Surfacing uncertainty is a cost, never a wrong answer.", "date": "2026-08-07" } },
          "eval_evidence": {
            "existence": { "reason": "The quoted words must be in a schedule.", "date": "2026-08-07" },
            "attribution": { "reason": "The words must come from the document the claim is about.", "date": "2026-08-07" },
            "support": { "reason": "A passage stating two values supports only one of them per term.", "date": "2026-08-07" }
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
                    "term": { "enum": ["compulsory_excess", "premium", "other"] },
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
    let previous: Vec<&str> = PREVIOUS.split("\n\n").collect();
    let renewal: Vec<&str> = RENEWAL.split("\n\n").collect();
    let expected = serde_json::json!({
        "fixture_id": "evidence-renewal-01",
        "eval_set": "development",
        "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "evidence-prev-dateline", "strata": [],
              "role": "previous", "segment": previous[0], "expect": null },
            { "id": "evidence-prev-excess", "strata": [],
              "role": "previous", "segment": previous[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£250",
                          "quote": "Compulsory excess: £250 per claim." },
              "evidence": {
                "unsupported": [
                  { "claim": { "term": "compulsory_excess", "value": "£100" },
                    "why": "£100 is the voluntary excess, not the compulsory one" }
                ]
              } },
            { "id": "evidence-renewal-dateline", "strata": [],
              "role": "renewal", "segment": renewal[0], "expect": null },
            { "id": "evidence-renewal-excess", "strata": [],
              "role": "renewal", "segment": renewal[1],
              "expect": { "term": "compulsory_excess", "basis": "per_claim",
                          "value": "£500",
                          "quote": "Compulsory excess: £500 per claim." },
              "evidence": {
                "unsupported": [
                  { "claim": { "term": "compulsory_excess", "value": "£100" },
                    "why": "£100 is the voluntary excess, not the compulsory one" }
                ]
              } }
        ]
    });
    write(
        "fixtures/renewal-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

/// The model reads the two-value passages and picks the wrong figure
/// each time, quoting the whole passage — so the quote is verbatim, in
/// the right document, and contains the value it asserts. Every check
/// short of support passes.
fn answer_picking_the_voluntary_excess() -> String {
    let previous: Vec<&str> = PREVIOUS.split("\n\n").collect();
    let renewal: Vec<&str> = RENEWAL.split("\n\n").collect();
    let wrong = |segment: &str| {
        serde_json::json!([{
            "term": "compulsory_excess", "basis": "per_claim",
            "value": "£100", "quote": segment
        }])
    };
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": previous[0], "confidence": "high", "terms": [] },
                { "id": 1, "segment": previous[1], "confidence": "high", "terms": wrong(previous[1]) },
                { "id": 2, "segment": renewal[0], "confidence": "high", "terms": [] },
                { "id": 3, "segment": renewal[1], "confidence": "high", "terms": wrong(renewal[1]) }
            ]
        })
        .to_string(),
    )
}

/// Issue #430's fourth companion case: two values in one passage. The
/// wrong pick is verbatim-quoted, correctly attributed, and its value
/// really is inside the quote — only support can say it is wrong, and
/// it must say why in the author's words.
#[test]
fn two_values_in_one_passage_scored_against_the_authored_supported_set() {
    let dir = comparison_pack("two-values");
    let report = evaluate(&dir, answer_picking_the_voluntary_excess());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "evidence-prev-excess")
        .expect("the previous excess is a scored item");

    assert_eq!(
        item.evidence.get(&EvidenceDimension::Existence),
        Some(&DimensionOutcome::Pass),
        "the quoted passage is in the schedule: {:?}",
        item.evidence
    );
    assert_eq!(
        item.evidence.get(&EvidenceDimension::Attribution),
        Some(&DimensionOutcome::Pass),
        "the quote comes from the right document and passage: {:?}",
        item.evidence
    );
    let support = item
        .evidence
        .get(&EvidenceDimension::Support)
        .expect("support is a recorded outcome");
    let DimensionOutcome::Fail { reason } = support else {
        panic!("the voluntary figure is not the compulsory excess: {support:?}");
    };
    assert!(
        reason.contains("voluntary excess"),
        "the failure carries the authored why: {reason}"
    );
}

/// A schedule whose premium passage carries a qualifier that changes
/// what the figure means. Wholly invented.
const QUALIFIED: &str = "Your renewal schedule for the year to 31 August 2027.\n\n\
Total annual premium: £612.50 paid as monthly instalments.";

fn qualified_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kettle-evidence-qual-{}-{name}",
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
          "id": "app.kttl.test-evidence-qualifier",
          "name": "Qualifier evidence test",
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
          "value_kinds": { "premium": "money" },
          "eval_costs": { "review_rate": { "reason": "Surfacing uncertainty is a cost, never a wrong answer.", "date": "2026-08-07" } },
          "eval_evidence": {
            "support": { "reason": "The figure read must be the figure the passage states.", "date": "2026-08-07" },
            "completeness": { "reason": "An annual figure without its instalment basis misprices the year.", "date": "2026-08-07" }
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
                    "term": { "enum": ["premium", "other"] },
                    "basis": { "enum": ["annual", "per_policy"] },
                    "value": { "type": "string" },
                    "quote": { "type": "string" }
                }, "required": ["term", "basis", "value", "quote"] } }
            }, "required": ["id", "segment", "confidence", "terms"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/renewal-01-previous.txt", "Nothing stated here.");
    write("fixtures/renewal-01-renewal.txt", QUALIFIED);
    let renewal: Vec<&str> = QUALIFIED.split("\n\n").collect();
    let expected = serde_json::json!({
        "fixture_id": "evidence-qualifier-01",
        "eval_set": "development",
        "inputs": {
            "previous": "renewal-01-previous.txt",
            "renewal": "renewal-01-renewal.txt"
        },
        "policy-terms": [
            { "id": "qualifier-premium", "strata": [],
              "role": "renewal", "segment": renewal[1],
              "expect": { "term": "premium", "basis": "annual",
                          "value": "£612.50",
                          "quote": "Total annual premium: £612.50 paid as monthly instalments." },
              "evidence": {
                "qualifiers": [
                  { "text": "paid as monthly instalments",
                    "why": "an annual figure without its instalment basis misprices the year" }
                ]
              } }
        ]
    });
    write(
        "fixtures/renewal-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

/// The model reads the right figure but quotes it shorn of the
/// qualifier — the claim is true and its evidence is misleading.
fn answer_omitting_the_qualifier() -> String {
    let renewal: Vec<&str> = QUALIFIED.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": "Nothing stated here.", "confidence": "high", "terms": [] },
                { "id": 1, "segment": renewal[0], "confidence": "high", "terms": [] },
                { "id": 2, "segment": renewal[1], "confidence": "high", "terms": [{
                    "term": "premium", "basis": "annual",
                    "value": "£612.50", "quote": "Total annual premium: £612.50"
                }] }
            ]
        })
        .to_string(),
    )
}

/// Issue #430's third companion case: the value matches and the quote
/// is verbatim, but it stops before the words that change what the
/// figure means. Support passes; completeness fails with the authored
/// why.
#[test]
fn a_matching_value_that_omits_a_material_qualifier_fails_completeness() {
    let dir = qualified_pack("omitted");
    let report = evaluate(&dir, answer_omitting_the_qualifier());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "qualifier-premium")
        .expect("the premium is a scored item");

    assert_eq!(
        item.evidence.get(&EvidenceDimension::Support),
        Some(&DimensionOutcome::Pass),
        "the figure itself is the one the passage states: {:?}",
        item.evidence
    );
    let completeness = item
        .evidence
        .get(&EvidenceDimension::Completeness)
        .expect("completeness is a recorded outcome");
    let DimensionOutcome::Fail { reason } = completeness else {
        panic!("a quote shorn of its qualifier is incomplete: {completeness:?}");
    };
    assert!(
        reason.contains("instalment"),
        "the failure carries the authored why: {reason}"
    );
}

/// A letter with one relative deadline, for the derivation dimension:
/// the due date Kettle computes must equal the authored operation
/// applied to the authored source date — two independent authorings
/// that can disagree, which is what makes the check a measurement.
const DATED_LETTER: &str = "3 March 2026\n\nPlease pay £120.00 to Harborne \
Parking Services within 14 days of the date of this letter.\n\nThank you \
for your co-operation.";

fn derivation_pack(name: &str, days: u32) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kettle-evidence-der-{}-{name}", std::process::id()));
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
          "id": "app.kttl.test-evidence-derivation",
          "name": "Derivation evidence test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "multiple": false }],
          "capabilities": ["read"],
          "model": { "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 },
          "copy": { "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }, "will": [], "run_verb": "Run this task" },
          "pipeline": [
            { "step": "preprocess", "impl": "builtin:document-text" },
            { "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 },
            { "step": "aggregate", "impl": "builtin:timeline-sort" },
            { "step": "render", "template": "report.html.tera" }
          ],
          "eval_metrics": ["extraction"],
          "eval_costs": { "review_rate": { "reason": "Tracks how many passages a person reads.", "date": "2026-08-07" } },
          "eval_evidence": {
            "derivation": { "reason": "A resolved date is an arithmetic claim, checkable in Rust.", "date": "2026-08-07" }
          },
          "outputs": ["report.html"]
        }"#,
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "enum": ["payment", "response", "attendance", "other"] },
                    "party": { "type": "string" },
                    "ask": { "type": "string" },
                    "deadline": { "type": "string" },
                    "anchor": { "type": "string" }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/letter-01.txt", DATED_LETTER);
    let expected = serde_json::json!({
        "fixture_id": "derivation-letter-01",
        "eval_set": "development",
        "obligations": [
            {
                "id": "derivation-payment-01",
                "strata": [],
                "segment": "Please pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.",
                "expect": {
                    "kind": "payment",
                    "party": "Harborne Parking Services",
                    "deadline": "within 14 days",
                    "anchor": "the date of this letter",
                    "due": "2026-03-17"
                },
                "evidence": {
                    "derivation": { "from": "2026-03-03", "op": "add_days", "days": days }
                }
            }
        ]
    });
    write(
        "fixtures/letter-01.expected.json",
        &serde_json::to_string_pretty(&expected).expect("expectations serialise"),
    );
    dir
}

fn answer_reading_the_payment() -> String {
    let segments: Vec<&str> = DATED_LETTER.split("\n\n").collect();
    completion_envelope(
        &serde_json::json!({
            "results": [
                { "id": 0, "segment": segments[0], "confidence": "high", "obligations": [] },
                { "id": 1, "segment": segments[1], "confidence": "high", "obligations": [{
                    "kind": "payment",
                    "party": "Harborne Parking Services",
                    "ask": "Pay £120.00",
                    "deadline": "within 14 days",
                    "anchor": "the date of this letter"
                }] },
                { "id": 2, "segment": segments[2], "confidence": "high", "obligations": [] }
            ]
        })
        .to_string(),
    )
}

/// Issue #430's fifth companion case, the passing half: the resolved
/// due date equals the authored operation applied to the authored
/// source — source value and deterministic operation, linked.
#[test]
fn a_derived_date_links_source_value_and_operation() {
    let dir = derivation_pack("agrees", 14);
    let report = evaluate(&dir, answer_reading_the_payment());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "derivation-payment-01")
        .expect("the payment is a scored item");

    assert_eq!(
        item.evidence.get(&EvidenceDimension::Derivation),
        Some(&DimensionOutcome::Pass),
        "3 March plus 14 days is 17 March: {:?}",
        item.evidence
    );
}

/// The failing half: the authored operation disagrees with the claim's
/// resolved date. Whichever side is wrong, the disagreement is recorded
/// and names both dates — the harness must never quietly prefer one.
#[test]
fn a_derived_date_that_disagrees_with_the_operation_fails_derivation() {
    let dir = derivation_pack("disagrees", 10);
    let report = evaluate(&dir, answer_reading_the_payment());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "derivation-payment-01")
        .expect("the payment is a scored item");

    let derivation = item
        .evidence
        .get(&EvidenceDimension::Derivation)
        .expect("derivation is a recorded outcome");
    let DimensionOutcome::Fail { reason } = derivation else {
        panic!("13 March is not the claim's 17 March: {derivation:?}");
    };
    assert!(
        reason.contains("2026-03-13") && reason.contains("2026-03-17"),
        "the failure names both dates: {reason}"
    );
}

/// Acceptance criterion: reports state which evidence dimensions were
/// and were not measured. A reader of a baseline must not have to
/// guess whether localisation was clean or simply never asked.
#[test]
fn the_report_says_which_dimensions_were_and_were_not_measured() {
    let dir = letter_pack("coverage");
    let report = evaluate(&dir, answer_asserting_payment_from_the_negation());

    let coverage = report
        .evidence
        .as_ref()
        .expect("a pack declaring dimensions produces evidence coverage");
    assert_eq!(
        coverage.measured,
        vec![
            EvidenceDimension::Existence,
            EvidenceDimension::Attribution,
            EvidenceDimension::Support
        ]
    );
    assert_eq!(
        coverage.not_measured,
        vec![
            EvidenceDimension::Completeness,
            EvidenceDimension::Localisation,
            EvidenceDimension::Derivation
        ]
    );
}

/// A dimension the runner does not know is a mistake in the manifest,
/// said out loud at load — not a silently ignored key.
#[test]
fn an_unknown_evidence_dimension_is_refused_at_load() {
    let dir = letter_pack("unknown-dimension");
    let manifest = std::fs::read_to_string(dir.join("pack.json")).expect("manifest reads");
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace("\"existence\"", "\"vibes\""),
    )
    .expect("manifest writes");

    let problem = load_pack(&dir).expect_err("'vibes' is not an evidence dimension");
    assert!(problem.to_string().contains("vibes"), "{problem}");
}

/// A declaration without its argument is refused the way an empty
/// eval_costs reason would be: the manifest records why a pack believes
/// it can score a dimension, not just that it wants to.
#[test]
fn an_evidence_declaration_with_an_empty_reason_is_refused() {
    let dir = letter_pack("empty-reason");
    let manifest = std::fs::read_to_string(dir.join("pack.json")).expect("manifest reads");
    std::fs::write(
        dir.join("pack.json"),
        manifest.replace("The quoted words must be in the letter.", ""),
    )
    .expect("manifest writes");

    let problem = load_pack(&dir).expect_err("an empty reason is no reason");
    assert!(
        problem.to_string().contains("existence"),
        "the refusal names the dimension: {problem}"
    );
}

/// Acceptance criterion: an unsupported-but-verbatim claim fails the
/// relevant harm metric. The support join and the harm lens agree by
/// construction — a claim that fails support is either an invention
/// (expected nothing) or a wrong assertion (expected something else),
/// and both already count confident-wrong. This pins that agreement so
/// no future join change can quietly break it.
#[test]
fn an_unsupported_verbatim_claim_fails_the_harm_metric() {
    use runner::eval::{EvalMetric, HarmClass, MetricReport};

    let dir = letter_pack("harm");
    let report = evaluate(&dir, answer_asserting_payment_from_the_negation());

    let item = report.fixtures[0]
        .items
        .iter()
        .find(|item| item.item_id == "settled-account-no-payment-01")
        .expect("the settled passage is a scored item");
    assert!(
        matches!(
            item.evidence.get(&EvidenceDimension::Support),
            Some(DimensionOutcome::Fail { .. })
        ),
        "the claim is unsupported: {:?}",
        item.evidence
    );

    let MetricReport::Extraction(metrics) = &report.metrics[&EvalMetric::Extraction] else {
        panic!("an extraction pack reports extraction metrics");
    };
    let invented = &metrics.overall.harm_classes[&HarmClass::NoObligation];
    assert_eq!(
        invented.confident_wrong_rate.successes, 1,
        "the unsupported claim is a confident wrong answer, not a diagnostic footnote"
    );
}
