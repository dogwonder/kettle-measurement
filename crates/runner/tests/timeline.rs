//! #241: deadlines resolved in Rust. The model reads "within 14 days"
//! off the page and into structure; Rust checks the structure against
//! the words and counts (review of #626, Task 5). Every date below is
//! arithmetic the model never does (CLAUDE.md) — a date a model
//! invented is a missed deadline — and no line is searched for.

use chrono::NaiveDate;
use runner::claim::Kind;
use runner::document::{segments_from_text, Segment};
use runner::reading::Reading;
use runner::run::Obligation;
use runner::run::When;
use runner::timeline::{
    confirm_letter_date, date_dispute, letter_date, resolve_structured, sort_timeline, Unresolved,
};
use std::str::FromStr;

fn date(iso: &str) -> NaiveDate {
    NaiveDate::from_str(iso).expect("test date")
}

/// The structured resolver, one case per row of the review's Task 5
/// list. Every field is checked against the words whether or not
/// another is `none`; a period counts only from a base the model read.
fn when(count: u64, unit: &str, qualifier: &str, counts_from: &str) -> When {
    When::new(count, unit, qualifier, counts_from)
}

#[test]
fn a_correct_count_is_checked_against_its_words_and_then_counted() {
    let letter = Some(date("2026-03-03"));
    let worked = |iso: &str| {
        Ok(runner::timeline::Resolved {
            date: date(iso),
            kind: Kind::WorkedOut,
        })
    };
    assert_eq!(
        resolve_structured(
            "within 14 days",
            &when(14, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-17")
    );
    assert_eq!(
        resolve_structured(
            "within fourteen days",
            &when(14, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-17")
    );
    assert_eq!(
        resolve_structured(
            "within a fortnight",
            &when(14, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-17")
    );
    assert_eq!(
        resolve_structured(
            "within 2 weeks",
            &when(2, "weeks", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-17")
    );
    assert_eq!(
        resolve_structured(
            "within one month",
            &when(1, "months", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-04-03")
    );
    assert_eq!(
        resolve_structured(
            "within 14 calendar days",
            &when(14, "days", "calendar", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-17")
    );
    assert_eq!(
        resolve_structured(
            "by the end of the month",
            &when(0, "none", "none", "month_end"),
            false,
            letter,
            Kind::WorkedOut
        ),
        worked("2026-03-31")
    );
    // A leap February's end is arithmetic, not guesswork.
    assert_eq!(
        resolve_structured(
            "by the end of the month",
            &when(0, "none", "none", "month_end"),
            false,
            Some(date("2028-02-10")),
            Kind::WorkedOut
        )
        .map(|r| r.date),
        Ok(date("2028-02-29"))
    );
}

#[test]
fn a_contradictory_count_or_unit_is_refused_never_repaired() {
    let letter = Some(date("2026-03-03"));
    let contradicted = |r: Result<runner::timeline::Resolved, Unresolved>| {
        matches!(r, Err(Unresolved::Contradicted { .. }))
    };
    assert!(
        contradicted(resolve_structured(
            "within 14 days",
            &when(28, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "28 against fourteen"
    );
    assert!(
        contradicted(resolve_structured(
            "within 14 days",
            &when(14, "weeks", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "weeks against days"
    );
    assert!(
        contradicted(resolve_structured(
            "at your earliest convenience",
            &when(14, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "a period the words never gave"
    );
    assert!(
        contradicted(resolve_structured(
            "within 14 days",
            &when(0, "none", "none", "month_end"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "month end the words never gave"
    );
    // `unit == none` does not skip the other fields.
    assert!(
        contradicted(resolve_structured(
            "on 6 March 2026",
            &when(14, "none", "none", "none"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "a count with no unit"
    );
    assert!(
        contradicted(resolve_structured(
            "on 6 March 2026",
            &when(0, "none", "calendar", "none"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "a qualifier the words never gave"
    );
    assert!(
        contradicted(resolve_structured(
            "on 6 March 2026",
            &when(0, "none", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "a base with nothing to count"
    );
    // The qualifier is checked as its word, both ways.
    assert!(
        contradicted(resolve_structured(
            "within 14 working days",
            &when(14, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "working days the reading left out"
    );
    assert!(
        contradicted(resolve_structured(
            "within 14 days",
            &when(14, "days", "working", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "working days the words never gave"
    );
    // And receipt.
    assert!(
        contradicted(resolve_structured(
            "within 28 days of receipt",
            &when(28, "days", "none", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "receipt the reading left out"
    );
}

#[test]
fn an_unsupported_computation_keeps_the_words_and_derives_nothing() {
    let letter = Some(date("2026-03-03"));
    let unsupported = |r: Result<runner::timeline::Resolved, Unresolved>| {
        matches!(r, Err(Unresolved::Unsupported { .. }))
    };
    assert!(
        unsupported(resolve_structured(
            "within 14 working days",
            &when(14, "days", "working", "letter_date"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "working days need a calendar Kettle does not have"
    );
    assert!(
        unsupported(resolve_structured(
            "within 28 days of receipt",
            &when(28, "days", "none", "receipt"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "receipt is a day the letter does not state"
    );
    assert!(
        unsupported(resolve_structured(
            "as soon as possible",
            &when(0, "none", "none", "none"),
            false,
            letter,
            Kind::WorkedOut
        )),
        "no day Kettle can read"
    );
}

#[test]
fn a_period_counts_only_from_a_base_the_model_read() {
    // An explicit named day: counted from it, worked out.
    assert_eq!(
        resolve_structured(
            "within 30 days of 22 May 2026",
            &when(30, "days", "none", "named_date"),
            false,
            Some(date("2026-05-22")),
            Kind::WorkedOut
        )
        .map(|r| (r.date, r.kind)),
        Ok((date("2026-06-21"), Kind::WorkedOut))
    );
    // The day the words themselves name is the base, whatever `from`
    // says: the words are the verified reading, and the 4B hands the
    // dateline over as `from` on every such letter.
    assert_eq!(
        resolve_structured(
            "within 30 days of 22 May 2026",
            &when(30, "days", "none", "named_date"),
            false,
            Some(date("2026-03-03")),
            Kind::WorkedOut
        )
        .map(|r| r.date),
        Ok(date("2026-06-21")),
        "the dateline handed over as the named day is ignored for the day the words print"
    );
    // Month end read with a unit of months and no count is the word
    // *month* the words give, not a period.
    assert_eq!(
        resolve_structured(
            "by the end of the month",
            &when(0, "months", "none", "month_end"),
            false,
            Some(date("2026-03-03")),
            Kind::WorkedOut
        )
        .map(|r| r.date),
        Ok(date("2026-03-31"))
    );
    // An absent base: undated, never borrowed from the letter.
    let no_base = |r: Result<runner::timeline::Resolved, Unresolved>| {
        matches!(r, Err(Unresolved::NoBase { .. }))
    };
    assert!(
        no_base(resolve_structured(
            "within 14 days",
            &when(14, "days", "none", "letter_date"),
            false,
            None,
            Kind::WorkedOut
        )),
        "the letter's date was not read"
    );
    assert!(
        no_base(resolve_structured(
            "within 14 days",
            &when(14, "days", "none", "named_date"),
            false,
            None,
            Kind::WorkedOut
        )),
        "the named day was not read"
    );
    assert!(
        no_base(resolve_structured(
            "within 14 days",
            &when(14, "days", "none", "none"),
            false,
            Some(date("2026-03-03")),
            Kind::WorkedOut
        )),
        "counts_from none counts from nothing, even with a letter date to hand"
    );
    assert!(no_base(resolve_structured(
        "by the end of the month",
        &when(0, "none", "none", "month_end"),
        false,
        None,
        Kind::WorkedOut
    )));
}

#[test]
fn a_day_the_words_name_is_read_and_a_row_pointed_at_is_read_at_the_row() {
    let read = |iso: &str| {
        Ok(runner::timeline::Resolved {
            date: date(iso),
            kind: Kind::ReadAndVerified,
        })
    };
    assert_eq!(
        resolve_structured(
            "by 12 August 2026",
            &when(0, "none", "none", "none"),
            false,
            None,
            Kind::WorkedOut
        ),
        read("2026-08-12")
    );
    assert_eq!(
        resolve_structured(
            "on 3 March 2026",
            &when(0, "none", "none", "none"),
            false,
            None,
            Kind::WorkedOut
        ),
        read("2026-03-03")
    );
    // A pointing ask's deadline is the date its row prints, verified
    // at the row and read from it.
    assert_eq!(
        resolve_structured(
            "6 March 2026",
            &when(0, "none", "none", "none"),
            true,
            None,
            Kind::WorkedOut
        ),
        read("2026-03-06")
    );
    // A date inside a relative phrase is the base, never the answer.
    assert_eq!(
        resolve_structured(
            "within 30 days of 22 May 2026",
            &when(30, "days", "none", "named_date"),
            false,
            Some(date("2026-05-22")),
            Kind::WorkedOut
        )
        .map(|r| r.date),
        Ok(date("2026-06-21"))
    );
}

#[test]
fn a_date_a_person_confirmed_is_theirs_and_only_where_it_counted() {
    let given = date("2026-03-03");
    assert_eq!(
        resolve_structured(
            "within 14 days",
            &when(14, "days", "none", "letter_date"),
            false,
            Some(given),
            Kind::Yours
        )
        .map(|r| (r.date, r.kind)),
        Ok((date("2026-03-17"), Kind::Yours))
    );
    // A day written on the page never depended on their answer.
    assert_eq!(
        resolve_structured(
            "by 12 August 2026",
            &when(0, "none", "none", "none"),
            false,
            Some(given),
            Kind::Yours
        )
        .map(|r| r.kind),
        Ok(Kind::ReadAndVerified)
    );
}

fn segment(ordinal: usize, text: &str) -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    }
}

/// A document that dates itself, as `sort_timeline` now reads it: the
/// date is a passage of the letter rather than a value handed in
/// beside it, because a pointing deadline needs the whole document and
/// a date on its own could never have carried the table (#544).
fn letter_dated(written: &str) -> Vec<Segment> {
    vec![segment(0, written)]
}

/// What a bed author knows about the phrases these tests use — the
/// structure the model would read, authored here rather than parsed.
fn structure_of(deadline: &str, anchor: &str) -> When {
    let named = runner::timeline::first_full_date(anchor).is_some();
    if let Some(days) = deadline
        .strip_prefix("within ")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
    {
        return When::new(
            days,
            "days",
            "none",
            if named { "named_date" } else { "letter_date" },
        );
    }
    if deadline.contains("end of the month") {
        return When::new(0, "none", "none", "month_end");
    }
    When::default()
}

/// The base a period counts from, as the reading the model would give:
/// the named day where the anchor is one, else the test letter's own
/// dateline at passage 0 — `letter_dated` — which the tests that need
/// it fill in with `dated`.
fn base_of(anchor: &str, own: usize) -> Reading {
    if runner::timeline::first_full_date(anchor).is_some() {
        Reading::new(own, anchor)
    } else {
        Reading::absent(own)
    }
}

/// Give an obligation counted from the letter's date the dateline
/// reading `letter_dated` puts at passage 0.
fn dated(mut obligation: Obligation, written: &str) -> Obligation {
    if matches!(
        obligation.read.counts_from.as_str(),
        "letter_date" | "month_end"
    ) {
        obligation.from = Reading::new(0, written);
    }
    obligation
}

fn obligation(deadline: &str, anchor: &str, evidence: Segment) -> Obligation {
    Obligation {
        kind: "payment".to_owned(),
        party: runner::reading::Reading::new(
            evidence.ordinal,
            "Harborne Parking Services".to_owned(),
        ),
        ask: "Pay £120.00".to_owned(),
        deadline: runner::reading::Reading::new(evidence.ordinal, deadline.to_owned()),
        read: structure_of(deadline, anchor),
        from: base_of(anchor, evidence.ordinal),
        unresolved: None,
        amount: runner::reading::Reading::absent(evidence.ordinal),
        refused: Vec::new(),
        confidence: "high".to_owned(),
        due: None,
        evidence: vec![evidence],
        dated_by: None,
        priced_by: None,
        shown: Default::default(),
        disputed: vec![],
    }
}

/// Two passages that say one ask are two readings, and both are shown
/// (review of #626, Task 2; `app/METHOD.md` §1.4). Until now four
/// strings — kind, party, deadline, anchor — merged them into one,
/// which is a judgement about meaning that no page can verify: it
/// compared neither the ask nor the sum, so two invoices to one payee
/// by one date became one obligation keeping the first sum. A
/// duplicate is a glance; a merge is a loss.
#[test]
fn a_repeated_ask_is_shown_from_each_passage_that_made_it() {
    let first = obligation(
        "within 14 days",
        "the date of this letter",
        segment(
            1,
            "Please pay £120.00 within 14 days of the date of this letter.",
        ),
    );
    let second = obligation(
        "within 14 days",
        "the date of this letter",
        segment(
            3,
            "We remind you that payment of £120.00 is due within 14 days.",
        ),
    );

    let sorted = sort_timeline(
        vec![dated(first, "3 March 2026"), dated(second, "3 March 2026")],
        &letter_dated("3 March 2026"),
    );

    assert_eq!(
        sorted.len(),
        2,
        "one ask, said twice, shown twice: {sorted:?}"
    );
    for obligation in &sorted {
        assert_eq!(obligation.due.map(|d| d.date), Some(date("2026-03-17")));
        assert_eq!(obligation.evidence.len(), 1, "each keeps its own passage");
    }
    let ordinals: Vec<usize> = sorted.iter().map(|o| o.evidence[0].ordinal).collect();
    assert_eq!(ordinals, vec![1, 3], "page order breaks the tie");
}

/// The same organisation, the same day, two invoices: two sums, two
/// obligations, and neither sum is lost.
#[test]
fn two_invoices_to_one_payee_by_one_date_stay_two_obligations() {
    let mut a = obligation("by 30 April 2026", "30 April 2026", segment(2, "Invoice A"));
    a.ask = "Pay invoice A".to_owned();
    a.amount.value = "£80.00".to_owned();
    let mut b = obligation("by 30 April 2026", "30 April 2026", segment(4, "Invoice B"));
    b.ask = "Pay invoice B".to_owned();
    b.amount.value = "£120.00".to_owned();

    let sorted = sort_timeline(vec![a, b], &letter_dated("3 March 2026"));

    let amounts: Vec<&str> = sorted.iter().map(|o| o.amount.value.as_str()).collect();
    assert_eq!(amounts, vec!["£80.00", "£120.00"], "{sorted:#?}");
}

/// Equal sums are not one invoice either.
#[test]
fn two_invoices_for_the_same_sum_stay_two_obligations() {
    let mut a = obligation("by 30 April 2026", "30 April 2026", segment(2, "Invoice A"));
    a.amount.value = "£80.00".to_owned();
    let mut b = obligation("by 30 April 2026", "30 April 2026", segment(4, "Invoice B"));
    b.amount.value = "£80.00".to_owned();

    let sorted = sort_timeline(vec![a, b], &letter_dated("3 March 2026"));
    assert_eq!(sorted.len(), 2, "{sorted:#?}");
}

/// Two things to send back by one date are two responses.
#[test]
fn two_responses_to_one_party_by_one_date_stay_two_obligations() {
    let mut form = obligation(
        "within 14 days",
        "the date of this letter",
        segment(2, "form"),
    );
    form.kind = "response".to_owned();
    form.ask = "Return the signed form".to_owned();
    let mut id = obligation(
        "within 14 days",
        "the date of this letter",
        segment(3, "id"),
    );
    id.kind = "response".to_owned();
    id.ask = "Send photo ID".to_owned();

    let sorted = sort_timeline(vec![form, id], &letter_dated("3 March 2026"));
    let asks: Vec<&str> = sorted.iter().map(|o| o.ask.as_str()).collect();
    assert_eq!(asks, vec!["Return the signed form", "Send photo ID"]);
}

/// The one duplicate Rust may still fold: the same candidate, word for
/// word, read twice out of the same passage — an execution artefact,
/// not a second ask on the page.
#[test]
fn the_same_candidate_from_the_same_passage_is_one() {
    let passage = segment(
        1,
        "Please pay £120.00 within 14 days of the date of this letter.",
    );
    let once = obligation("within 14 days", "the date of this letter", passage.clone());
    let twice = obligation("within 14 days", "the date of this letter", passage);

    let sorted = sort_timeline(vec![once, twice], &letter_dated("3 March 2026"));
    assert_eq!(sorted.len(), 1, "{sorted:#?}");
    assert_eq!(sorted[0].evidence.len(), 1);
}

/// A low-confidence reading keeps its own confidence and its own
/// passage: it is routed for checking on its own, not folded into a
/// confident twin.
#[test]
fn a_less_confident_repeat_is_shown_at_its_own_confidence() {
    let confident = obligation("within 14 days", "the date of this letter", segment(1, "a"));
    let mut unsure = obligation("within 14 days", "the date of this letter", segment(2, "b"));
    unsure.confidence = "low".to_owned();

    let sorted = sort_timeline(vec![confident, unsure], &letter_dated("3 March 2026"));
    assert_eq!(sorted.len(), 2);
    assert_eq!(sorted[0].confidence, "high");
    assert_eq!(sorted[0].evidence[0].ordinal, 1);
    assert_eq!(sorted[1].confidence, "low");
    assert_eq!(sorted[1].evidence[0].ordinal, 2);
}

#[test]
fn the_timeline_is_date_ordered_with_undated_obligations_surviving_last() {
    let august = {
        let mut o = obligation("by 12 August 2026", "12 August 2026", segment(4, "confirm"));
        o.kind = "response".to_owned();
        o
    };
    let march = dated(
        obligation(
            "within 14 days",
            "the date of this letter",
            segment(1, "pay"),
        ),
        "3 March 2026",
    );
    let vague = {
        let mut o = obligation(
            "when convenient",
            "no particular date",
            segment(6, "call us"),
        );
        o.kind = "other".to_owned();
        o
    };

    let sorted = sort_timeline(vec![august, vague, march], &letter_dated("3 March 2026"));

    assert_eq!(sorted.len(), 3, "nothing is silently dropped: {sorted:?}");
    assert_eq!(
        sorted[0].due.map(|d| d.date),
        Some(date("2026-03-17")),
        "soonest first"
    );
    assert_eq!(sorted[1].due.map(|d| d.date), Some(date("2026-08-12")));
    assert_eq!(
        sorted[2].due, None,
        "an unresolvable deadline keeps its phrase and stays visible"
    );
    assert_eq!(sorted[2].deadline.value, "when convenient");
}

/// The kind survives the sort (#366, #367).
///
/// `resolve_deadline` knows which branch it took, and the timeline is
/// the only place that knows it — by the time a report renders, the
/// phrase and the date are all that is left. Dropping the kind here is
/// how #366 decays into a sentence in the copy layer: the template
/// would have to guess, and a template that guesses "worked out" over a
/// date quoted off the page is the false assurance #367 exists to stop.
#[test]
fn a_sorted_obligation_still_says_how_its_date_was_arrived_at() {
    let counted = dated(
        obligation(
            "within 14 days",
            "the date of this letter",
            segment(1, "pay"),
        ),
        "3 March 2026",
    );
    let written = {
        let mut o = obligation("by 12 August 2026", "12 August 2026", segment(4, "confirm"));
        o.kind = "response".to_owned();
        o
    };

    let sorted = sort_timeline(vec![written, counted], &letter_dated("3 March 2026"));

    assert_eq!(sorted[0].due.map(|d| d.kind), Some(Kind::WorkedOut));
    assert_eq!(sorted[1].due.map(|d| d.kind), Some(Kind::ReadAndVerified));
}

#[test]
fn without_a_letter_date_relative_deadlines_stay_undated() {
    // No anchor to count from is not a licence to invent one.
    let relative = obligation(
        "within 14 days",
        "the date of this letter",
        segment(1, "pay"),
    );
    let sorted = sort_timeline(vec![relative], &[]);
    assert_eq!(sorted[0].due, None);
}

#[test]
fn the_letter_date_is_the_first_full_date_the_document_shows() {
    let segments = vec![
        segment(0, "Harborne Parking Services\nReference 4821"),
        segment(1, "3 March 2026"),
        segment(2, "Please pay £120.00 by 12 August 2026."),
    ];
    assert_eq!(letter_date(&segments), Some(date("2026-03-03")));

    let undated = vec![segment(0, "Dear Mr Henderson,")];
    assert_eq!(letter_date(&undated), None);
}

#[test]
fn a_letter_date_below_an_address_block_is_still_the_letter_date() {
    // #401's second consequence. "The opening of the document" was
    // counted in segments, and three of them was the whole header while
    // a header was one segment. Now that the line rhythm segments text
    // (`document::stopped_short`), an address block is five or six
    // segments and the date sits below all of them — where a window
    // counted in segments cannot see it.
    //
    // The cost is not a missing date on a report. `resolve` falls back
    // to the letter date for every relative deadline, so losing it
    // silently undates "within 14 days of the date of this letter" —
    // the commonest deadline there is, on the letters people actually
    // get.
    //
    // The window must therefore be measured in something segmentation
    // cannot move.
    let segments = vec![
        segment(0, "Ashgrove Housing Association"),
        segment(1, "Pennine House"),
        segment(2, "12 Bramley Road"),
        segment(3, "Manchester"),
        segment(4, "M14 5QT"),
        segment(5, "3 March 2026"),
        segment(6, "Dear Ms Okafor"),
        segment(
            7,
            "Please pay £120.00 within 14 days of the date of this letter.",
        ),
    ];

    assert_eq!(letter_date(&segments), Some(date("2026-03-03")));
}

#[test]
fn a_date_inside_a_sentence_does_not_date_the_letter() {
    // #578, found on the #431 study corpus by a person sitting the
    // study, not by a test. `planned_works_notice-023` carries no date
    // of its own; its only date is the works commencement, written mid
    // sentence. The opening window took it as the letter's date, and
    // "within 28 days" then resolved to 20 May 2026 — a deadline the
    // letter never set, asserted as "asked for this by 20 May 2026",
    // exported as a calendar reminder, with an honest quote beside it.
    //
    // Every quote guardrail passed, because the invention was in the
    // anchor and no quote rule looks there.
    let sentence = vec![
        segment(0, "Harrowdene Housing Association"),
        segment(1, "Our reference: HX539621"),
        segment(2, "Dear Pamela,"),
        segment(
            3,
            "We are writing to let you know that external redecoration and gutter \
             replacement will begin at your building on 22 April 2026.",
        ),
    ];
    assert_eq!(letter_date(&sentence), None);

    // And so the ask that counts from it stays undated, which is the
    // honest answer: 28 days from nothing is nothing.
    let ask = obligation("within 28 days", "", sentence[3].clone());
    let sorted = sort_timeline(vec![ask], &sentence);
    assert_eq!(sorted[0].due, None);
}

#[test]
fn a_letter_dates_itself_the_way_letters_actually_write_dates() {
    // Found on the first real photographed letter (#399): a housing
    // association dated it "Thursday 28th April 2022", and Kettle read
    // no date at all. `28th` is not a number to `str::parse`, so the
    // ordinal suffix — which is how British correspondence writes a
    // date — silently defeated the whole search.
    //
    // The cost is not a missing field. Without the letter date every
    // "within 14 days of the date of this letter" stays undated, and
    // undated reads as "Kettle could not work this out", which is
    // indistinguishable from the letter never having said.
    let ordinal = vec![segment(0, "Thursday 28th April 2022")];
    assert_eq!(letter_date(&ordinal), Some(date("2022-04-28")));

    // The other three suffixes, which appear on nine days in ten.
    for (written, iso) in [
        ("1st May 2026", "2026-05-01"),
        ("2nd June 2026", "2026-06-02"),
        ("3rd June 2026", "2026-06-03"),
        ("21st December 2026", "2026-12-21"),
    ] {
        assert_eq!(
            letter_date(&[segment(0, written)]),
            Some(date(iso)),
            "{written}"
        );
    }

    // Unchanged: a plain date still reads, and a reference number that
    // happens to end in letters is not a day.
    assert_eq!(
        letter_date(&[segment(0, "3 March 2026")]),
        Some(date("2026-03-03"))
    );
    assert_eq!(letter_date(&[segment(0, "No. 18521R April 2022")]), None);
}

#[test]
fn a_letter_dating_itself_all_numerically_is_read_where_its_digits_settle_the_order() {
    // #613. The first real letter through the packaged app dated itself
    // `20/08/2026`, alone on its line, read at confidence 1.000 — and
    // was refused, because all-numeric British and American forms
    // cannot be told apart. So "within 7 calendar days" had no anchor
    // and the ask showed as *Not stated*.
    //
    // A day over twelve is not ambiguous: `20/08/2026` cannot be month
    // twenty, whichever side of the Atlantic printed it. That half is
    // read. The other half — both fields twelve or under — keeps
    // refusing, because guessing wrong there moves every deadline in
    // the letter by up to eleven months.
    for written in [
        "20/08/2026",
        "Date: 20/08/2026",
        "20.08.2026",
        "20-08-2026",
        "08/20/2026",
    ] {
        assert_eq!(
            letter_date(&[segment(0, written)]),
            Some(date("2026-08-20")),
            "{written}"
        );
    }
    // Still refused: nothing on the page settles the order.
    assert_eq!(letter_date(&[segment(0, "06/03/2026")]), None);
    assert_eq!(letter_date(&[segment(0, "Date: 3/6/2026")]), None);
    // A two-digit year names no century.
    assert_eq!(letter_date(&[segment(0, "20/08/26")]), None);
    // A reference number is not a date.
    assert_eq!(
        letter_date(&[segment(0, "Our ref: 3001-249696-11463")]),
        None
    );

    // The point of reading it: the relative deadline now resolves.
    let letter = vec![segment(0, "20/08/2026")];
    // The model reads the dateline as the base and its structure as
    // seven calendar days from it; Rust checks both and counts.
    let mut ask = obligation(
        "within 7 calendar days",
        "no particular date",
        letter[0].clone(),
    );
    ask.read = When::new(7, "days", "calendar", "letter_date");
    ask.from = Reading::new(0, "20/08/2026");
    let resolved = sort_timeline(vec![ask], &letter);
    assert_eq!(
        resolved[0].due.as_ref().map(|r| r.date),
        Some(date("2026-08-27"))
    );
}

#[test]
fn two_readings_that_date_the_letter_differently_are_a_dispute() {
    // #412, step 4. Every relative deadline in a letter is counted from
    // the letter's own date, so one wrong digit here moves every date in
    // the report — and it moves them to dates that look worked out and
    // certain. This is the one thing worth stopping a person for.
    //
    // Derived from the two readings rather than from the disputed
    // lines, because it is the *date* that matters and not the line: a
    // dispute over a word in the dateline that leaves the date itself
    // unchanged is not worth anybody's time.
    let applied = vec![segment(0, "Anytown Council"), segment(1, "3 March 2026")];
    let literal = vec![segment(0, "Anytown Council"), segment(1, "8 March 2026")];

    let dispute = date_dispute(&applied, &literal).expect("the readings differ");
    assert_eq!(dispute.read, Some(date("2026-03-03")));
    assert_eq!(dispute.also_read, Some(date("2026-03-08")));
}

#[test]
fn readings_that_agree_about_the_date_are_not_a_dispute() {
    // The common case is silent, or the step is the click-through gate
    // #412 exists to avoid. The two readings differ here — one dropped
    // a space, the way the literal pass actually does — and still agree
    // about the date, which is all that is being asked.
    let applied = vec![segment(0, "Dated 28th April 2026")];
    let literal = vec![segment(0, "Dated 28thApril 2026")];

    assert_eq!(date_dispute(&applied, &literal), None);
}

#[test]
fn a_date_only_one_reading_found_is_a_dispute() {
    // Not "fall back to whichever pass found one". A date one reading
    // saw and the other did not is unconfirmed, and a letter date is
    // too load-bearing to accept unconfirmed — the alternative is
    // asserting every deadline in the letter off a single unverified
    // reading.
    let applied = vec![segment(0, "Dated 28th April 2026")];
    let literal = vec![segment(0, "Dated 28th Apri1 2026")];

    let dispute = date_dispute(&applied, &literal).expect("only one reading found a date");
    assert_eq!(dispute.read, Some(date("2026-04-28")));
    assert_eq!(dispute.also_read, None);
}

#[test]
fn two_undated_readings_are_not_a_dispute() {
    // Both agree there is no date. That is a letter that never dated
    // itself — an ordinary, scored case for this pack — and not
    // something to stop a person over.
    let applied = vec![segment(0, "Dear Ms Okafor")];
    let literal = vec![segment(0, "Dear Ms Okafor")];

    assert_eq!(date_dispute(&applied, &literal), None);
}

#[test]
fn confirming_a_date_resolves_the_deadlines_that_waited_on_it() {
    // #412 step 4, applied. The run has already happened: the model
    // read "within 14 days" off the page, and the deadline stayed
    // undated because the two readings disagreed about the letter's own
    // date. A person settles it, and the deadline resolves — without
    // asking the model anything again, because nothing it answered
    // depended on the date.
    let waiting = obligation(
        "within 14 days",
        "the date of this letter",
        segment(1, "Please pay £120.00 within 14 days."),
    );

    let settled = confirm_letter_date(vec![waiting], 0, date("2026-03-03"));

    assert_eq!(settled.len(), 1);
    let due = settled[0].due.expect("the deadline resolves once dated");
    assert_eq!(due.date, date("2026-03-17"));
    assert_eq!(due.kind, Kind::Yours, "counted from the date they gave");
}

#[test]
fn confirming_one_letters_date_leaves_another_letters_alone() {
    // A run may hold several letters (#330), and each has its own date.
    // An answer about one must not silently re-date the obligations of
    // another — the person was shown one letter's passage and asked
    // about that.
    let mine = obligation("within 14 days", "the date of this letter", {
        let mut s = segment(1, "Please pay £120.00 within 14 days.");
        s.document = 0;
        s
    });
    let theirs = obligation("within 14 days", "the date of this letter", {
        let mut s = segment(1, "Please reply within 14 days.");
        s.document = 1;
        s
    });

    let settled = confirm_letter_date(vec![mine, theirs], 0, date("2026-03-03"));

    assert_eq!(settled[0].due.map(|d| d.date), Some(date("2026-03-17")));
    assert_eq!(
        settled[1].due, None,
        "the second letter's date was never confirmed, so its deadline stays undated"
    );
}

#[test]
fn a_date_deep_in_the_body_is_not_the_letter_date() {
    // The other half, and the reason the answer is not "search the
    // whole document": a date in the body is usually somebody else's —
    // an appointment, a payment already made, a period being described.
    // Taking one as the document's own date would anchor every relative
    // deadline to it and state the results as resolved facts.
    let mut segments = vec![segment(0, "Dear Ms Okafor")];
    for ordinal in 1..12 {
        segments.push(segment(
            ordinal,
            "We are writing about the works to your building, which the \
             contractor has now scheduled and which will affect access to \
             the rear entrance for a short period.",
        ));
    }
    segments.push(segment(12, "Your visit of 12 January was noted."));

    assert_eq!(letter_date(&segments), None);
}

/// The mortise-02 letter, segmented as a run segments it.
///
/// Read from the committed fixture rather than retyped: what is under
/// test is the passage a person's letter actually produces, and the
/// alignment that makes the table a table is exactly what a retyped
/// literal loses.
fn mortise_02() -> Vec<Segment> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../packs/app.kttl.letter-to-actions/fixtures/\
generated-development-invoice_totals-mortise-02.txt",
    );
    segments_from_text(&std::fs::read_to_string(path).expect("the mortise-02 letter"))
}

/// The passage that defers to the layout instead of restating it.
fn pointing_passage(segments: &[Segment]) -> Segment {
    segments
        .iter()
        .find(|segment| segment.text.starts_with("Please find our invoice"))
        .expect("the passage that points at the table")
        .clone()
}

/// The due-date row the pointing passage points at.
fn due_date_row(segments: &[Segment]) -> Segment {
    segments
        .iter()
        .find(|segment| segment.text.starts_with("Due date"))
        .expect("the due-date row")
        .clone()
}

/// #544 on the reading shape (review of #626, Task 5): an ask that
/// points at the page — "Payment of the total is due by the date shown
/// beside it" — has for its deadline the date the row prints, read at
/// that row. The ask is still scored where it was made: `evidence` is
/// the prose passage, the row travels as `dated_by`, and the date is
/// read-and-verified because nothing was computed. No direction word
/// list decides any of it; the model named the row and the page
/// vouched for the date.
#[test]
fn a_pointing_deadline_is_the_date_its_row_prints_read_at_the_row() {
    let segments = mortise_02();
    let prose = pointing_passage(&segments);
    let row = due_date_row(&segments);
    let mut pointing = obligation("6 March 2026", "no particular date", prose.clone());
    pointing.deadline = Reading::new(row.ordinal, "6 March 2026");

    let sorted = sort_timeline(vec![pointing], &segments);

    assert_eq!(
        sorted[0].due.map(|r| (r.date, r.kind)),
        Some((date("2026-03-06"), Kind::ReadAndVerified)),
        "{sorted:#?}"
    );
    let dated_by = sorted[0]
        .dated_by
        .as_ref()
        .expect("the row travels with the claim");
    assert!(dated_by.text.contains("6 March 2026"), "{dated_by:#?}");
    let quoted: Vec<&str> = sorted[0].evidence.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        quoted,
        vec![prose.text.as_str()],
        "the passage the model answered about is the only one it asserted on"
    );
}

#[test]
fn a_pointing_ask_with_no_row_to_point_at_keeps_its_words_and_stays_undated() {
    let segments = vec![
        segment(0, "6 February 2026"),
        segment(
            1,
            "Payment of the total is due by the date shown beside it.",
        ),
        segment(2, "We wrote to you about this on 3 January 2026."),
    ];
    // Nothing to point at, so the model copies the words at the ask's
    // own passage: a day Kettle cannot read, kept as words.
    let sorted = sort_timeline(
        vec![obligation(
            "by the date shown beside it",
            "no particular date",
            segments[1].clone(),
        )],
        &segments,
    );
    assert_eq!(sorted[0].due, None, "{sorted:#?}");
    assert_eq!(sorted[0].deadline.value, "by the date shown beside it");
    assert!(
        matches!(sorted[0].unresolved, Some(Unresolved::Unsupported { .. })),
        "{:?}",
        sorted[0].unresolved
    );
}

#[test]
fn a_deadline_that_names_its_date_is_read_at_its_own_passage() {
    let segments = vec![
        segment(0, "6 February 2026"),
        segment(1, "Please confirm in writing by 20 February 2026."),
        segment(2, "Due date 6 March 2026"),
    ];
    let sorted = sort_timeline(
        vec![obligation(
            "by 20 February 2026",
            "20 February 2026",
            segments[1].clone(),
        )],
        &segments,
    );
    assert_eq!(
        sorted[0].due.map(|r| r.date),
        Some(date("2026-02-20")),
        "{sorted:#?}"
    );
    assert!(sorted[0].dated_by.is_none(), "its own passage, not a row");
}

/// The sum was verified at the passage `amount.at` at read time; the
/// sort carries that passage as `priced_by` where it is not the ask's
/// own. Nothing is searched for where none was read (the amount finder
/// is gone; review of #626, Task 5).
#[test]
fn a_sum_read_at_a_row_travels_with_the_claim_and_an_absent_one_stays_absent() {
    let ask = segment(
        2,
        "Payment of the total is due by the date shown beside it.",
    );
    let segments = vec![
        segment(0, "6 February 2026"),
        segment(1, "Belwood Joinery"),
        ask.clone(),
        segment(3, "Total £360.00"),
    ];

    let mut priced = obligation("within 14 days", "the date of this letter", ask.clone());
    priced.amount = Reading::new(3, "£360.00");
    let priced = sort_timeline(vec![priced], &segments);
    assert_eq!(priced[0].amount.value, "£360.00");
    assert_eq!(priced[0].priced_by.as_ref().map(|s| s.ordinal), Some(3));

    let mut own = obligation("within 14 days", "the date of this letter", ask.clone());
    own.amount = Reading::new(2, "£120.00");
    let own = sort_timeline(vec![own], &segments);
    assert!(own[0].priced_by.is_none(), "its own passage is not a row");

    let absent = sort_timeline(
        vec![obligation("within 14 days", "the date of this letter", ask)],
        &segments,
    );
    assert!(
        absent[0].amount.is_absent(),
        "no sum read, none found: {:?}",
        absent[0].amount
    );
    assert!(absent[0].priced_by.is_none());
}

#[test]
fn a_claim_may_name_the_passage_its_value_lives_in_and_rust_checks_it() {
    let ask = segment(
        14,
        "Unless payment of all overdue invoices is received within 7 calendar days, we \
         may commence legal action.",
    );
    // No labels the finder knows: the row says "Sum owing".
    let segments: Vec<Segment> = (0..20)
        .map(|n| match n {
            9 => segment(9, "Sum owing 41.21 GBP 009422"),
            11 => segment(11, "Pay by 27/08/2026"),
            14 => ask.clone(),
            _ => segment(n, "Lorem ipsum."),
        })
        .collect();

    // The whole page was one batch: every id below is one the model
    // was shown (#624).
    let shown = |mut obligation: Obligation| {
        obligation.shown = (0..20).collect();
        obligation
    };
    // Named and verified: the figure is in the named passage.
    let mut named = shown(obligation(
        "within 7 calendar days",
        "no particular date",
        ask.clone(),
    ));
    named.amount = runner::reading::Reading::new(9, "41.21 GBP");
    let named = sort_timeline(vec![named], &segments)[0].clone();
    assert_eq!(named.amount.value, "41.21 GBP");
    assert_eq!(named.priced_by.as_ref().map(|s| s.ordinal), Some(9));

    // Named and wrong: the page refused the reading at read time
    // (`reading::check`, tests/readings.rs), so the sort sees an absent
    // sum with the refusal beside it — and does not go hunting for one.
    let mut wrong = shown(obligation(
        "within 7 calendar days",
        "no particular date",
        ask.clone(),
    ));
    wrong.amount = runner::reading::Reading::absent(14);
    wrong.refused = vec![runner::run::RefusedReading {
        field: "amount".to_owned(),
        at: 11,
        value: "41.21 GBP".to_owned(),
        why: "amount's value is not in passage 11: refused".to_owned(),
    }];
    let wrong = sort_timeline(vec![wrong], &segments)[0].clone();
    assert!(wrong.amount.is_absent(), "{:?}", wrong.amount);
    assert!(wrong.priced_by.is_none());

    // A deadline whose date is printed elsewhere: the model names the
    // passage, Rust reads one full date from it, read-and-verified.
    let mut pointed = shown(obligation(
        "by the date below",
        "no particular date",
        ask.clone(),
    ));
    pointed.amount = runner::reading::Reading::absent(14);
    pointed.deadline = runner::reading::Reading::new(11, "27/08/2026");
    let pointed = sort_timeline(vec![pointed], &segments)[0].clone();
    assert_eq!(
        pointed.due.as_ref().map(|r| (r.date, r.kind)),
        Some((date("2026-08-27"), Kind::ReadAndVerified))
    );
    assert_eq!(pointed.dated_by.as_ref().map(|s| s.ordinal), Some(11));
    assert_eq!(pointed.evidence.len(), 1);

    // Naming its own passage is not naming another: nothing to carry.
    let mut own = shown(obligation(
        "within 7 calendar days",
        "no particular date",
        ask.clone(),
    ));
    own.amount = runner::reading::Reading::absent(14);
    let own = sort_timeline(vec![own], &segments)[0].clone();
    assert!(own.priced_by.is_none());
}

/// A sum is a payment's alone (6 September 2026): a response ask
/// carrying the letter's sum keeps its ask and loses the figure.
#[test]
fn a_sum_on_a_response_ask_is_dropped_as_policy() {
    let segments = vec![
        segment(0, "3 March 2026"),
        segment(1, "Please pay £480.00 within 14 days."),
        segment(2, "Please return the slip within 28 days."),
    ];
    let mut reply = dated(
        obligation(
            "within 28 days",
            "the date of this letter",
            segments[2].clone(),
        ),
        "3 March 2026",
    );
    reply.kind = "response".to_owned();
    reply.amount = Reading::new(1, "£480.00");
    let sorted = sort_timeline(vec![reply], &segments);
    assert!(sorted[0].amount.is_absent(), "{:?}", sorted[0].amount);
    assert_eq!(
        sorted[0].due.map(|d| d.date),
        Some(date("2026-03-31")),
        "the ask stands"
    );
}
