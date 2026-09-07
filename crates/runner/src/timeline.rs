//! Deadlines resolved in Rust, soonest first (#241); one reading per
//! passage, never merged across passages (review of #626, Task 2).
//!
//! A letter says "within 14 days of the date of this letter", "by the
//! end of the month", "on 3 March 2026". The model reads those phrases
//! off the page (#240) and reads them into structure — a count, a
//! unit, a qualifier, what they count from — and Rust checks every
//! field against the words before it counts (`resolve_structured`;
//! review of #626, Task 5). Every date below is arithmetic the model
//! never does (CLAUDE.md), because a date a model invented is a missed
//! deadline; and no line is searched for either (*Rust verifies; it
//! never discovers*): the passage a period counts from is a reading
//! the model names and Rust verifies, never a dateline Rust went and
//! found. The one finder left, `dateline`, serves the OCR dispute
//! (`date_dispute`) and is staged for retirement there.
//!
//! Unresolvable phrases are not guessed. An obligation whose date
//! cannot be resolved keeps its phrase, stays undated and sorts last —
//! in front of a person, never silently dropped or given today's date.
//!
//! Generalised, not letter-specific: housing complaints (#92) and
//! warranty monitoring want the same step, which is why the plugin
//! architecture doc names `timeline-sort` rather than
//! `letter-timeline-sort`.

use crate::claim::Kind;
use crate::document::Segment;
use crate::run::Obligation;
use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// A due date and how it was arrived at (#366, #367).
///
/// The two are one value because they cannot be allowed to disagree: a
/// date and a separate claim *about* that date is a second assertion
/// nobody checks, and the resolver is the only place that knows which
/// branch it took. "12 August 2026" quoted off the page is wrong only
/// if the page was misread; "within 14 days" counted from the letter is
/// wrong only if this arithmetic is. A person chasing a missed deadline
/// needs to know which of those they are looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    pub date: NaiveDate,
    pub kind: Kind,
}

/// The arithmetic a deadline phrase asks for, if it asks for any.
///
/// Naming it separates the two questions that used to be tangled: *is
/// this phrase relative* decides whether a date inside it is the answer
/// or the starting point, and it has to be settled before that date is
/// looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Counted {
    Days(u64),
    /// Calendar months, which are not a fixed number of days: "within
    /// one month" of 31 January is 28 February, and chrono's
    /// `checked_add_months` clamps to the month's last day exactly as a
    /// person would.
    Months(u32),
}

/// A count written as digits or as a word.
///
/// "a fortnight" and "one month" are the same construction as "14
/// days", and a letter picks between them by rhythm rather than by
/// meaning. The word list stops at the counts a letter actually uses:
/// nothing says "within eighty-three days".
fn count_word(word: &str) -> Option<u64> {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    if let Ok(number) = word.parse::<u64>() {
        return (number > 0).then_some(number);
    }
    Some(match word {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "fourteen" => 14,
        "twenty" => 20,
        "thirty" => 30,
        "sixty" => 60,
        "ninety" => 90,
        _ => return None,
    })
}

/// What a deadline phrase does, read from its words alone (#554).
///
/// The resolver above asks these questions in this order — does it
/// count, does it name a day, does it point at one — and the shape is
/// that order's answer, so an identity built on it agrees with how the
/// day was arrived at. It is part of an obligation's identity because
/// two phrases can resolve to one day by different routes, and the
/// route is a claim: "within 45 days of 23 August 2026" was read and
/// counted, "by 7 October 2026" for the same letter was *computed by
/// the model*, which the prompt forbids and the report would present
/// as read from the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineShape {
    /// Asks for arithmetic from a base date: "within 14 days", "by the
    /// end of the month".
    Counted,
    /// Names its own day: "on 27 December 2026".
    Absolute,
    /// Names no day in the ask and says where on the page one is
    /// (#544): the date the due-date row prints, read at that row.
    Pointed,
    /// None of the above: "as soon as you are able".
    Undated,
}

/// The route a deadline takes, read from its structure and not from
/// its words (#554; review of #626, Task 5) — the same function on the
/// run's side and the bed's, so the scorer's identity and the runtime
/// agree by construction rather than by two parsers agreeing.
///
/// `pointed`: the words were read at a passage other than the ask's
/// own — the due-date row. `dated`: they parse as one full date.
pub fn deadline_route(read: &crate::run::When, pointed: bool, dated: bool) -> DeadlineShape {
    if read.counts_from == "month_end" || read.unit != "none" {
        DeadlineShape::Counted
    } else if pointed && dated {
        DeadlineShape::Pointed
    } else if dated {
        DeadlineShape::Absolute
    } else {
        DeadlineShape::Undated
    }
}

/// Why a deadline stayed undated, in the vocabulary of the three
/// outcomes (`app/METHOD.md` §0): the page contradicted the structure
/// (refused), the words are on the page but ask for a computation
/// Kettle does not make (unsupported), or nothing was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Unresolved {
    /// The structure the model read is not in the words it read it from.
    Contradicted { why: String },
    /// The words are the page's; the derivation is not one Kettle makes.
    Unsupported { why: String },
    /// A base the period counts from was not read.
    NoBase { why: String },
}

/// Resolve a deadline from the fields the model read it into, after
/// checking every field against the words it was read from — whether
/// or not another field is `none` (review of #626, Task 5, closing
/// the four gaps in #622's first cut).
///
/// The check is containment, the same rule as a quote (#460): the
/// count must appear in the words as digits or as its word ("a
/// fortnight" standing for fourteen days or two weeks, "a month" for
/// one), the unit as its word, a qualifier as its word, "receipt" as
/// the word, and the end of a month as *end* and *month* both present.
/// Working days and receipt are unsupported computations, refused by
/// their value and kept as words. A period counts only from a base
/// the model read (`from`, verified as one full date at the passage
/// it names): `counts_from: none` counts from nothing and never
/// borrows the letter's date, and a `from` the page refused leaves the
/// period undated. Nothing here parses prose to *find* anything; it
/// asks whether what the model said is there, and then counts.
///
/// `from` is the base's date where one was read; `from_kind` is whose
/// claim a date counted from it is — `WorkedOut` when the model read
/// the base, `Yours` when a person confirmed it (#412).
pub fn resolve_structured(
    deadline: &str,
    read: &crate::run::When,
    pointed: bool,
    from: Option<NaiveDate>,
    from_kind: Kind,
) -> Result<Resolved, Unresolved> {
    let lowered = deadline.to_lowercase();
    let words: Vec<&str> = lowered
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .collect();
    let has = |w: &str| words.contains(&w);
    let contradicted = |why: &str| Unresolved::Contradicted {
        why: why.to_owned(),
    };
    let unsupported = |why: &str| Unresolved::Unsupported {
        why: why.to_owned(),
    };

    // Every field against the words, before anything else. The
    // qualifier and the receipt are checked both ways: a structure
    // that leaves out a word the letter wrote is as wrong as one that
    // adds a word it did not.
    let says_working = has("working") || has("business");
    match read.qualifier.as_str() {
        "none" => {
            if says_working {
                return Err(contradicted(
                    "the words say working days and the reading did not",
                ));
            }
        }
        "working" => {
            if !says_working {
                return Err(contradicted(
                    "the reading says working days and the words do not",
                ));
            }
        }
        "calendar" | "clear" => {
            if !has(&read.qualifier) {
                return Err(contradicted("the reading's qualifier is not in the words"));
            }
        }
        _ => return Err(contradicted("an unknown qualifier")),
    }
    let says_receipt = has("receipt");
    if read.counts_from == "receipt" && !says_receipt {
        return Err(contradicted(
            "the reading counts from receipt and the words do not say so",
        ));
    }
    if says_receipt && read.counts_from != "receipt" {
        return Err(contradicted(
            "the words count from receipt and the reading does not",
        ));
    }

    // The end of a month names no count and no unit: a base and an
    // operation, verified as *end* and *month* both present.
    if read.counts_from == "month_end" {
        if !(has("end") && (has("month") || has("months"))) {
            return Err(contradicted("month end the words never gave"));
        }
        // A count is a period the words never gave; a unit of months
        // with no count is the word *month* the words do give, and
        // contradicts nothing.
        if read.count != 0 || !matches!(read.unit.as_str(), "none" | "months") {
            return Err(contradicted("a period alongside month end"));
        }
        let Some(base) = from else {
            return Err(Unresolved::NoBase {
                why: "the month is the letter's, and its date was not read".to_owned(),
            });
        };
        return end_of_month(base)
            .map(|date| Resolved {
                date,
                kind: from_kind,
            })
            .ok_or_else(|| unsupported("no such month"));
    }

    // No period: the day is named in the words or not at all.
    if read.unit == "none" {
        if read.count != 0 {
            return Err(contradicted("a count with no unit"));
        }
        if matches!(read.counts_from.as_str(), "letter_date" | "receipt") {
            return Err(contradicted("a base with no period to count"));
        }
        return match first_full_date(deadline) {
            Some(date) => Ok(Resolved {
                date,
                kind: Kind::ReadAndVerified,
            }),
            None => Err(unsupported(if pointed {
                "the row pointed at prints no full date"
            } else {
                "the words name no day Kettle can read"
            })),
        };
    }

    // A period: the count as digits or as its word, the unit as its word.
    let counted = match read.unit.as_str() {
        "days" => Counted::Days(read.count),
        "weeks" => Counted::Days(read.count.saturating_mul(7)),
        "months" => Counted::Months(u32::try_from(read.count).unwrap_or(u32::MAX)),
        _ => return Err(contradicted("an unknown unit")),
    };
    if read.count == 0 {
        return Err(contradicted("a period of nothing"));
    }
    let count_present = words.iter().any(|w| count_word(w) == Some(read.count))
        || (has("fortnight")
            && (read.unit == "days" && read.count == 14
                || read.unit == "weeks" && read.count == 2))
        || (read.unit == "months" && read.count == 1 && (has("month") || has("months")));
    if !count_present {
        return Err(contradicted("a count the words do not contain"));
    }
    let unit_present = match read.unit.as_str() {
        "days" => has("day") || has("days") || has("fortnight"),
        "weeks" => has("week") || has("weeks") || has("fortnight"),
        _ => has("month") || has("months"),
    };
    if !unit_present {
        return Err(contradicted("a unit the words do not contain"));
    }
    if read.qualifier == "working" {
        return Err(unsupported(
            "working days need a bank-holiday calendar Kettle does not have",
        ));
    }
    let (base, kind) = match read.counts_from.as_str() {
        "receipt" => {
            return Err(unsupported("receipt is a day the letter does not state"));
        }
        // Where the words themselves print the day they count from
        // ("within 30 days of 20 March 2026"), that day is the base:
        // the words are the model's own verified reading, and parsing
        // a date out of them is step 4, not a search. A `from` naming
        // a different day is the model handing the dateline over, and
        // the words win.
        "named_date" if first_full_date(deadline).is_some() => {
            (first_full_date(deadline).expect("checked"), Kind::WorkedOut)
        }
        "named_date" | "letter_date" => match from {
            Some(base) => (base, from_kind),
            None => {
                return Err(Unresolved::NoBase {
                    why: if read.counts_from == "letter_date" {
                        "the letter's date was not read".to_owned()
                    } else {
                        "the day it counts from was not read".to_owned()
                    },
                })
            }
        },
        "none" => {
            return Err(Unresolved::NoBase {
                why: "the words say what to count and not what from".to_owned(),
            });
        }
        _ => return Err(contradicted("an unknown base")),
    };
    let date = match counted {
        Counted::Days(days) => base.checked_add_days(Days::new(days)),
        Counted::Months(months) => base.checked_add_months(Months::new(months)),
    };
    date.map(|date| Resolved { date, kind })
        .ok_or_else(|| unsupported("a date past the calendar"))
}

/// The last day of `date`'s month — a leap-year February included,
/// because it is computed as the day before the next month's first.
fn end_of_month(date: NaiveDate) -> Option<NaiveDate> {
    date.with_day(1)?
        .checked_add_months(Months::new(1))?
        .checked_sub_days(Days::new(1))
}

/// The first "3 March 2026"-shaped date in `text`. British letters
/// write the day first and the month as a word; this reads exactly
/// that, and nothing looser — "03/04/2026" is ambiguous on purpose.
pub fn first_full_date(text: &str) -> Option<NaiveDate> {
    find_full_date(&date_words(text)).map(|(date, _, _)| date)
}

/// The words a date is looked for in, split the way a date is written.
///
/// A stop between two digits is a separator inside one date —
/// `20.8.2026` — and not the end of a word, so it stays. A stop after
/// a letter (`Sept. 2026`) still splits.
fn date_words(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let boundary = |at: usize, c: char| {
        if c.is_whitespace() || matches!(c, ',' | ';' | ':' | '(' | ')') {
            return true;
        }
        c == '.'
            && !(at > 0
                && bytes[at - 1].is_ascii_digit()
                && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
    };
    let mut words = Vec::new();
    let mut start = 0;
    for (at, c) in text.char_indices() {
        if boundary(at, c) {
            if start < at {
                words.push(&text[start..at]);
            }
            start = at + c.len_utf8();
        }
    }
    if start < text.len() {
        words.push(&text[start..]);
    }
    words.into_iter().flat_map(split_run_together_day).collect()
}

/// An all-numeric date, read only where its own digits settle the
/// order (#613).
///
/// `06/03/2026` is 6 March to a British reader and 3 June to an
/// American one, and guessing wrong moves every deadline in the letter
/// by up to eleven months — so it is refused. But a day over twelve
/// cannot be a month: `20/08/2026` is 20 August in both forms, and so
/// is `08/20/2026`. That half is unambiguous by the same argument as
/// ISO and is read. The rule depends on the value rather than the
/// form, so one letterhead reads in August and refuses in April; that
/// is a refusal being claim-local, which is the honest shape — the
/// April letter is genuinely ambiguous and the August one is not.
///
/// Four-digit years only. A two-digit year names no century, and the
/// day-over-twelve rule cannot then say which field is the year.
fn numeric_date(word: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = word.split(['/', '-', '.']).collect();
    let [first, second, year] = parts[..] else {
        return None;
    };
    if year.len() != 4 || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let year: i32 = year.parse().ok().filter(|y| (1900..=2200).contains(y))?;
    let (first, second): (u32, u32) = (first.parse().ok()?, second.parse().ok()?);
    let (day, month) = match (first > 12, second > 12) {
        (true, false) => (first, second),
        (false, true) => (second, first),
        // Both could be the month: refused. Neither could: not a date.
        _ => return None,
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

/// The first full date, where in `words` it starts and how many words
/// it spans, so a caller can ask what else is on the line with it.
fn find_full_date(words: &[&str]) -> Option<(NaiveDate, usize, usize)> {
    // ISO first, because it is one word and the windows below cannot
    // see inside it. Unambiguous by definition, which is why it is read
    // where the all-numeric British and American forms are refused.
    if let Some(found) = words.iter().enumerate().find_map(|(at, word)| {
        NaiveDate::parse_from_str(word, "%Y-%m-%d")
            .ok()
            .or_else(|| numeric_date(word))
            .map(|date| (date, at, 1))
    }) {
        return Some(found);
    }
    words.windows(3).enumerate().find_map(|(at, window)| {
        // Day first — "6 March 2026" — is how a British letter writes
        // it, and month first — "March 6, 2026" — is how imported
        // stationery and some software do. The month is a word in both,
        // so neither can be read as the other and taking both costs no
        // ambiguity.
        let (day, month) = match (day_number(window[0]), month_number(window[1])) {
            (Some(day), Some(month)) => (day, month),
            _ => (day_number(window[1])?, month_number(window[0])?),
        };
        let year: i32 = window[2]
            .parse()
            .ok()
            .filter(|y| (1900..=2200).contains(y))?;
        NaiveDate::from_ymd_opt(year, month, day).map(|date| (date, at, 3))
    })
}

/// The words a dateline may carry besides the date itself.
const DATELINE_WORDS: [&str; 10] = [
    "date",
    "dated",
    "issue",
    "issued",
    "of",
    "on",
    "our",
    "ref",
    "reference",
    "your",
];

/// The document dating *itself*, as opposed to a sentence that happens
/// to name a day (#578).
///
/// A letter writes its own date on a line of its own — "3 March 2026",
/// "Date: 3 March 2026", "Thursday 28th April 2022", or beside the
/// sender's name on a letterhead. A sentence about something else —
/// "works will begin at your building on 22 April 2026" — is not the
/// letter dating itself, and taking it as one hands every relative
/// deadline in the document an anchor the letter never offered.
///
/// The test is prose, and prose is lowercase: every word on the line
/// that is not part of the date must be capitalised, or one of the few
/// lowercase words a dateline uses. A line long enough to be a sentence
/// is refused whatever its case, because a run of capitals is a
/// heading, not a date.
fn dateline(line: &str) -> Option<NaiveDate> {
    let words = date_words(line);
    let (date, at, span) = find_full_date(&words)?;
    let rest: Vec<&&str> = words
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < at || *index >= at + span)
        .map(|(_, word)| word)
        .collect();
    if rest.len() > 6 {
        return None;
    }
    rest.iter()
        .all(|word| {
            let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
            bare.is_empty()
                || bare.chars().next().is_some_and(char::is_uppercase)
                || DATELINE_WORDS.contains(&bare.to_lowercase().as_str())
        })
        .then_some(date)
}

/// Split a day that has run into its month: `28thApril` → `28th`,
/// `April`.
///
/// Not a hypothetical tidy-up. A photograph read without the reader's
/// language correction drops word spaces wholesale — measured on a real
/// letter, which came back as `28thApril`, `herebyauthorise`,
/// `BuildingSafety` (#412). Left alone, that reading finds no date,
/// which then disagrees with the corrected reading, and the person is
/// stopped to adjudicate a dispute that is nothing but a missing space.
/// The gate has to fire on wrong dates, not on the reader's habits.
///
/// Only a leading run of digits followed by a letter is split off, so
/// `M14` and `18521R` are untouched — neither begins with the shape a
/// day does — and `20/08/2026` stays one word for the all-numeric rule.
fn split_run_together_day(word: &str) -> Vec<&str> {
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 2 || !word[digits..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return vec![word];
    }
    // An ordinal suffix belongs with the day, not with the month.
    let after_digits = &word[digits..];
    let suffix = ["st", "nd", "rd", "th"]
        .iter()
        .find(|s| after_digits.to_lowercase().starts_with(**s))
        .map_or(0, |s| s.len());
    let (day, rest) = word.split_at(digits + suffix);
    if rest.is_empty() {
        vec![word]
    } else {
        vec![day, rest]
    }
}

/// The day of the month, written as a letter writes it.
///
/// "28th April 2022" is how British correspondence dates itself, and
/// `"28th".parse::<u32>()` is `None` — which is how the first real
/// photographed letter came through undated (#399), taking every
/// relative deadline in it down with the date.
///
/// The suffix is stripped only when what remains is entirely digits, so
/// a reference like `18521R` is still not a day.
fn day_number(word: &str) -> Option<u32> {
    let digits = ["st", "nd", "rd", "th"]
        .iter()
        .find_map(|suffix| word.strip_suffix(suffix))
        .filter(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(word);
    digits.parse().ok().filter(|day| (1..=31).contains(day))
}

/// A month written as a word, in full or abbreviated.
///
/// Numbers are deliberately absent. Reading "3" as March would make
/// "6.3.2026" resolve, and day-first and month-first cannot be told
/// apart — the refusal in `reading_vocabulary.rs` is what this omission
/// implements.
fn month_number(word: &str) -> Option<u32> {
    let month = match word.to_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" | "sept" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        "january" => 1,
        "february" => 2,
        "march" => 3,
        "april" => 4,
        "may" => 5,
        "june" => 6,
        "july" => 7,
        "august" => 8,
        "september" => 9,
        "october" => 10,
        "november" => 11,
        "december" => 12,
        _ => return None,
    };
    Some(month)
}

/// How much of a document's opening is searched for its own date.
///
/// Roughly a letterhead's worth: a sender's name and address, a
/// recipient's, a reference line and the date itself, with room for a
/// salutation. Long enough that a letter which puts its date under a
/// full address block still dates itself, short enough that the body
/// stays out.
///
/// Small on purpose. Missing the date leaves relative deadlines
/// undated, which a report shows as undated; taking a body date by
/// mistake anchors every one of them to the wrong day and shows the
/// results as resolved facts. The second is the worse failure, so the
/// window is sized against it. Pinned by the tests in `tests/timeline.rs`.
const OPENING_CHARS: usize = 600;

/// The document's own date: the first full date written in its opening.
/// Letters date themselves near the top; a date deep in the body ("your
/// visit of 12 January") is more likely to be somebody else's, so the
/// search deliberately stops early.
///
/// The opening is measured in characters rather than segments (#401).
/// It was three segments while a header was one segment, but how much
/// document a segment holds is a decision `document::segments_from_text`
/// makes, and it changed: line rhythm now splits an address block into
/// a segment per line, which pushed the date out of a three-segment
/// window entirely. A window counted in the unit that moved could only
/// be recalibrated, never fixed — so it is counted in the text itself,
/// which segmentation cannot move.
pub fn letter_date(segments: &[Segment]) -> Option<NaiveDate> {
    let mut read = 0usize;
    for segment in segments {
        // The first segment is always searched, however long it is: a
        // document that is one segment must still be able to date
        // itself. Line by line, because a dateline is a line — a
        // segment may hold a whole header, and the sentence below the
        // date is not the date.
        for line in segment.text.lines() {
            if let Some(date) = dateline(line) {
                return Some(date);
            }
        }
        read += segment.text.chars().count();
        if read >= OPENING_CHARS {
            break;
        }
    }
    None
}

/// Re-resolve one document's obligations against a date the person
/// settled (#412, step 4).
///
/// The model is not asked anything again, and does not need to be:
/// nothing it answered depended on the date. It read "within 14 days"
/// off the page, and the arithmetic that turns a phrase into a day is
/// Rust's (CLAUDE.md). So a confirmation costs a re-resolve, not a
/// re-run.
///
/// Scoped to one document because a run may hold several letters (#330)
/// and each has its own date. An answer about one must not silently
/// re-date another's obligations — the person was shown one letter's
/// passage and asked about that letter.
///
/// An obligation whose deadline still cannot be resolved keeps its
/// phrase and stays undated, exactly as it would have without an
/// answer. A settled date is not a licence to resolve what the words
/// never said.
pub fn confirm_letter_date(
    obligations: Vec<Obligation>,
    document: usize,
    given: NaiveDate,
) -> Vec<Obligation> {
    obligations
        .into_iter()
        .map(|mut obligation| {
            let theirs = obligation
                .evidence
                .first()
                .is_some_and(|passage| passage.document == document);
            // Only what depended on the letter's date becomes theirs
            // (#412): a period counted from the letter's date, or its
            // month end. A day written on the page, or a period counted
            // from a named day, never depended on their answer.
            if theirs
                && matches!(
                    obligation.read.counts_from.as_str(),
                    "letter_date" | "month_end"
                )
            {
                let pointed = obligation.dated_by.is_some();
                obligation.due = resolve_structured(
                    &obligation.deadline.value,
                    &obligation.read,
                    pointed,
                    Some(given),
                    Kind::Yours,
                )
                .ok();
            }
            obligation
        })
        .collect()
}

/// Two readings of one letter that do not agree about its date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateDispute {
    /// The date the reading Kettle would otherwise use gave.
    pub read: Option<NaiveDate>,
    /// The date the second reading gave — `None` if it found none.
    pub also_read: Option<NaiveDate>,
}

/// Do two readings of one photographed letter agree about its date
/// (#412, step 4)?
///
/// This is the one thing worth stopping a person for. Every relative
/// deadline in a letter is counted from the letter's own date, so a
/// single wrong digit here moves every date in the report — and moves
/// them to dates that look worked out and certain.
///
/// Compared as *dates*, not as text. The two readings differ on
/// something in most letters (the literal pass drops word spaces), and
/// a dispute over a word in the dateline that leaves the date itself
/// unchanged is not worth anybody's time. A gate that fires on those is
/// the click-through gate this design exists to avoid.
///
/// A date only one reading found is a dispute. Falling back to whichever
/// pass found one would assert every deadline in the letter off a single
/// unverified reading, which is the thing being guarded against. Two
/// readings that agree there is no date are not a dispute — a letter
/// that never dated itself is an ordinary case this pack already scores.
pub fn date_dispute(read: &[Segment], also_read: &[Segment]) -> Option<DateDispute> {
    let one = letter_date(read);
    let other = letter_date(also_read);
    (one != other).then_some(DateDispute {
        read: one,
        also_read: other,
    })
}

/// Resolve, merge and order one document's obligations.
///
/// Duplicates — the same ask, read from overlapping segments — merge
/// into one obligation keeping every piece of evidence, and the *least*
/// confident reading's confidence: two readings where one is unsure is
/// a thing a person should check, not a thing to round up.
///
/// Order is due date ascending, undated last — soonest obligations are
/// the ones a person can still act on, and an undated one must survive
/// to where they will see it.
pub fn sort_timeline(obligations: Vec<Obligation>, segments: &[Segment]) -> Vec<Obligation> {
    sort_timeline_verified(obligations, segments)
}

/// The sort itself: every reading arrived verified (`reading::check`,
/// at read time), so what is left here is to resolve, to read the row
/// a verified `at` points to, and to order.
fn sort_timeline_verified(obligations: Vec<Obligation>, segments: &[Segment]) -> Vec<Obligation> {
    let mut merged: Vec<Obligation> = Vec::new();
    for mut obligation in obligations {
        // Every reading arrived verified (`reading::check`, at read
        // time): the words at the passage that prints them, the base
        // at the passage that prints it. What is left is to check the
        // structure against the words and count (`resolve_structured`),
        // and to carry the passages `at` points to — in `dated_by` and
        // never in `evidence`, because `evidence` is what the model was
        // asked about and a row added there reads downstream as an
        // obligation asserted on a due-date row (#544, #460 rule one).
        let row = named_row(&obligation, obligation.deadline.at, segments);
        let pointed = row.is_some();
        let from = (!obligation.from.is_absent())
            .then(|| first_full_date(&obligation.from.value))
            .flatten();
        match resolve_structured(
            &obligation.deadline.value,
            &obligation.read,
            pointed,
            from,
            Kind::WorkedOut,
        ) {
            Ok(resolved) => {
                obligation.due = Some(resolved);
                obligation.dated_by = row;
                obligation.unresolved = None;
            }
            Err(why) => {
                obligation.due = None;
                obligation.dated_by = row;
                obligation.unresolved = Some(why);
            }
        }
        // The sum was verified against the passage `amount.at` at read
        // time (#460 rule one, as a whole money token). Where that
        // passage is not the ask's own, it travels as `priced_by`.
        // Nothing goes looking where the model read none. A sum is a
        // payment's alone: the page vouches that a figure is printed,
        // not that a reply slip is for money, and the 4B copies the
        // letter's sum onto its response ask on 41 of 85 such letters
        // (6 September 2026) — a pack policy, refused to derive, and
        // the report already shows none there.
        if obligation.kind == "payment" {
            if !obligation.amount.is_absent() {
                obligation.priced_by = named_row(&obligation, obligation.amount.at, segments);
            }
        } else if !obligation.amount.is_absent() {
            let own = obligation.evidence.first().map_or(0, |s| s.ordinal);
            obligation.amount = crate::reading::Reading::absent(own);
        }
        if merged.iter().any(|kept| same_candidate(kept, &obligation)) {
            continue;
        }
        merged.push(obligation);
    }
    merged.sort_by(|a, b| {
        // `None` sorts after every date: undated last, never dropped.
        // Then the page: two readings of one ask by one date tie on
        // everything else, and the letter's own order is the honest
        // tiebreak (document before ordinal — an ordinal is
        // document-local, #330).
        let key = |o: &Obligation| {
            let due = o.due.map(|resolved| resolved.date);
            let at = o
                .evidence
                .first()
                .map(|segment| (segment.document, segment.ordinal));
            (
                due.is_none(),
                due,
                o.kind.clone(),
                o.party.value.clone(),
                at,
            )
        };
        key(a).cmp(&key(b))
    });
    merged
}

/// The passage a verified reading's `at` points to, when it is not
/// the passage the claim was read from. `at` was checked at read time
/// (`reading::check`: shown in the request answered, and a passage of
/// this document), so this only asks whether it is another passage.
fn named_row(obligation: &Obligation, at: usize, segments: &[Segment]) -> Option<Segment> {
    let own = obligation.evidence.first()?;
    segments
        .get(at)
        .filter(|row| row.document == own.document && row.ordinal != own.ordinal)
        .cloned()
}

/// The one duplicate Rust may fold: the same candidate, field for
/// field, read out of the same passage of the same document — an
/// execution artefact (the model listing one ask twice in one answer),
/// not a second ask on the page.
///
/// Until the review of #626 (Task 2) this was `same_obligation`: kind,
/// party, deadline and anchor, across passages and across documents,
/// comparing neither the ask nor the sum. Two invoices to one payee by
/// one date became one obligation keeping the first sum, and nobody
/// was told. Whether two passages make *one* ask is a judgement about
/// meaning with no page to check it against (`app/METHOD.md` §1.4),
/// so Rust no longer makes it: every passage's reading is shown, and a
/// duplicate costs a person a glance where a merge cost them a sum.
fn same_candidate(a: &Obligation, b: &Obligation) -> bool {
    let same_passage = match (a.evidence.first(), b.evidence.first()) {
        (Some(x), Some(y)) => x.document == y.document && x.ordinal == y.ordinal,
        _ => false,
    };
    same_passage
        && a.kind == b.kind
        && a.party == b.party
        && a.ask == b.ask
        && a.deadline == b.deadline
        && a.read == b.read
        && a.from == b.from
        && a.amount == b.amount
}
