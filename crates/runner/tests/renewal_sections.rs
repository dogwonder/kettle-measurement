//! #378: the bed can now represent a schedule with repeated sections.
//!
//! Every other fixture in the renewal bed is a single-section personal
//! policy, where each modelled term occurs once and `(term, basis)` is
//! a sufficient key. The pack met a commercial schedule on 4 August
//! 2026 and was confidently wrong about it, and the bed could not see
//! any of it — not because the case was unmeasured, but because it was
//! **unrepresentable** in the bed as authored.
//!
//! The first two tests are about the bed rather than the runner: they
//! hold the shape in it, as a ratchet against regenerating the blind
//! spot back in.
//!
//! The third was authored red and `#[ignore]`d, because what the runner
//! should *do* with a repeated term was #377's to decide and not
//! something a test could settle. It was decided on 4 August 2026 — the
//! floor, keyed on the repetition itself — so the ignore is gone and
//! the test now asserts the answer rather than recording the failure.

use runner::eval::fixture::{fixtures_in, score_fixture, TermExpectation};
use runner::eval::renewals::SECTIONS_REPEAT_SHEET;
use runner::eval::{Extracted, ExtractionOutcome, Perf, ScoredDecision};
use runner::packs::load_pack;
use runner::run::{BoundInput, ComparisonOutcome, InputSeen, Payload, RunOutcome};
use runner::terms::{diff_terms, Term, TermFamilies};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff")
}

/// The bed contains a document that states the same `(term, basis)`
/// more than once.
///
/// This is the whole of #378 in one assertion. Before it, no fixture in
/// 88 did — checked, and the answer was zero — so the pairing key could
/// never be exercised where it is arbitrary, and the eval passing was
/// evidence that the pack works *on documents shaped like the bed*.
///
/// A ratchet, deliberately: regenerate the bed without this shape and
/// this fails rather than quietly returning the blind spot.
#[test]
fn the_bed_states_a_term_more_than_once_in_one_document() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    let mut repeating = 0;
    let mut most = 0;
    for fixture in &fixtures {
        let mut per_document: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
        for item in &fixture.expected.policy_terms {
            let Some(expect) = &item.expect else { continue };
            *per_document
                .entry((
                    item.role.as_str(),
                    expect.term.as_str(),
                    expect.basis.as_str(),
                ))
                .or_default() += 1;
        }
        let peak = per_document.values().copied().max().unwrap_or(0);
        if peak > 1 {
            repeating += 1;
        }
        most = most.max(peak);
    }

    assert!(
        repeating >= 8,
        "the bed must carry the repeated-section shape in both sets: {repeating} fixture(s) \
         state a term twice in one document"
    );
    // Four `(premium, annual)` readings in one document: three cover
    // sections and the schedule's own total. That is the passage pair
    // the sevenfold overstatement came from — a section's premium
    // compared against the whole schedule's.
    assert!(
        most >= 4,
        "a section premium and a document total are both (premium, annual): peak was {most}"
    );
}

/// The bed carries a passage whose correct answer is "this states no
/// value I model", written where a monetary limit could plausibly be
/// read.
///
/// The policy period was read as a `cover_limit` on the real document
/// and compared against money. #380 now refuses the value on its shape,
/// but the reading itself is still an invention, and an invention the
/// bed could not previously score because no such passage existed.
#[test]
fn the_bed_plants_a_date_range_where_a_limit_could_be_read() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    let periods: Vec<&str> = fixtures
        .iter()
        .flat_map(|fixture| &fixture.expected.policy_terms)
        .filter(|item| {
            item.expect.is_none()
                && item.segment.contains("1 September")
                && item.segment.contains("31 August")
        })
        .map(|item| item.segment.as_str())
        .collect();

    assert!(
        periods.len() >= 16,
        "every section fixture states its period, in both documents: {} found",
        periods.len()
    );
}

/// The failure #378 made representable, now that #377 has decided it.
///
/// This was authored red and `#[ignore]`d — the bed could show the
/// failure and nothing was going to fix it until the decision landed.
/// The decision (4 August 2026) was the floor, keyed on the repetition
/// itself: a `(term, basis)` read more than once in one document does
/// not pair at all.
///
/// It reads the fixture's own expectations — the readings a *perfect*
/// model makes, so nothing here depends on a model — and asks what the
/// diff does with them. It used to take the first reading from each
/// document and drop the other three, so a schedule where every
/// section's premium rose by £15.00 reported one £15.00 rise and said
/// nothing about the £45.00 the person actually pays.
#[test]
fn a_repeated_term_does_not_pair_across_sections() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");
    let fixture = fixtures
        .iter()
        .find(|fixture| fixture.name.contains("sections_repeat"))
        .expect("the bed carries a repeated-section fixture");

    // A perfect reading of both documents: every expectation the bed
    // holds, attributed to the document it belongs to.
    let terms: Vec<Term> = fixture
        .expected
        .policy_terms
        .iter()
        .filter_map(|item| {
            let expect = item.expect.as_ref()?;
            Some(Term {
                term: expect.term.clone(),
                basis: expect.basis.clone(),
                value: expect.value.clone(),
                quote: expect.quote.clone(),
                segment: item.segment.clone(),
                document: usize::from(item.role == "renewal"),
                confidence: "high".to_owned(),
            })
        })
        .collect();

    let diff = diff_terms(&terms, 0, 1, &excess_family());

    // Four `(premium, annual)` readings on each side — three cover
    // sections and the schedule's own total. No row may claim to have
    // compared them: every wrong choice among those four is wrong by
    // hundreds, and the report would render it as Kettle's arithmetic.
    assert!(
        !diff
            .rows
            .iter()
            .any(|row| row.term == "premium" && row.basis == "annual"),
        "four premiums on each side must not become one subtraction: {:#?}",
        diff.rows
    );

    // Refused, not dropped: the reader is told which term, how many
    // times, and shown every passage behind it.
    let premium = diff
        .not_compared
        .iter()
        .find(|refused| refused.term == "premium" && refused.basis == "annual")
        .expect("the premium is reported as not compared");
    assert_eq!(premium.readings, 4);
    assert_eq!(premium.quotes.len(), 8, "both documents' readings");

    // And the rest of the schedule is refused for the same reason
    // rather than half-answered: each section repeats its cover limit
    // and its excess too.
    for term in ["cover_limit", "compulsory_excess"] {
        let refused = diff
            .not_compared
            .iter()
            .find(|refused| refused.term == term)
            .unwrap_or_else(|| panic!("{term} repeats under three sections"));
        assert_eq!(refused.readings, 3, "{term}");
    }
}

/// #457: an expectation is scored against the term read from **its own**
/// passage, even when the quote alone cannot say which passage that was.
///
/// The 8 August v12 measurement failed the renewal miss ceiling at 0.05
/// (n=302), every one of the 16 wrong assertions in a `sections_repeat`
/// fixture. The run dir says the model was not wrong about any of them:
/// it read all three sections of both documents correctly. What differed
/// was the quote — `£1,462.20` out of the previous document, the bare
/// label `Annual premium` out of the renewal.
///
/// A bare label is verbatim in its own passage, so the run's #258 quote
/// guardrail passes it, and it is *equally* verbatim in the other two
/// sections. The eval joined an expectation to the first term whose
/// quote its segment contained, so sections two and three were both
/// scored against section one's reading and the person was recorded as
/// having been told £1,462.20 three times. Nobody was: `diff_terms`
/// refuses a repeated `(term, basis)` and every reading went to review.
///
/// The join is the defect. It re-derived which passage a term came from
/// out of the quote, when the run already knew — and a weak quote then
/// silently rebound a correct reading onto the wrong expectation. This
/// asserts the property the join must have: one term, one passage, its
/// own.
#[test]
fn repeated_sections_bind_each_value_to_its_own_section() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");
    let fixture = fixtures
        .iter()
        .find(|fixture| fixture.name.contains("sections_repeat"))
        .expect("the bed carries a repeated-section fixture");

    // A perfect reading of both documents, quoted the way the model
    // actually quoted the renewal: the label, not the value. Every
    // value here is the one its own passage states.
    let terms: Vec<Term> = fixture
        .expected
        .policy_terms
        .iter()
        .filter_map(|item| {
            let expect = item.expect.as_ref()?;
            Some(Term {
                term: expect.term.clone(),
                basis: expect.basis.clone(),
                value: expect.value.clone(),
                quote: item
                    .segment
                    .split_once(':')
                    .map(|(label, _)| label.to_owned())
                    .unwrap_or_else(|| expect.quote.clone()),
                segment: item.segment.clone(),
                document: usize::from(item.role == "renewal"),
                confidence: "high".to_owned(),
            })
        })
        .collect();

    let outcome = RunOutcome {
        input: InputSeen {
            rows: 0,
            period: None,
        },
        inputs: vec![
            BoundInput {
                role: "previous".to_owned(),
                file: "previous.txt".to_owned(),
            },
            BoundInput {
                role: "renewal".to_owned(),
                file: "renewal.txt".to_owned(),
            },
        ],
        needs_review: Vec::new(),
        warnings: Vec::new(),
        claim_traces: Vec::new(),
        payload: Payload::Comparison(ComparisonOutcome {
            diff: diff_terms(&terms, 0, 1, &excess_family()).rows,
            not_compared: diff_terms(&terms, 0, 1, &excess_family()).not_compared,
            terms,
        }),
    };

    let result = score_fixture(
        &fixture.name,
        &fixture.expected,
        &outcome,
        Perf {
            wall_ms: 0,
            model_ms: 0,
            tokens_per_second: 0.0,
            peak_rss_mb: 0,
        },
    );

    let mut rebound = Vec::new();
    for (item, want) in result.items.iter().zip(&fixture.expected.policy_terms) {
        let (Some(expect), ScoredDecision::Extraction { actual, .. }) =
            (&want.expect, &item.decision)
        else {
            continue;
        };
        let ExtractionOutcome::Found {
            extracted: Extracted::Term(got),
        } = actual
        else {
            continue;
        };
        if got.value != expect.value {
            rebound.push(format!(
                "{:?} states {} but was scored against {}",
                want.segment, expect.value, got.value
            ));
        }
    }

    assert!(
        rebound.is_empty(),
        "every passage is scored against its own reading: {rebound:#?}"
    );
}

/// #462: no phrase this family generates may be left implicitly
/// asserted.
///
/// `sections_repeat` was authored with genuine commercial-schedule
/// wording (#378) while its expectations were written as though that
/// wording were the personal-lines wording it replaced. Three
/// disagreements came out of the gap, one measurement at a time: a bare
/// `Excess:` carrying two different labels (#457), label-only quotes
/// breaking the eval join (#457), and every `Insurance amount:` line
/// routed to a person (#468). None of them was a decision anybody made.
/// The bed asserted by default, and the default was invisible.
///
/// So the family now carries a **sheet**: every sentence it generates,
/// in both voices, with the outcome the Phase 1 sitting decided for it
/// (10 August 2026). This test is the ratchet between the sheet and the
/// bytes on disk, and it bites in three directions:
///
/// 1. **A phrase with no row fails.** Add a sentence to the family and
///    it has no decided outcome until somebody writes one down.
/// 2. **A row nothing generates fails.** A decision about a sentence
///    the bed stopped writing is stale, and stale is how the sheet
///    would come to describe a bed that had moved out from under it.
/// 3. **A row whose disposition the bed contradicts fails.** The sheet
///    saying `cover_limit` while `expected.json` says otherwise is the
///    #462 defect exactly, and it is now a red test rather than a
///    measurement three weeks later.
#[test]
fn every_commercial_phrase_the_family_generates_has_a_decided_outcome() {
    let pack = load_pack(&pack_dir()).expect("pack loads");
    let fixtures = fixtures_in(&pack).expect("fixtures load");

    let mut used = vec![false; SECTIONS_REPEAT_SHEET.len()];
    let mut undecided: Vec<String> = Vec::new();
    let mut ambiguous: Vec<String> = Vec::new();
    let mut contradicted: Vec<String> = Vec::new();
    let mut scored = 0;

    for fixture in &fixtures {
        if !fixture.name.contains("sections_repeat") {
            continue;
        }
        for item in &fixture.expected.policy_terms {
            scored += 1;
            let matched: Vec<usize> = SECTIONS_REPEAT_SHEET
                .iter()
                .enumerate()
                .filter(|(_, phrase)| phrase.matches(&item.segment))
                .map(|(index, _)| index)
                .collect();
            match matched.as_slice() {
                [] => undecided.push(format!(
                    "{:?} in {} matches no row of the sheet",
                    item.segment, fixture.name
                )),
                [index] => {
                    used[*index] = true;
                    let sheet = &SECTIONS_REPEAT_SHEET[*index];
                    let authored = authored_disposition(item);
                    let decided = format!("{:?}", sheet.decided);
                    if authored != decided {
                        contradicted.push(format!(
                            "{:?}: the sheet decided {decided}, the bed authored {authored}",
                            item.segment
                        ));
                    }
                }
                several => ambiguous.push(format!(
                    "{:?} matches {} rows of the sheet: {:?}",
                    item.segment,
                    several.len(),
                    several
                        .iter()
                        .map(|index| SECTIONS_REPEAT_SHEET[*index].template)
                        .collect::<Vec<_>>()
                )),
            }
        }
    }

    assert!(
        scored >= 8 * 34,
        "the sheet is checked against the whole family, both voices: {scored} expectations seen"
    );
    assert!(
        undecided.is_empty(),
        "every phrase the family generates needs a decided outcome (#462): {undecided:#?}"
    );
    assert!(
        ambiguous.is_empty(),
        "a phrase decided twice is a phrase decided by whichever row was read first: {ambiguous:#?}"
    );
    assert!(
        contradicted.is_empty(),
        "the sheet and the bed disagree, which is #462's defect itself: {contradicted:#?}"
    );

    let stale: Vec<&str> = SECTIONS_REPEAT_SHEET
        .iter()
        .zip(&used)
        .filter(|(_, seen)| !**seen)
        .map(|(phrase, _)| phrase.template)
        .collect();
    assert!(
        stale.is_empty(),
        "a decision about a sentence the bed no longer writes is stale: {stale:#?}"
    );
}

/// What `expected.json` actually says about one passage, said in the
/// sheet's own vocabulary — so a disagreement can be shown rather than
/// merely detected. Kept as a string so no borrowed data has to be
/// leaked to be compared.
fn authored_disposition(item: &TermExpectation) -> String {
    if item.review {
        return "Refer".to_owned();
    }
    match &item.expect {
        None => "StatesNothing".to_owned(),
        Some(expect) => format!(
            "Read {{ term: {:?}, basis: {:?} }}",
            expect.term, expect.basis
        ),
    }
}

/// #461: a bare `Excess:` is a modelled value whose *label* is
/// unavailable, and the pack must say so rather than pick one.
///
/// This is the 8 August v12 reading, kept exactly: the identical
/// sentence answered `compulsory_excess` out of one document and
/// `total_excess` out of the other, across all three sections. The bed
/// was wrong to demand a determinate answer and #457 renamed the line —
/// but the sentence is real, commercial schedules write it, and a
/// person reading one cannot tell which excess it is either.
///
/// The model *told us* it was unsure, by answering two different ways.
/// Kettle threw that away and asserted whichever label came back, which
/// renders as two confident findings — an excess removed and a
/// different excess added — from one ambiguous line. Neither is true.
///
/// So the refusal is derived in Rust from the disagreement itself
/// (#258: the model keeps to closed questions), and it lands in
/// `not_compared` rather than `needs_review` for `run.rs`'s reason:
/// "the model could not answer" and "Rust declined to compare what the
/// model read perfectly well" are different facts about a run.
#[test]
fn an_unqualified_excess_is_referred_rather_than_labelled() {
    let reading = |document: usize, term: &str, value: &str| Term {
        term: term.to_owned(),
        basis: "per_claim".to_owned(),
        value: value.to_owned(),
        quote: format!("Excess: {value} each and every claim."),
        segment: format!("Excess: {value} each and every claim."),
        document,
        confidence: "high".to_owned(),
    };
    // One excess in each document, labelled differently. Nothing else
    // in either document names an excess, so there is no other reading
    // to tell the two apart by.
    let terms = vec![
        reading(0, "compulsory_excess", "£5,420.00"),
        reading(1, "total_excess", "£5,600.00"),
    ];

    let diff = diff_terms(&terms, 0, 1, &excess_family());

    let asserted: Vec<&str> = diff.rows.iter().map(|row| row.term.as_str()).collect();
    assert!(
        asserted.is_empty(),
        "one ambiguous line must not become two confident findings — an excess removed and \
         another added: {asserted:?}"
    );
    // Refused, not dropped: both readings reach a person with the words
    // they were read from, or the person is told nothing at all about
    // their excess, which is worse than being told to look.
    let quotes: Vec<&str> = diff
        .not_compared
        .iter()
        .flat_map(|refused| &refused.quotes)
        .map(String::as_str)
        .collect();
    assert!(
        quotes.contains(&"Excess: £5,420.00 each and every claim.")
            && quotes.contains(&"Excess: £5,600.00 each and every claim."),
        "every reading of the disagreed term reaches a person with its quote: {quotes:?}"
    );
}

/// The same sentence, labelled the *same* way in both documents — the
/// residual the mechanism cannot catch (#461's authoring condition).
///
/// A bare `Excess:` that both documents call a compulsory excess is
/// just as ambiguous, and produces no disagreement to derive a referral
/// from. Kettle compares it and says the excess rose by £180.00, which
/// may be true or may be a total excess being compared against a
/// compulsory one. This test does not assert that is *right*; it holds
/// the behaviour still and names it, so the bed family can size how
/// often it happens rather than the mechanism being assumed to cover a
/// class it only half covers.
#[test]
fn an_agreed_label_on_an_ambiguous_line_is_still_compared() {
    let reading = |document: usize, value: &str| Term {
        term: "compulsory_excess".to_owned(),
        basis: "per_claim".to_owned(),
        value: value.to_owned(),
        quote: format!("Excess: {value} each and every claim."),
        segment: format!("Excess: {value} each and every claim."),
        document,
        confidence: "high".to_owned(),
    };
    let terms = vec![reading(0, "£5,420.00"), reading(1, "£5,600.00")];

    let diff = diff_terms(&terms, 0, 1, &excess_family());

    assert_eq!(
        diff.rows.len(),
        1,
        "agreement leaves nothing to derive a refusal from: {:?}",
        diff.rows
    );
    assert!(
        diff.not_compared.is_empty(),
        "and nothing is referred: {:?}",
        diff.not_compared
    );
}

/// The excess family, as the renewal pack declares it.
fn excess_family() -> TermFamilies {
    load_pack(&pack_dir())
        .expect("pack loads")
        .manifest
        .term_families
        .clone()
}
