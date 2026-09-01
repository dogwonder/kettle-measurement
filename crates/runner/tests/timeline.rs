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
        party: "Harborne Parking Services".to_owned(),
        ask: "Pay £120.00".to_owned(),
        deadline: deadline.to_owned(),
        anchor: anchor.to_owned(),
        confidence: "high".to_owned(),
        due: None,
        evidence: vec![evidence],
        dated_by: None,
        disputed: vec![],
    }
}

#[test]
fn duplicates_from_overlapping_segments_merge_keeping_every_piece_of_evidence() {
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

    assert_eq!(sorted.len(), 1, "one obligation, said twice: {sorted:?}");
    assert_eq!(sorted[0].due.map(|d| d.date), Some(date("2026-03-17")));
    let ordinals: Vec<usize> = sorted[0].evidence.iter().map(|s| s.ordinal).collect();
    assert_eq!(ordinals, vec![1, 3], "both passages kept as evidence");
}

#[test]
fn a_less_confident_duplicate_makes_the_merged_obligation_less_confident() {
    // Two readings of one obligation, one of them unsure: the person
    // should be shown it for checking, so the doubt wins the merge.
    let confident = obligation("within 14 days", "the date of this letter", segment(1, "a"));
    let mut unsure = obligation("within 14 days", "the date of this letter", segment(2, "b"));
    unsure.confidence = "low".to_owned();

    let sorted = sort_timeline(vec![confident, unsure], &letter_dated("3 March 2026"));
    assert_eq!(sorted.len(), 1);
    assert_eq!(sorted[0].confidence, "low");
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
    assert_eq!(sorted[2].deadline, "when convenient");
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
        sorted[0].deadline, "by the date shown beside it",
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
