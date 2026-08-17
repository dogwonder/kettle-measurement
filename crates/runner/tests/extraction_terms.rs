//! #351: the extraction lens carries a payload that is not a letter
//! obligation.
//!
//! The expensive half of the lens — the miss/invent classification,
//! Wilson ceilings, strata, the pooled gate, the tombstone registry —
//! was built generic (#240, #279). The payload was not: `Found` held an
//! `ExpectedObligation` and every field of it was a letter's. A
//! renewal term (#66, #350) shares none of those fields, so #66 could
//! not score a decision at all.
//!
//! These tests are about the payload only. Nothing here asserts a new
//! statistic: if the lens now says something different about an
//! obligation, that is a regression, not a feature.

mod support;

use runner::claim_trace::{CheckOutcome, ClaimKind, Guardrail, TerminalDisposition};
use runner::eval::{
    extraction_metrics, EvalMetric, ExpectedObligation, ExpectedTerm, Extracted, ExtractionOutcome,
    HarmClass, ScoredDecision, ScoredItem,
};
use runner::packs::load_pack;
use runner::run::{run_pack_bound, Answers};
use runner::run_dir::NoLog;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use support::{completion_envelope, MockModel};

/// One scored decision about a named term: what the document should
/// have been read as saying, and what the run made of it.
fn scored_term(
    ordinal: usize,
    expected: Option<ExpectedTerm>,
    actual: ExtractionOutcome,
) -> ScoredItem {
    ScoredItem {
        id: format!("app.kttl.test-renewal/renewal-01/term-{ordinal:02}"),
        item_id: format!("term-{ordinal:02}"),
        pack: "app.kttl.test-renewal".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:test".to_owned(),
        fixture: "renewal.txt".to_owned(),
        fixture_id: "renewal-01".to_owned(),
        strata: vec!["named-value".to_owned()],
        raw_input: format!("PASSAGE {ordinal}"),
        decision_key: format!("PASSAGE {ordinal}"),
        decision: ScoredDecision::Extraction {
            expected_review: false,
            expected: expected.map(Extracted::Term),
            unauthored_negative: false,
            actual,
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: Vec::new(),
    }
}

fn term(name: &str, value: &str) -> ExpectedTerm {
    ExpectedTerm {
        term: name.to_owned(),
        basis: "per_claim".to_owned(),
        value: value.to_owned(),
        quote: format!("{name}: {value}"),
    }
}

/// The test #351 names. A policy term must classify under the same
/// lens, in its own vocabulary, with no obligation field populated —
/// and `extraction_metrics` must count it in its stratum.
#[test]
fn a_term_extraction_scores_a_miss_and_an_invention_without_an_obligation() {
    let items = vec![
        // Read correctly.
        scored_term(
            1,
            Some(term("compulsory_excess", "£500")),
            ExtractionOutcome::Found {
                extracted: Extracted::Term(term("compulsory_excess", "£500")),
            },
        ),
        // A miss: the passage states a term and the run asserted
        // nothing from it.
        scored_term(
            2,
            Some(term("compulsory_excess", "£250")),
            ExtractionOutcome::Absent,
        ),
        // An invention: the passage states no named term and the run
        // asserted one anyway.
        scored_term(
            3,
            None,
            ExtractionOutcome::Found {
                extracted: Extracted::Term(term("premium", "£612.50")),
            },
        ),
        // Surfaced rather than asserted: neither a hit nor a miss.
        scored_term(
            4,
            Some(term("compulsory_excess", "£500")),
            ExtractionOutcome::NeedsReview {
                reason: "Kettle wasn't sure.".to_owned(),
            },
        ),
    ];

    let metrics = extraction_metrics(&items);

    assert_eq!(metrics.overall.n, 4, "every term decision is counted");
    let stratum = metrics
        .strata
        .get("named-value")
        .expect("terms reach their own stratum");
    assert_eq!(stratum.n, 4);

    // The class that carries a value: one right, one missed, one
    // surfaced. The miss is the confident-wrong cell.
    let carries = stratum
        .harm_classes
        .get(&HarmClass::Obligation)
        .expect("the extraction lens's carries-a-value class");
    assert_eq!(
        carries.confident_wrong_rate.successes, 1,
        "the miss: {carries:?}"
    );

    // The class that carries none: the invention is its confident-wrong
    // cell, exactly as it is for a letter.
    let carries_none = stratum
        .harm_classes
        .get(&HarmClass::NoObligation)
        .expect("the extraction lens's carries-nothing class");
    assert_eq!(
        carries_none.confident_wrong_rate.successes, 1,
        "the invention: {carries_none:?}"
    );
}

/// A term decision is an extraction decision. It is the metric that
/// decides which ceilings and which gate read it, so a payload change
/// that moved the metric would silently take a pack's declared gates
/// out of play.
#[test]
fn a_term_decision_is_still_an_extraction_decision() {
    let item = scored_term(
        1,
        Some(term("premium", "£480.00")),
        ExtractionOutcome::Absent,
    );

    assert_eq!(item.decision.metric(), EvalMetric::Extraction);
    assert_eq!(
        item.decision.describe_expected(),
        "premium / per_claim / £480.00"
    );
    assert!(
        item.decision.as_extraction().is_some(),
        "the lens reads it without knowing which shape it carries"
    );
}

/// The recordings on disk are the reason this matters more than tidiness
/// (#303, #320). An obligation must serialise exactly as it did before
/// `Found` was decoupled, and every recording already written must still
/// read back as the decision it recorded — otherwise the payload change
/// silently invalidates every baseline and forces a re-measurement
/// nobody asked for.
#[test]
fn an_obligation_recording_survives_the_decoupling() {
    let obligation = ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Harborne Parking Services".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "the date of this letter".to_owned(),
        due: None,
    };

    // The bytes an already-recorded item carries, field name and all.
    let recorded = serde_json::json!({
        "id": "app.kttl.letter-to-actions/letters-01/obligation-01",
        "item_id": "obligation-01",
        "pack": "app.kttl.letter-to-actions",
        "pack_version": "1.0.0",
        "prompt_version": "blake3:test",
        "fixture": "letter.txt",
        "fixture_id": "letters-01",
        "strata": ["any-letter"],
        "raw_input": "PASSAGE 1",
        "decision_key": "PASSAGE 1",
        "metric": "extraction",
        "expected": {
            "kind": "payment",
            "party": "Harborne Parking Services",
            "deadline": "within 14 days",
            "anchor": "the date of this letter",
            "due": null
        },
        "actual": { "outcome": "found", "obligation": {
            "kind": "payment",
            "party": "Harborne Parking Services",
            "deadline": "within 14 days",
            "anchor": "the date of this letter",
            "due": null
        } },
        "exchanges": []
    });

    let item: ScoredItem =
        serde_json::from_value(recorded).expect("an existing recording still reads");
    let (expected, actual) = item
        .decision
        .as_extraction()
        .expect("it is still an extraction decision");
    assert_eq!(
        expected.as_ref(),
        Some(&Extracted::Obligation(obligation.clone()))
    );
    assert_eq!(
        actual,
        &ExtractionOutcome::Found {
            extracted: Extracted::Obligation(obligation.clone()),
        }
    );
    assert_eq!(
        actual.describe(),
        "payment / Harborne Parking Services / within 14 days",
        "an obligation's description must not move — a changed description \
         is a scoring change, and scoring changes bump SCORING_VERSION"
    );

    // And the expected half serialises back to exactly the bytes it
    // came from: an obligation is written as an obligation, not as a
    // wrapper around one.
    assert_eq!(
        serde_json::to_value(Extracted::Obligation(obligation)).expect("serialises"),
        serde_json::json!({
            "kind": "payment",
            "party": "Harborne Parking Services",
            "deadline": "within 14 days",
            "anchor": "the date of this letter",
            "due": null
        }),
    );
}

/// The same guarantee against the recordings this repo actually holds,
/// rather than against one this test wrote. `evals/*.json` are what a
/// prompt edit is compared with (`--baseline`, exit 1 on any drop); a
/// payload change that made them unreadable would silently retire every
/// comparison the project can make, and cost hours of local model time
/// to rebuild.
#[test]
fn every_recorded_item_in_the_shipped_baselines_still_reads() {
    let evals = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut items_read = 0usize;
    let mut extractions = 0usize;

    for entry in std::fs::read_dir(&evals).expect("the evals directory") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read a baseline");
        let baseline: serde_json::Value = serde_json::from_str(&text).expect("a baseline parses");
        let reports = baseline["reports"].as_array().cloned().unwrap_or_default();
        for report in reports {
            for fixture in report["fixtures"].as_array().cloned().unwrap_or_default() {
                for item in fixture["items"].as_array().cloned().unwrap_or_default() {
                    let scored: ScoredItem =
                        serde_json::from_value(item.clone()).unwrap_or_else(|e| {
                            panic!("{} no longer reads: {e}\n{item}", path.display())
                        });
                    items_read += 1;
                    if scored.decision.metric() == EvalMetric::Extraction {
                        extractions += 1;
                    }
                }
            }
        }
    }

    // A test that read nothing would pass silently and guarantee
    // nothing, which is the failure mode this whole file is about.
    assert!(items_read > 0, "no recorded items were found to read");
    assert!(
        extractions > 0,
        "no recorded *extraction* items were read, so the payload this test \
         exists for was never exercised"
    );
}

/// The safeguard the untagged representation needs: two shapes that
/// cannot be told apart would be read as whichever came first in the
/// enum, silently. This fails the day a new payload overlaps an
/// existing one, which is when somebody has to choose a tag.
#[test]
fn the_two_payload_shapes_cannot_be_mistaken_for_each_other() {
    let obligation = serde_json::json!({
        "kind": "payment", "party": "Harborne Parking Services",
        "deadline": "within 14 days", "anchor": "the date of this letter", "due": null
    });
    let policy_term = serde_json::json!({
        "term": "compulsory_excess", "basis": "per_claim",
        "value": "£500", "quote": "Compulsory excess: £500 per claim."
    });

    assert!(matches!(
        serde_json::from_value::<Extracted>(obligation).expect("reads"),
        Extracted::Obligation(_)
    ));
    assert!(matches!(
        serde_json::from_value::<Extracted>(policy_term).expect("reads"),
        Extracted::Term(_)
    ));
}

/// The test #460 names — rule 1, the refusal. On the 8 August v12
/// renewal run the model quoted the bare label `Excess` as evidence for
/// three different numbers, and every one passed `quote_is_in`, because
/// `Excess` is verbatim in its own passage. A quote that supports three
/// values supports none of them: a quote that does not contain the
/// value it evidences is not evidence, and the claim is routed to
/// review, never compared. The rule is claim-local — the neighbouring
/// term, quoting its passage in full, still reads.
#[test]
fn a_quote_that_does_not_contain_its_value_is_not_evidence() {
    let pack =
        load_pack(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff"))
            .expect("the built-in renewal pack loads");

    let dir = std::env::temp_dir().join(format!("kettle-quote-rule-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("quote rule scratch directory");
    let previous = dir.join("previous.txt");
    let renewal = dir.join("renewal.txt");
    let previous_text = "Excess: £5,420.00 each and every claim.";
    let renewal_text = "Excess: £5,451.00 each and every claim.";
    std::fs::write(&previous, previous_text).expect("previous policy");
    std::fs::write(&renewal, renewal_text).expect("renewal policy");

    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                {
                    "id": 0,
                    "segment": previous_text,
                    "confidence": "high",
                    "terms": [{
                        "term": "compulsory_excess",
                        "basis": "per_claim",
                        "value": "£5,420.00",
                        "quote": "Excess"
                    }]
                },
                {
                    "id": 1,
                    "segment": renewal_text,
                    "confidence": "high",
                    "terms": [{
                        "term": "compulsory_excess",
                        "basis": "per_claim",
                        "value": "£5,451.00",
                        "quote": "Excess: £5,451.00 each and every claim."
                    }]
                }
            ]
        })
        .to_string(),
    );
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);

    let outcome = run_pack_bound(
        &pack,
        &[("previous", previous), ("renewal", renewal)],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the guarded run completes");

    let bare = outcome
        .claim_traces
        .iter()
        .find(|trace| {
            trace.kind == ClaimKind::PolicyTerm && trace.candidate["value"] == "£5,420.00"
        })
        .expect("the bare-label candidate remains traceable");
    assert_eq!(bare.check(Guardrail::Schema), Some(CheckOutcome::Passed));
    assert_eq!(bare.check(Guardrail::Pairing), Some(CheckOutcome::Passed));
    assert_eq!(
        bare.check(Guardrail::Quote),
        Some(CheckOutcome::Failed),
        "`Excess` is on the page, and it does not contain £5,420.00 — \
         a quote that does not contain its value is not evidence"
    );
    assert_eq!(bare.terminal, TerminalDisposition::NeedsReview);

    let full = outcome
        .claim_traces
        .iter()
        .find(|trace| {
            trace.kind == ClaimKind::PolicyTerm && trace.candidate["value"] == "£5,451.00"
        })
        .expect("the fully-quoted candidate remains traceable");
    assert_eq!(
        full.terminal,
        TerminalDisposition::Accepted,
        "the rule is claim-local: a neighbour quoting in full still reads"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Rule 2 of #460 — the quote identifies its passage — as a warning,
/// never a refusal. A quote verbatim in two passages of its document
/// leaves a person unable to say which one the claim rests on, but that
/// is a property of the document rather than of the claim, so refusing
/// on it would hand a person a reason they cannot act on. The claim
/// still reads; the trace says the evidence is weaker than it looks.
/// Document-scoped: the same words appearing once in the *other*
/// document take nothing from a quote that is unique in its own.
#[test]
fn a_quote_found_in_two_passages_of_its_document_warns_and_still_reads() {
    let pack =
        load_pack(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff"))
            .expect("the built-in renewal pack loads");

    let dir = std::env::temp_dir().join(format!("kettle-quote-passage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("quote passage scratch directory");
    let previous = dir.join("previous.txt");
    let renewal = dir.join("renewal.txt");
    // Two sections of last year's policy state the same excess in the
    // same words; this year's policy states it once.
    let previous_buildings = "Buildings cover. Excess: £250 per claim.";
    let previous_contents = "Contents cover. Excess: £250 per claim.";
    let renewal_text = "All sections. Excess: £250 per claim.";
    std::fs::write(
        &previous,
        format!("{previous_buildings}\n\n{previous_contents}"),
    )
    .expect("previous policy");
    std::fs::write(&renewal, renewal_text).expect("renewal policy");

    let answer = completion_envelope(
        &serde_json::json!({
            "results": [
                {
                    "id": 0,
                    "segment": previous_buildings,
                    "confidence": "high",
                    "terms": [{
                        "term": "compulsory_excess",
                        "basis": "per_claim",
                        "value": "£250",
                        "quote": "Excess: £250 per claim."
                    }]
                },
                {
                    "id": 1,
                    "segment": previous_contents,
                    "confidence": "high",
                    "terms": []
                },
                {
                    "id": 2,
                    "segment": renewal_text,
                    "confidence": "high",
                    "terms": [{
                        "term": "compulsory_excess",
                        "basis": "per_claim",
                        "value": "£250",
                        "quote": "Excess: £250 per claim."
                    }]
                }
            ]
        })
        .to_string(),
    );
    let mock = MockModel::respond_sequence(vec![("200 OK", answer)]);

    let outcome = run_pack_bound(
        &pack,
        &[("previous", previous), ("renewal", renewal)],
        &Answers::FromModel(mock.endpoint()),
        &AtomicBool::new(false),
        &mut |_| {},
        &NoLog,
    )
    .expect("the guarded run completes");

    let ambiguous = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::PolicyTerm && trace.source == previous_buildings)
        .expect("the ambiguously quoted candidate remains traceable");
    assert_eq!(
        ambiguous.check(Guardrail::Quote),
        Some(CheckOutcome::Passed)
    );
    assert_eq!(
        ambiguous.check(Guardrail::QuoteIdentifiesPassage),
        Some(CheckOutcome::Warned),
        "the quote is verbatim in both of last year's cover sections, \
         so it does not identify its passage"
    );
    assert_eq!(
        ambiguous.terminal,
        TerminalDisposition::Accepted,
        "a warning is not a refusal: the claim still reads"
    );

    let unique = outcome
        .claim_traces
        .iter()
        .find(|trace| trace.kind == ClaimKind::PolicyTerm && trace.source == renewal_text)
        .expect("the uniquely quoted candidate remains traceable");
    assert_eq!(
        unique.check(Guardrail::QuoteIdentifiesPassage),
        Some(CheckOutcome::Passed),
        "the same words appear in the other document, and the rule is \
         document-scoped: unique in its own document is identified"
    );
    assert_eq!(unique.terminal, TerminalDisposition::Accepted);

    let _ = std::fs::remove_dir_all(&dir);
}
