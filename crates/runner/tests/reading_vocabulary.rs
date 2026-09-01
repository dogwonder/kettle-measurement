//! What Kettle can read off a page, stated as a table (#399).
//!
//! Every bed in this repository was authored beside the parser it
//! exercises, by the same hand, in the same sitting — so the set of
//! phrases the bed contains and the set the parser accepts are the same
//! set, and a measurement between two things that coincide by
//! construction discovers nothing. The letter bed's entire deadline
//! vocabulary is `within N days`, `on <date>` and `by the end of the
//! month`, which is exactly what `resolve_deadline` accepted; a pack
//! scoring 1.00 had never been asked `within 14 working days`.
//!
//! This table is the other instrument. It is not a bed: no model, no
//! GPU, no fixtures. It is a list of the surface forms a British letter
//! actually uses, each with what Kettle makes of it — and crucially, a
//! **refusal is a first-class entry**. Some forms Kettle should not
//! read, and saying so here is what stops a later widening from
//! quietly making Kettle guess.
//!
//! Add a row when a real letter shows a form this table lacks. Never
//! delete one to make a change pass.

use chrono::NaiveDate;

/// What Kettle should make of a phrase.
enum Reading {
    /// Resolves to this day, counted from [`LETTER_DATE`] where the
    /// phrase counts at all.
    Day(&'static str),
    /// Refused on purpose, for this reason. A refusal displays the
    /// phrase and no date, which is honest; a guess would not be.
    Refused(&'static str),
}
use Reading::{Day, Refused};

/// The letter every relative phrase below counts from.
const LETTER_DATE: (i32, u32, u32) = (2026, 3, 10);

/// The anchor the model would supply for a phrase counting from the
/// letter. Phrases naming their own day ignore it.
const FROM_LETTER: &str = "the date of this letter";

const VOCABULARY: [(&str, &str, Reading); 28] = [
    // ---- a day named outright -------------------------------------
    ("day-first, month in words", "on 6 March 2026", Day("2026-03-06")),
    ("day-first with an ordinal", "on 6th March 2026", Day("2026-03-06")),
    ("weekday before the date", "on Friday 6 March 2026", Day("2026-03-06")),
    ("month abbreviated", "on 6 Mar 2026", Day("2026-03-06")),
    ("month abbreviated with a stop", "on 6 Sept. 2026", Day("2026-09-06")),
    ("month-first, month in words", "on March 6, 2026", Day("2026-03-06")),
    ("ISO 8601", "on 2026-03-06", Day("2026-03-06")),
    ("a time before the day", "before 5pm on 6 March 2026", Day("2026-03-06")),
    ("on or before", "on or before 6 March 2026", Day("2026-03-06")),
    (
        "all-numeric, slashes",
        "on 06/03/2026",
        Refused("day-first and month-first cannot be told apart, and guessing wrong moves a deadline by up to eleven months"),
    ),
    (
        "all-numeric, stops",
        "on 6.3.2026",
        Refused("ambiguous for the same reason as slashes"),
    ),
    // ---- an interval counted from the letter -----------------------
    ("plain days", "within 14 days", Day("2026-03-24")),
    ("days spelled out", "within fourteen days", Day("2026-03-24")),
    ("calendar days", "within 14 calendar days", Day("2026-03-24")),
    ("clear days", "within 14 clear days", Day("2026-03-24")),
    ("no later than", "no later than 14 days", Day("2026-03-24")),
    ("in the next", "in the next 14 days", Day("2026-03-24")),
    ("from, not within", "14 days from the date of this letter", Day("2026-03-24")),
    ("weeks", "within 2 weeks", Day("2026-03-24")),
    ("a fortnight", "within a fortnight", Day("2026-03-24")),
    ("one month", "within one month", Day("2026-04-10")),
    ("several months", "within 3 months", Day("2026-06-10")),
    (
        "working days",
        "within 14 working days",
        Refused("working days need a bank-holiday calendar Kettle does not have, and counting them as calendar days would be wrong by up to a week"),
    ),
    (
        "counted from receipt",
        "within 28 days of receipt",
        Refused("receipt is a day the letter does not state; counting from the letter's own date answers a question the page did not ask"),
    ),
    // ---- the end of a month ---------------------------------------
    ("end of the month", "by the end of the month", Day("2026-03-31")),
    ("end of this month", "by the end of this month", Day("2026-03-31")),
    ("the last day", "by the last day of the month", Day("2026-03-31")),
    // ---- no day at all --------------------------------------------
    (
        "by return",
        "by return of post",
        Refused("names no day, and the post is not a date"),
    ),
];

#[test]
fn every_surface_form_reads_as_the_table_says() {
    let letter =
        NaiveDate::from_ymd_opt(LETTER_DATE.0, LETTER_DATE.1, LETTER_DATE.2).expect("a real date");
    let mut wrong = Vec::new();
    for (form, phrase, want) in &VOCABULARY {
        let got = runner::timeline::resolve_deadline(phrase, FROM_LETTER, letter)
            .map(|r| r.date.to_string());
        match (want, &got) {
            (Day(expected), Some(actual)) if expected == actual => {}
            (Refused(_), None) => {}
            (Day(expected), _) => wrong.push(format!(
                "  {form}: {phrase:?} should read {expected}, read {got:?}"
            )),
            (Refused(why), Some(actual)) => wrong.push(format!(
                "  {form}: {phrase:?} should be refused ({why}), read {actual}"
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} surface forms do not read as this table says:\n{}",
        wrong.len(),
        VOCABULARY.len(),
        wrong.join("\n")
    );
}
