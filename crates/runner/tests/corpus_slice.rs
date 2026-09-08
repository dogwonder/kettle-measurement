//! The shared capability corpus, first slice (plan work package 4).
//!
//! Fixed mock proposals over `evals/corpus/slice-01.json` establish
//! what the scorer says about a correct, wrong, missing, uncertain and
//! unsupported answer — raw reading and verified output judged apart,
//! whole-item correctness beside the fields, invented and missed asks
//! counted separately, and evidence attachment recorded. No weights.

use runner::document::Segment;
use runner::eval::corpus::{
    score_case, summarise, AskStatus, Case, Corpus, Evidence, Field, Outcome, Proposal,
    ProposedAsk, Selection, VerifiedAsk,
};
use runner::reading::{self, Checked, Kind, Reading};
use runner::run::{Obligation, When};
use runner::timeline::sort_timeline;
use std::collections::BTreeSet;
use std::path::Path;

fn corpus() -> Corpus {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/corpus/slice-01.json");
    Corpus::parse(&std::fs::read_to_string(&path).expect("committed corpus slice"))
        .expect("valid corpus")
}

fn diagnostic() -> Corpus {
    Corpus::parse(include_str!("../../../evals/corpus/diagnostic-01.json")).unwrap()
}

#[test]
fn pack_deadline_default_is_separate_from_source_truth_and_period_reading() {
    let corpus = diagnostic();
    let case = corpus.case("relation-form-001-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks[0].from = Some(Reading::new(0, "10 March 2026"));
    let shown = verify(case, &proposal);
    assert_eq!(shown[0].due.unwrap().to_string(), "2026-03-24");
    let score = score_case(&corpus, case, &proposal, &shown, &Selection::letter_pack());
    let a = &score.asks[0];
    assert!(matches!(
        a.verified[&Field::Deadline],
        Outcome::Wrong { .. }
    ));
    let contract = a.deadline_contract.as_ref().unwrap();
    assert_eq!(contract.reading, Outcome::Correct);
    assert_eq!(contract.structure, Some(Outcome::Correct));
    assert_eq!(contract.resolution, Outcome::Correct);
    assert!(contract.policy.is_some());
    assert_eq!(
        corpus.fact("relation-form-001-deadline").unwrap().status,
        runner::eval::corpus::FactStatus::Ambiguous
    );
    // A right calendar date cannot conceal misread structure, nor a wrong
    // calendar date be accepted merely because the right policy was declared.
    proposal.asks[0].read.as_mut().unwrap().count = 15;
    let mut wrong = shown.clone();
    wrong[0].due = Some("2026-03-25".parse().unwrap());
    let score = score_case(&corpus, case, &proposal, &wrong, &Selection::letter_pack());
    let contract = score.asks[0].deadline_contract.as_ref().unwrap();
    assert!(matches!(contract.structure, Some(Outcome::Wrong { .. })));
    assert!(matches!(contract.resolution, Outcome::Wrong { .. }));
}

#[test]
fn pointer_contract_and_strict_source_copy_keep_distinct_evidence() {
    let corpus = diagnostic();
    let case = corpus.case("relation-form-010-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks[0].deadline = Reading::new(3, "6 April 2026");
    let shown = verify(case, &proposal);
    let score = score_case(&corpus, case, &proposal, &shown, &Selection::letter_pack());
    let a = &score.asks[0];
    assert!(matches!(a.raw[&Field::Deadline], Outcome::Wrong { .. }));
    assert_eq!(a.evidence[&Field::Deadline], Evidence::Misattached);
    let contract = a.deadline_contract.as_ref().unwrap();
    assert_eq!(contract.reading, Outcome::Correct);
    assert_eq!(contract.evidence, Evidence::Attached);
    proposal.asks[0].deadline.at = 0;
    let score = score_case(&corpus, case, &proposal, &shown, &Selection::letter_pack());
    assert_eq!(
        score.asks[0].deadline_contract.as_ref().unwrap().evidence,
        Evidence::Misattached
    );
    proposal.asks[0].deadline = Reading::new(3, "7 April 2026");
    let score = score_case(&corpus, case, &proposal, &shown, &Selection::letter_pack());
    assert!(matches!(
        score.asks[0].deadline_contract.as_ref().unwrap().reading,
        Outcome::Wrong { .. }
    ));

    // No implicit list of benign wording edits is inferred from one run.
    let case = corpus.case("date-form-001-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks[0].deadline.value = "6 March 2026".into();
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verify(case, &proposal),
        &Selection::letter_pack(),
    );
    assert!(matches!(
        score.asks[0].deadline_contract.as_ref().unwrap().reading,
        Outcome::Wrong { .. }
    ));
    assert_eq!(score.asks[0].verified[&Field::Deadline], Outcome::Correct);
}

#[test]
fn task_slot_mismatches_never_claim_to_judge_action_meaning() {
    use runner::eval::corpus::{CandidateSite, SemanticCoverage};
    let corpus = diagnostic();
    let case = corpus.case("obligation-form-013-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks.truncate(1);
    for wording in [
        "Return the form and send photo ID",
        "Return the form",
        "Something unrelated",
    ] {
        proposal.asks[0].text = Some(wording.into());
        let score = score_case(
            &corpus,
            case,
            &proposal,
            &verify(case, &proposal),
            &Selection::letter_pack(),
        );
        let slots = score.task_slots.as_ref().unwrap();
        assert_eq!(
            (
                slots.expected,
                slots.raw_candidates,
                slots.missing_raw_slots
            ),
            (2, 1, 1)
        );
        assert!(matches!(
            slots.semantic_action_coverage,
            SemanticCoverage::NotAssessed
        ));
        assert_eq!(score.asks[0].raw_candidate, Some(0));
        assert_eq!(score.asks[1].raw_candidate, None);
    }
    let case = corpus.case("format-form-002-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    let mut extra = proposal.asks[0].clone();
    extra.passage = 2;
    extra.text = Some("Return the form and send photo ID".into());
    proposal.asks.push(extra);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verify(case, &proposal),
        &Selection::letter_pack(),
    );
    let unmatched = &score.task_slots.as_ref().unwrap().unmatched_raw;
    assert_eq!(unmatched.len(), 1);
    assert_eq!(unmatched[0].index, 2);
    assert_eq!(
        unmatched[0].text.as_deref(),
        Some("Return the form and send photo ID")
    );
    assert!(matches!(
        unmatched[0].site,
        CandidateSite::NoAuthoredAskSite
    ));
}

#[test]
fn authored_policy_and_reading_expectations_are_validated_before_execution() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../../../evals/corpus/diagnostic-01.json")).unwrap();
    for (field, value) in [
        ("due", serde_json::json!("2026-03-25")),
        ("base", serde_json::json!("unknown-fact")),
        ("policy", serde_json::json!("guess-any-date")),
    ] {
        let mut changed = original.clone();
        let case = changed["cases"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["id"] == "relation-form-001-letter")
            .unwrap();
        case["asks"][0]["deadline_resolution"][field] = value;
        assert!(Corpus::parse(&changed.to_string()).is_err());
    }
    let mut changed = original.clone();
    let case = changed["cases"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == "relation-form-010-letter")
        .unwrap();
    case["asks"][0]["deadline_reading"]["at"] = serde_json::json!(0);
    assert!(Corpus::parse(&changed.to_string()).is_err());
    let mut changed = original;
    let case = changed["cases"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == "relation-form-001-letter")
        .unwrap();
    case["asks"][0]["deadline_read"]["count"] = serde_json::json!(u64::MAX);
    case["asks"][0]["deadline_read"]["unit"] = serde_json::json!("weeks");
    assert!(Corpus::parse(&changed.to_string()).is_err());
}

#[test]
fn diagnostic_links_and_negative_sites_have_explicit_denominators() {
    let corpus = diagnostic();
    corpus
        .validate_inventory(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/capabilities"))
        .unwrap();
    assert_eq!(corpus.cases.len(), 33);
    let scores: Vec<_> = corpus
        .cases
        .iter()
        .map(|case| {
            let proposed = faithful(&corpus, case);
            let score = score_case(
                &corpus,
                case,
                &proposed,
                &verify(case, &proposed),
                &Selection::letter_pack(),
            );
            for ask in &score.asks {
                for field in &Selection::letter_pack().fields {
                    assert_eq!(
                        ask.raw[field],
                        Outcome::Correct,
                        "{} {} {field:?}",
                        case.id,
                        ask.ask
                    );
                }
            }
            score
        })
        .collect();
    let summary = summarise(&scores);
    assert_eq!(summary.items, 28);
    assert_eq!(summary.no_obligation_sites, 7);
    assert_eq!(summary.invented_raw, 0);
    let mut changed = corpus.clone();
    changed.cases[0].coverage[0].inventory_digest = "sha256:stale".into();
    assert!(changed
        .validate_inventory(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/capabilities"))
        .is_err());
    changed = corpus.clone();
    changed.selection.as_mut().unwrap().purpose = "challenge".into();
    assert!(changed.validate().is_err());
}

#[test]
fn copied_unresolved_words_and_pointed_dates_keep_their_distinct_evidence() {
    let corpus = diagnostic();
    for ident in [
        "date-form-014",
        "date-form-028",
        "date-form-029",
        "relation-form-010",
        "format-form-002",
    ] {
        let case = corpus
            .cases
            .iter()
            .find(|c| c.coverage[0].inventory_id == ident)
            .unwrap();
        let mut proposed = faithful(&corpus, case);
        let verified = verify(case, &proposed);
        let score = score_case(
            &corpus,
            case,
            &proposed,
            &verified,
            &Selection::letter_pack(),
        );
        assert_eq!(
            score.asks[0].raw[&Field::Deadline],
            Outcome::Correct,
            "{ident}"
        );
        assert_eq!(
            score.asks[0].evidence[&Field::Deadline],
            Evidence::Attached,
            "{ident}"
        );
        if ident.starts_with("date-") {
            assert_eq!(
                score.asks[0].verified[&Field::Deadline],
                Outcome::Correct,
                "unresolved {ident}"
            );
        }
        proposed.asks[0].deadline.at = 0;
        let score = score_case(
            &corpus,
            case,
            &proposed,
            &verified,
            &Selection::letter_pack(),
        );
        assert_eq!(
            score.asks[0].evidence[&Field::Deadline],
            Evidence::Misattached
        );
    }
}

#[test]
fn inventions_on_cancelled_or_completed_asks_stay_visible_by_site() {
    let corpus = diagnostic();
    for ident in [
        "obligation-form-006",
        "obligation-form-009",
        "obligation-form-010",
    ] {
        let case = corpus
            .cases
            .iter()
            .find(|c| c.coverage[0].inventory_id == ident)
            .unwrap();
        let proposal = Proposal {
            case: case.id.clone(),
            asks: vec![ProposedAsk {
                text: None,
                passage: 2,
                kind: "payment".into(),
                party: Reading::new(1, "Example Services"),
                deadline: Reading::absent(2),
                amount: Reading::absent(2),
                read: None,
                from: None,
                time: None,
                place: None,
                reference: None,
                confidence: "high".into(),
            }],
        };
        let score = score_case(
            &corpus,
            case,
            &proposal,
            &verify(case, &proposal),
            &Selection::letter_pack(),
        );
        assert!(score.asks.is_empty());
        assert_eq!(score.invented_raw, 1);
        assert_eq!(score.negative_sites[0].invented_raw, 1);
        assert_eq!(score.negative_sites[0].invented_verified, 1);
    }
}

#[test]
fn verified_money_preserves_currency_and_sign_and_names_unsupported_forms() {
    for (truth, currency, shown, expected) in [
        ("305.29", "GBP", "£305.29", "correct"),
        ("305.29", "GBP", "EUR 305.29", "wrong"),
        ("305.29", "GBP", "-£305.29", "wrong"),
        ("-305.29", "GBP", "£-305.29", "correct"),
        ("-305.29", "GBP", "£305.29", "wrong"),
        ("3052.90", "GBP", "GBP 3,052.9", "correct"),
        ("305.29", "USD", "$305.29", "unsupported"),
        ("305.29", "GBP", "305.29", "unsupported"),
        ("305.29", "GBP", "£3,05.29", "unsupported"),
        ("305.29", "GBP", "", "missing"),
    ] {
        let mut corpus = corpus();
        corpus
            .facts
            .iter_mut()
            .find(|f| f.id == "fact-instalment")
            .unwrap()
            .value = Some(runner::eval::corpus::Value::Money {
            amount: truth.into(),
            currency: currency.into(),
        });
        let case = corpus.case("slice01-letter").unwrap();
        let proposal = faithful(&corpus, case);
        let mut verified = verify(case, &proposal);
        verified
            .iter_mut()
            .find(|a| a.kind == "payment")
            .unwrap()
            .amount = shown.into();
        let score = score_case(
            &corpus,
            case,
            &proposal,
            &verified,
            &Selection::letter_pack(),
        );
        let (_, result) = outcome(&score, "ask-pay", Field::Amount);
        let label = match result {
            Outcome::Correct => "correct",
            Outcome::Wrong { .. } => "wrong",
            Outcome::Missing => "missing",
            Outcome::Unsupported => "unsupported",
            Outcome::Uncertain { .. } => "uncertain",
        };
        assert_eq!(label, expected, "{truth} {currency}, shown {shown:?}");
    }
}

fn segments(case: &Case) -> Vec<Segment> {
    case.passages
        .iter()
        .enumerate()
        .map(|(ordinal, text)| Segment {
            document: 0,
            page: 1,
            ordinal,
            text: text.clone(),
            rows: Vec::new(),
        })
        .collect()
}

/// The verified output as the runner would produce it from a raw
/// proposal: every reading through `reading::check` against the page
/// (a refused reading is absent), then the timeline. Nothing is
/// discovered here; the model's `at` is checked, never searched for.
fn verify(case: &Case, proposal: &Proposal) -> Vec<VerifiedAsk> {
    let segments = segments(case);
    let shown: BTreeSet<usize> = (0..segments.len()).collect();
    let obligations = proposal
        .asks
        .iter()
        .map(|p| {
            let own = &segments[p.passage];
            let checked =
                |r: &Reading, kind: Kind| match reading::check(r, kind, own, &shown, &segments) {
                    Checked::Supported { .. } => r.clone(),
                    Checked::Absent | Checked::Refused(_) => Reading::absent(p.passage),
                };
            Obligation {
                kind: p.kind.clone(),
                party: checked(&p.party, Kind::Name),
                ask: String::new(),
                deadline: p.deadline.clone(),
                // The structure and base as the model gives them since
                // scoring 19; a proposal without them is a period
                // nobody can count, and stays undated.
                read: p
                    .read
                    .clone()
                    .unwrap_or_else(|| When::new(0, "none", "none", "none")),
                from: p
                    .from
                    .as_ref()
                    .map(|from| checked(from, Kind::Date))
                    .unwrap_or_else(|| Reading::absent(p.passage)),
                unresolved: None,
                amount: checked(&p.amount, Kind::Money),
                refused: Vec::new(),
                confidence: p.confidence.clone(),
                due: None,
                evidence: vec![own.clone()],
                dated_by: None,
                priced_by: None,
                shown: shown.clone(),
                disputed: vec![],
            }
        })
        .collect();
    sort_timeline(obligations, &segments)
        .iter()
        .map(VerifiedAsk::from)
        .collect()
}

fn ask<'a>(case: &'a Case, id: &str) -> &'a runner::eval::corpus::Ask {
    case.asks.iter().find(|a| a.id == id).unwrap()
}

/// A faithful proposal for a case: each obligation read at the passage
/// that makes it, every field copied as the document words it.
fn faithful(_corpus: &Corpus, case: &Case) -> Proposal {
    let asks = case
        .asks
        .iter()
        .filter(|a| a.status == AskStatus::Obligation)
        .map(|a| {
            let span = |field: &str| {
                a.fields
                    .get(field)
                    .cloned()
                    .flatten()
                    .and_then(|fact| case.span(&fact))
                    .map(|s| Reading::new(s.passage, s.text.clone()))
            };
            ProposedAsk {
                text: None,
                passage: a.passage,
                kind: a.kind.clone(),
                party: span("party").unwrap(),
                deadline: Reading::new(
                    a.deadline_at.unwrap_or(a.passage),
                    a.deadline_words.clone().unwrap_or_default(),
                ),
                // The structure the corpus authors beside the words, and
                // the base fact's span where the period counts from one.
                read: a.deadline_read.clone(),
                from: span("base"),
                amount: span("amount").unwrap_or_else(|| Reading::absent(a.passage)),
                time: span("time"),
                place: span("place"),
                reference: span("reference"),
                confidence: "high".to_owned(),
            }
        })
        .collect();
    Proposal {
        case: case.id.clone(),
        asks,
    }
}

fn outcome<'a>(
    score: &'a runner::eval::corpus::CaseScore,
    ask: &str,
    field: Field,
) -> (&'a Outcome, &'a Outcome) {
    let a = score.asks.iter().find(|a| a.ask == ask).unwrap();
    (&a.raw[&field], &a.verified[&field])
}

#[test]
fn a_faithful_proposal_is_correct_on_every_selected_field_in_both_kinds() {
    let corpus = corpus();
    assert_eq!(corpus.cases.len(), 2, "two document kinds share the slice");
    for case in &corpus.cases {
        let proposal = faithful(&corpus, case);
        let verified = verify(case, &proposal);
        let score = score_case(
            &corpus,
            case,
            &proposal,
            &verified,
            &Selection::letter_pack(),
        );
        for a in &score.asks {
            for field in Selection::letter_pack().fields {
                assert_eq!(
                    a.raw[&field],
                    Outcome::Correct,
                    "{} {} {:?} raw",
                    case.id,
                    a.ask,
                    field
                );
                // One cell is honestly not a pass: see the next test.
                if (case.document_kind.as_str(), a.ask.as_str(), field)
                    == ("statement", "ask-attend", Field::Deadline)
                {
                    continue;
                }
                assert_eq!(
                    a.verified[&field],
                    Outcome::Correct,
                    "{} {} {:?} verified",
                    case.id,
                    a.ask,
                    field
                );
            }
            assert!(a.whole_item_raw, "{} {}", case.id, a.ask);
        }
        assert_eq!(
            (score.invented_raw, score.invented_verified),
            (0, 0),
            "{}",
            case.id
        );
        assert_eq!(
            score.asks.len(),
            2,
            "{}: the conditional ask is not an obligation",
            case.id
        );
    }
}

/// The first thing the corpus found: the statement writes the
/// appointment as `02/04/2026`, which the resolver refuses on purpose
/// (both fields could be the month), so the verified output shows no
/// date at all. The raw reading copied the right words; the verified
/// column is `Missing`, not `Wrong` and never `Correct` — the two
/// columns disagreeing is the finding, and pooling them would hide it.
#[test]
fn an_ambiguous_all_numeric_date_is_read_correctly_and_verified_as_missing() {
    let corpus = corpus();
    let case = corpus.case("slice01-statement").unwrap();
    let proposal = faithful(&corpus, case);
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let (raw, ver) = outcome(&score, "ask-attend", Field::Deadline);
    assert_eq!((raw, ver), (&Outcome::Correct, &Outcome::Missing));
    let attend = score.asks.iter().find(|a| a.ask == "ask-attend").unwrap();
    assert!(attend.whole_item_raw && !attend.whole_item_verified);
    // The payment row's `24/03/2026` is unambiguous and resolves.
    let (raw, ver) = outcome(&score, "ask-pay", Field::Deadline);
    assert_eq!((raw, ver), (&Outcome::Correct, &Outcome::Correct));
}

#[test]
fn the_annual_total_beside_the_instalment_is_wrong_on_both_sides_and_fails_the_item() {
    let corpus = corpus();
    let case = corpus.case("slice01-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    let total = case.span("fact-annual-total").unwrap();
    proposal.asks[0].amount = Reading::new(total.passage, total.text.clone());
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let (raw, ver) = outcome(&score, "ask-pay", Field::Amount);
    assert_eq!(
        raw,
        &Outcome::Wrong {
            got: "£3,052.90".to_owned()
        }
    );
    // The verifier accepts it: the sum is on the page. Verification
    // cannot catch a wrong choice between two true figures, which is
    // exactly why the two columns are kept apart.
    assert_eq!(
        ver,
        &Outcome::Wrong {
            got: "£3,052.90".to_owned()
        }
    );
    let pay = &score.asks[0];
    assert_eq!(pay.raw[&Field::Deadline], Outcome::Correct);
    assert!(!pay.whole_item_raw && !pay.whole_item_verified);
}

#[test]
fn a_date_copied_faithfully_from_the_wrong_event_is_wrong_and_misattached() {
    let corpus = corpus();
    let case = corpus.case("slice01-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    let earlier = case.span("fact-earlier-letter-date").unwrap();
    proposal.asks[1].deadline = Reading::new(earlier.passage, earlier.text.clone());
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let (raw, ver) = outcome(&score, "ask-attend", Field::Deadline);
    assert_eq!(
        raw,
        &Outcome::Wrong {
            got: "3 March 2026".to_owned()
        }
    );
    assert_eq!(
        ver,
        &Outcome::Wrong {
            got: "2026-03-03".to_owned()
        }
    );
    assert_eq!(
        score.asks[1].evidence[&Field::Deadline],
        Evidence::Misattached
    );
}

#[test]
fn a_missed_ask_is_missing_on_every_field_and_counted_once() {
    let corpus = corpus();
    let case = corpus.case("slice01-statement").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks.retain(|a| a.kind != "attendance");
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let attend = score.asks.iter().find(|a| a.ask == "ask-attend").unwrap();
    assert!(attend.missed_raw && attend.missed_verified);
    for field in Selection::letter_pack().fields {
        assert_eq!(attend.raw[&field], Outcome::Missing);
        assert_eq!(attend.verified[&field], Outcome::Missing);
    }
    let summary = summarise(&[score]);
    assert_eq!(
        (summary.items, summary.missed_raw, summary.whole_item_raw),
        (2, 1, 1)
    );
}

#[test]
fn an_ask_asserted_on_the_conditional_passage_is_invented_not_a_wrong_field() {
    let corpus = corpus();
    let case = corpus.case("slice01-letter").unwrap();
    let tenants = ask(case, "ask-tenants");
    let mut proposal = faithful(&corpus, case);
    proposal.asks.push(ProposedAsk {
        text: None,
        passage: tenants.passage,
        kind: "response".to_owned(),
        party: Reading::new(1, "Redhill District Council"),
        deadline: Reading::absent(tenants.passage),
        read: None,
        from: None,
        amount: Reading::absent(tenants.passage),
        time: None,
        place: None,
        reference: None,
        confidence: "high".to_owned(),
    });
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    assert_eq!((score.invented_raw, score.invented_verified), (1, 1));
    assert!(
        score.asks.iter().all(|a| a.whole_item_raw),
        "the true asks are still right"
    );
}

#[test]
fn a_low_confidence_answer_is_uncertain_and_never_a_whole_item_pass() {
    let corpus = corpus();
    let case = corpus.case("slice01-letter").unwrap();
    let mut proposal = faithful(&corpus, case);
    proposal.asks[0].confidence = "low".to_owned();
    let verified = verify(case, &proposal);
    let score = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let pay = &score.asks[0];
    assert_eq!(
        pay.raw[&Field::Amount],
        Outcome::Uncertain { correct: true }
    );
    assert!(!pay.whole_item_raw);
    let summary = summarise(&[score]);
    assert_eq!(summary.raw[&Field::Amount].uncertain, 1);
    // The attendance ask states no sum, and none was read: correct.
    assert_eq!(summary.raw[&Field::Amount].correct, 1);
    assert_eq!(summary.whole_item_raw, 1);
}

#[test]
fn a_right_time_outside_the_packs_selection_is_unsupported_never_correct() {
    let corpus = corpus();
    let case = corpus.case("slice01-letter").unwrap();
    let proposal = faithful(&corpus, case);
    assert_eq!(
        proposal.asks[1].time.as_ref().map(|r| r.value.as_str()),
        Some("2:30pm")
    );
    let verified = verify(case, &proposal);
    let letter = score_case(
        &corpus,
        case,
        &proposal,
        &verified,
        &Selection::letter_pack(),
    );
    let (raw, ver) = outcome(&letter, "ask-attend", Field::Time);
    assert_eq!((raw, ver), (&Outcome::Unsupported, &Outcome::Unsupported));
    // Selecting the field judges the raw reading; the verified output
    // still carries no time, and says so rather than passing.
    let all = score_case(&corpus, case, &proposal, &verified, &Selection::all());
    let (raw, ver) = outcome(&all, "ask-attend", Field::Time);
    assert_eq!((raw, ver), (&Outcome::Correct, &Outcome::Unsupported));
    let attend = all.asks.iter().find(|a| a.ask == "ask-attend").unwrap();
    assert!(attend.whole_item_raw && !attend.whole_item_verified);
    let summary = summarise(&[letter]);
    assert_eq!(summary.raw[&Field::Time].unsupported, 2);
    assert_eq!(summary.raw[&Field::Time].correct, 0);
}

#[test]
fn a_time_given_for_the_payment_is_an_invention_because_the_document_states_none() {
    let corpus = corpus();
    let case = corpus.case("slice01-statement").unwrap();
    let mut proposal = faithful(&corpus, case);
    assert!(
        proposal.asks[0].time.is_none(),
        "the absent fact renders nowhere"
    );
    proposal.asks[0].time = Some(Reading::new(6, "14:30"));
    let verified = verify(case, &proposal);
    let score = score_case(&corpus, case, &proposal, &verified, &Selection::all());
    let (raw, _) = outcome(&score, "ask-pay", Field::Time);
    assert_eq!(
        raw,
        &Outcome::Wrong {
            got: "14:30".to_owned()
        }
    );
}

#[test]
fn the_slice_says_where_it_came_from_and_binds_every_span() {
    let corpus = corpus();
    assert_eq!(corpus.provenance["synthetic"], true);
    assert!(corpus.provenance["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(corpus.relations.len(), 3);
    for rel in &corpus.relations {
        assert_ne!(
            corpus.fact(&rel.selected).unwrap().value,
            corpus.fact(&rel.distractor).unwrap().value,
            "{}",
            rel.id
        );
    }
}
