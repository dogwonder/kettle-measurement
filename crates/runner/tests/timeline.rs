//! #241: deadlines resolved in Rust, duplicates merged. The model
//! reads "within 14 days" off the page; every date below is arithmetic
//! it never does (CLAUDE.md) — a date a model invented is a missed
//! deadline.

use chrono::NaiveDate;
use runner::claim::Kind;
use runner::document::{segments_from_text, Segment};
use runner::run::Obligation;
use runner::timeline::{
    confirm_letter_date, confirmed_deadline, date_dispute, letter_date, resolve_deadline,
    sort_timeline,
};
use std::str::FromStr;

fn date(iso: &str) -> NaiveDate {
    NaiveDate::from_str(iso).expect("test date")
}

#[test]
fn a_relative_deadline_resolves_against_the_letter_date() {
    // The issue's own three cases, anchored to 3 March 2026.
    let letter = date("2026-03-03");
    assert_eq!(
        resolve_deadline("within 14 days", "the date of this letter", letter).map(|r| r.date),
        Some(date("2026-03-17"))
    );
    assert_eq!(
        resolve_deadline("by the end of the month", "the date of this letter", letter)
            .map(|r| r.date),
        Some(date("2026-03-31"))
    );
    assert_eq!(
        resolve_deadline("when convenient", "no particular date", letter),
        None,
        "an unresolvable phrase is not guessed"
    );
}

#[test]
fn an_absolute_date_is_read_not_computed() {
    let letter = date("2026-03-03");
    assert_eq!(
        resolve_deadline("by 12 August 2026", "12 August 2026", letter).map(|r| r.date),
        Some(date("2026-08-12"))
    );
    // The deadline phrase alone carries the date; the anchor echoes it.
    assert_eq!(
        resolve_deadline("on 3 March 2026", "3 March 2026", letter).map(|r| r.date),
        Some(date("2026-03-03"))
    );
}

/// A resolved deadline says which kind of claim it is (#366, #367).
///
/// The test above is *named* "read not computed" and cannot assert it:
/// both branches return a bare date, so "12 August 2026" quoted off the
/// page and "within 14 days" counted from the letter arrive at the
/// report identically. They are not the same claim — one is wrong only
/// if the page was misread, the other only if this arithmetic is wrong,
/// and a person chasing a missed deadline needs to know which they are
/// looking at.
#[test]
fn a_resolved_deadline_states_whether_it_was_read_or_worked_out() {
    let letter = date("2026-03-03");

    let written = resolve_deadline("by 12 August 2026", "12 August 2026", letter)
        .expect("an absolute deadline resolves");
    assert_eq!(written.date, date("2026-08-12"));
    assert_eq!(written.kind, Kind::ReadAndVerified);

    let counted = resolve_deadline("within 14 days", "the date of this letter", letter)
        .expect("a relative deadline resolves against the letter date");
    assert_eq!(counted.date, date("2026-03-17"));
    assert_eq!(counted.kind, Kind::WorkedOut);

    // The month-end phrase is arithmetic too — the page never wrote
    // "31 March", Rust worked out which day the month ends on.
    let month_end = resolve_deadline("by the end of the month", "the date of this letter", letter)
        .expect("a month-end deadline resolves");
    assert_eq!(month_end.kind, Kind::WorkedOut);
}

/// A deadline phrase that quotes its own anchor is still counted from
/// it (#435).
///
/// Found by measuring Qwen3.5-9B on the letter bed: it returns the whole
/// phrase, anchor included — `"within 30 days of 22 May 2026"` — where
/// the other models return the bare `"within 30 days"`. Both are
/// defensible readings of the sentence, so the resolver has to survive
/// either.
///
/// It did not. The written-date shortcut scans the *whole* phrase for a
/// date, found the anchor sitting inside it and returned that: thirty
/// days early, on 35 of 445 obligation decisions. The arithmetic below
/// it never ran.
///
/// The kind is the half that matters more. Returning early claims the
/// page wrote this date (#366), so a date Kettle should have worked out
/// was both wrong and asserted in the strongest voice it has.
#[test]
fn a_deadline_that_quotes_its_anchor_is_still_counted_from_it() {
    let letter = date("2026-03-03");

    let counted = resolve_deadline("within 30 days of 22 May 2026", "22 May 2026", letter)
        .expect("a relative deadline resolves against its dated anchor");
    assert_eq!(
        counted.date,
        date("2026-06-21"),
        "thirty days after the anchor, not the anchor itself"
    );
    assert_eq!(
        counted.kind,
        Kind::WorkedOut,
        "arithmetic over a date that was read is not itself read"
    );

    // The same phrase with no separate anchor field: the date the
    // arithmetic counts from is inside the deadline and nowhere else,
    // which is exactly the shape the model returned.
    let inline = resolve_deadline("within 30 days of 22 May 2026", "", letter)
        .expect("the anchor inside the phrase is still an anchor");
    assert_eq!(inline.date, date("2026-06-21"));
    assert_eq!(inline.kind, Kind::WorkedOut);
}

#[test]
fn a_dated_anchor_beats_the_letter_date() {
    // "within 14 days of the hearing on 1 June 2026" counts from the
    // hearing, not from the letter.
    let letter = date("2026-03-03");
    assert_eq!(
        resolve_deadline("within 14 days", "1 June 2026", letter).map(|r| r.date),
        Some(date("2026-06-15"))
    );
}

#[test]
fn month_ends_and_leap_years_are_arithmetic_not_guesswork() {
    // Thirty days from 31 January 2024 crosses a 29-day February.
    assert_eq!(
        resolve_deadline(
            "within 30 days",
            "the date of this letter",
            date("2024-01-31")
        )
        .map(|r| r.date),
        Some(date("2024-03-01"))
    );
    // The end of February is the 29th in a leap year and the 28th not.
    assert_eq!(
        resolve_deadline(
            "by the end of the month",
            "the date of this letter",
            date("2024-02-10")
        )
        .map(|r| r.date),
        Some(date("2024-02-29"))
    );
    assert_eq!(
        resolve_deadline(
            "by the end of the month",
            "the date of this letter",
            date("2025-02-10")
        )
        .map(|r| r.date),
        Some(date("2025-02-28"))
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

fn obligation(deadline: &str, anchor: &str, evidence: Segment) -> Obligation {
    Obligation {
        kind: "payment".to_owned(),
        party: runner::reading::Reading::new(
            evidence.ordinal,
            "Harborne Parking Services".to_owned(),
        ),
        ask: "Pay £120.00".to_owned(),
        deadline: runner::reading::Reading::new(evidence.ordinal, deadline.to_owned()),
        anchor: anchor.to_owned(),
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

    let sorted = sort_timeline(vec![first, second], &letter_dated("3 March 2026"));

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
    let march = obligation(
        "within 14 days",
        "the date of this letter",
        segment(1, "pay"),
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
    let counted = obligation(
        "within 14 days",
        "the date of this letter",
        segment(1, "pay"),
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
    let resolved = sort_timeline(
        vec![obligation(
            "within 7 calendar days",
            "no particular date",
            letter[0].clone(),
        )],
        &letter,
    );
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
fn a_deadline_counted_from_a_date_you_gave_is_yours() {
    // #412 step 4, and the test `claim.rs` asked for before `Yours`
    // could exist. When two readings disagreed about the letter's date
    // and a person settled it, every deadline counted from that date
    // depends on their answer — so if one is wrong, it is wrong because
    // their date was, not because Rust's arithmetic or the reading was.
    // A report that called it "worked out" would be claiming arithmetic
    // over values read off the page, which is no longer true.
    let yours = confirmed_deadline(
        "within 14 days",
        "the date of this letter",
        date("2026-03-03"),
    )
    .expect("a relative deadline resolves against the date you gave");

    assert_eq!(yours.date, date("2026-03-17"));
    assert_eq!(yours.kind, Kind::Yours);
}

#[test]
fn a_date_written_on_the_page_is_unaffected_by_your_correction() {
    // "by 12 August 2026" never depended on the letter's date, so
    // correcting the letter's date must not restate it as yours. It is
    // still read off the page, and still wrong only if the page was
    // misread.
    let written = confirmed_deadline("by 12 August 2026", "12 August 2026", date("2026-03-03"))
        .expect("an absolute deadline resolves");

    assert_eq!(written.date, date("2026-08-12"));
    assert_eq!(written.kind, Kind::ReadAndVerified);
}

#[test]
fn a_deadline_counted_from_a_dated_anchor_is_still_worked_out() {
    // "within 14 days of the hearing on 1 June 2026" counts from the
    // hearing, which is on the page. The person's date is not in this
    // answer at all, so claiming it is theirs would be false — and
    // would quietly widen what their correction is taken to cover.
    let counted = confirmed_deadline("within 14 days", "1 June 2026", date("2026-03-03"))
        .expect("a dated anchor resolves");

    assert_eq!(counted.date, date("2026-06-15"));
    assert_eq!(counted.kind, Kind::WorkedOut);
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

/// #544: a deadline that points at a table resolves against the table.
///
/// The v14 letter run got this passage right and was scored wrong for
/// it. Given `"Please find our invoice ... Payment of the total is due
/// by the date shown beside it."`, the model recorded a payment
/// obligation and copied the deadline exactly — which is what the
/// prompt demands of it, since the sentence contains no date to read.
/// The bed expected the obligation on the table row instead, and the
/// row is `"Due date 6 March 2026"`: no ask, no party, nothing a
/// closed question about that passage alone could turn into a payment.
/// Asking the model for one is asking it to invent, which is the harm
/// the pack's `no_obligation` ceiling exists to stop.
///
/// So the ask stays where it was made and the date stays where it was
/// printed, and the resolver is what has to cross between them. The
/// letter is read from the committed fixture rather than retyped: what
/// is being tested is the passage a person's letter actually produces.
#[test]
fn a_pointing_deadline_resolves_against_the_table_it_points_at() {
    let segments = mortise_02();
    let prose = pointing_passage(&segments);

    // The v14 run's own answer for that passage, copied from the
    // archived response rather than written to suit the test.
    let pointing = obligation(
        "by the date shown beside it",
        "the date shown beside it",
        prose,
    );

    let sorted = sort_timeline(vec![pointing], &segments);

    assert_eq!(
        sorted[0].due.map(|resolved| resolved.date),
        Some(date("2026-03-06")),
        "the date printed beside the ask is the date a person is given: {sorted:#?}"
    );
}

/// #460 rule one, applied to the date this resolution invents nothing
/// to reach: the quote must contain the value it evidences.
///
/// The pointing passage says "by the date shown beside it" and that is
/// the whole of what it says — a person reading the report sees 6 March
/// 2026 asserted, and the words offered for it contain no date at all.
/// The row the resolver read has to travel with the claim; otherwise
/// the fix trades a missing date for an unbacked one, which is the
/// worse of the two.
///
/// It travels in its own field, not in `evidence`, and that distinction
/// was measured rather than reasoned. Written as an extra `evidence`
/// entry it read, to everything downstream, as *the run asserted an
/// obligation on this row* — and replaying the v14 letter run scored
/// all twelve due-date rows as inventions on exactly that basis. The
/// passages in `evidence` are the ones the model was asked about; this
/// is one Rust went and read because the answer said where to look.
#[test]
fn the_row_a_pointing_deadline_was_read_from_travels_with_the_claim() {
    let segments = mortise_02();
    let prose = pointing_passage(&segments);
    let sorted = sort_timeline(
        vec![obligation(
            "by the date shown beside it",
            "the date shown beside it",
            prose,
        )],
        &segments,
    );

    let dated_by = sorted[0]
        .dated_by
        .as_ref()
        .expect("the row the date was read out of");
    assert!(
        dated_by.text.contains("6 March 2026"),
        "the date a person is shown is quoted from the page it was read off: {dated_by:#?}"
    );

    let quoted: Vec<&str> = sorted[0]
        .evidence
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert_eq!(
        quoted,
        vec![
            "Please find our invoice for your council tax account below. \
Payment of the total is due by the date shown beside it."
        ],
        "the passage the model answered about is the only one it asserted on"
    );
}

/// The refusal half. A pointing phrase is only resolvable because the
/// page prints the date somewhere; where it does not, the honest answer
/// is the one this pack already gives for every other phrase it cannot
/// resolve — keep the words, stay undated, sort last (#241).
///
/// Without this the new rule degrades into "a letter that mentions a
/// date beside something gets that date", which is the guessing the
/// resolver's small closed phrase set exists to prevent.
#[test]
fn a_pointing_deadline_with_no_row_to_point_at_stays_undated() {
    let segments = vec![
        segment(0, "6 February 2026"),
        segment(
            1,
            "Payment of the total is due by the date shown beside it.",
        ),
        segment(2, "We wrote to you about this on 3 January 2026."),
    ];
    let sorted = sort_timeline(
        vec![obligation(
            "by the date shown beside it",
            "the date shown beside it",
            segments[1].clone(),
        )],
        &segments,
    );

    assert_eq!(
        sorted[0].due, None,
        "no due-date row, so no date: {sorted:#?}"
    );
    assert_eq!(
        sorted[0].deadline.value, "by the date shown beside it",
        "the letter's own words survive to where a person will read them"
    );
}

/// #330's rule, which this resolution has to inherit rather than
/// rediscover: "beside it" is a place on *this* page. A run may pool a
/// letter and its chaser, and the second document's due-date row is
/// beside nothing in the first — a date months wrong, presented as read
/// off the page, which is the strongest voice the report has.
#[test]
fn a_pointing_deadline_cannot_reach_another_documents_due_date() {
    let mut invoice = segment(0, "Due date 6 March 2026");
    invoice.document = 1;
    let chaser = segment(
        0,
        "Payment of the total is due by the date shown beside it.",
    );
    let segments = vec![chaser.clone(), invoice];

    let sorted = sort_timeline(
        vec![obligation(
            "by the date shown beside it",
            "the date shown beside it",
            chaser,
        )],
        &segments,
    );

    assert_eq!(
        sorted[0].due, None,
        "the other document's due date is beside nothing here: {sorted:#?}"
    );
}

/// The commonest shape must be untouched. A deadline that states its
/// own date resolves from the phrase, as it always has, and never
/// consults a row — otherwise a letter carrying both would answer with
/// whichever the code happened to try first.
#[test]
fn a_deadline_that_names_its_date_ignores_the_due_date_row() {
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
        sorted[0].due.map(|resolved| resolved.date),
        Some(date("2026-02-20")),
        "the phrase's own date is the answer: {sorted:#?}"
    );
}

/// A letter is free to word the pointer however it likes, and the two
/// halves of this bed already do: development invoices say "by the date
/// shown beside it" and exam invoices say "by the date given against
/// it". A rule that recognised the first and not the second would
/// resolve the set it was written against and leave the sealed set
/// undated, which is a fix that measures itself.
///
/// So the rule is stated over what the phrase *does* — name the date
/// and point somewhere on the page — rather than over either set's
/// wording.
#[test]
fn a_pointer_is_recognised_by_what_it_does_not_by_its_wording() {
    let row = segment(2, "Due date 6 March 2026");
    for words in [
        "by the date shown beside it",
        "by the date given against it",
        "by the date shown opposite",
        "by the date set out below",
    ] {
        let prose = segment(1, "Payment of the total is due.");
        let segments = vec![segment(0, "6 February 2026"), prose.clone(), row.clone()];
        let sorted = sort_timeline(vec![obligation(words, words, prose)], &segments);
        assert_eq!(
            sorted[0].due.map(|resolved| resolved.date),
            Some(date("2026-03-06")),
            "{words:?} points at the row: {sorted:#?}"
        );
    }

    // And the counter-case, which is why this cannot simply be "any
    // phrase naming a date": a date somewhere else is not a date on
    // this page, and the row must not be read as though it were.
    let prose = segment(
        1,
        "Payment is due by the date shown on your last statement.",
    );
    let segments = vec![segment(0, "6 February 2026"), prose.clone(), row];
    let sorted = sort_timeline(
        vec![obligation(
            "by the date shown on your last statement",
            "no particular date",
            prose,
        )],
        &segments,
    );
    assert_eq!(
        sorted[0].due, None,
        "another document's date is not beside anything here: {sorted:#?}"
    );
}

/// #612, second half — found on the same real letter the field was
/// built for. The ask sentence, *"Unless payment of all overdue
/// invoices is received within 7 calendar days…"*, prints no sum; the
/// sum sits two passages away in the letter's own row, *Amount Due
/// 41.21 GBP*. The model answered `no amount` for its passage, which
/// was right, and the report showed nothing, which was not what the
/// letter said.
///
/// #544's shape: where a payment ask's passage prints no sum and the
/// document labels one — an amount-due, total or balance row — Rust
/// reads that row and it travels with the claim in `priced_by`, never
/// in `evidence`. Nothing is computed, so it is read-and-verified.
#[test]
fn the_row_a_payment_asks_sum_was_read_from_travels_with_the_claim() {
    let ask = segment(
        14,
        "All outstanding transactions are detailed below. Unless payment of all          overdue invoices is received within 7 calendar days, we may commence          immediate legal action without further notice.",
    );
    let segments = vec![
        segment(6, "20/08/2026"),
        segment(9, "Amount Due 41.21 GBP 009422"),
        ask.clone(),
        segment(
            18,
            "Transaction Details (GBP) Original (GBP) Amount Due (GBP) Invoice Number              Invoice Type Invoice Date Invoice Age",
        ),
        segment(19, "EXPD 30/06/2026 51 41.21 41.21 396636183"),
    ];
    let mut asked = obligation("within 7 calendar days", "no particular date", ask);
    asked.amount = runner::reading::Reading::absent(asked.amount.at);
    let sorted = sort_timeline(vec![asked], &segments);

    assert_eq!(sorted[0].amount.value, "41.21 GBP");
    let priced_by = sorted[0]
        .priced_by
        .as_ref()
        .expect("the row the sum was read out of");
    assert_eq!(priced_by.ordinal, 9);
    assert_eq!(
        sorted[0].evidence.len(),
        1,
        "the passage the model answered about is the only one it asserted on"
    );
}

/// The refusals. A sum the passage prints itself is left alone; an ask
/// that is not a payment gets no sum; a document whose best label names
/// two different figures is ambiguous and stays blank; and the bed's
/// invoice table, where the labels sit mid-text after the reader took
/// each column in turn, gives the total and not the sub total.
#[test]
fn a_sum_is_read_off_a_row_only_where_the_page_labels_exactly_one() {
    let total_row = |text: &str| segment(3, text);
    let ask = || {
        segment(
            2,
            "Payment of the total is due by the date shown beside it.",
        )
    };

    // Printed in the passage: untouched.
    let mut own = obligation("within 14 days", "the date of this letter", ask());
    own.amount.value = "£120.00".to_owned();
    let own = sort_timeline(vec![own], &[ask(), total_row("Total £360.00")]);
    assert_eq!(own[0].amount.value, "£120.00");
    assert!(own[0].priced_by.is_none());

    // Not a payment: no sum is looked for.
    let mut reply = obligation("within 14 days", "the date of this letter", ask());
    reply.kind = "response".to_owned();
    reply.amount = runner::reading::Reading::absent(reply.amount.at);
    let reply = sort_timeline(vec![reply], &[ask(), total_row("Total £360.00")]);
    assert!(reply[0].amount.is_absent());

    // Two totals: ambiguous, so blank.
    let mut two = obligation("within 14 days", "the date of this letter", ask());
    two.amount = runner::reading::Reading::absent(two.amount.at);
    let two = sort_timeline(
        vec![two],
        &[
            ask(),
            total_row("Total £360.00"),
            segment(4, "Total £400.00"),
        ],
    );
    assert!(two[0].amount.is_absent());
    assert!(two[0].priced_by.is_none());

    // The bed's invoice table, read column by column.
    let mut invoice = obligation("within 14 days", "the date of this letter", ask());
    invoice.amount = runner::reading::Reading::absent(invoice.amount.at);
    let invoice = sort_timeline(
        vec![invoice],
        &[
            ask(),
            total_row("Due date 6 March 2026 Sub total £300.00 VAT £60.00 Total £360.00"),
        ],
    );
    assert_eq!(invoice[0].amount.value, "£360.00");
    assert_eq!(invoice[0].priced_by.as_ref().map(|s| s.ordinal), Some(3));
}

/// *Rust verifies; it never discovers* (CLAUDE.md, 4 September 2026).
/// The model names the passage a value lives in; Rust checks the named
/// passage and refuses a wrong naming. No label list is consulted when
/// a passage is named.
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
    pointed.deadline = runner::reading::Reading::new(11, "by the date below");
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
