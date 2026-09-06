//! Deadlines resolved in Rust, soonest first (#241); one reading per
//! passage, never merged across passages (review of #626, Task 2).
//!
//! A letter says "within 14 days of the date of this letter", "by the
//! end of the month", "on 3 March 2026". The model reads those phrases
//! off the page (#240); every date below is arithmetic it never does
//! (CLAUDE.md), because a date a model invented is a missed deadline.
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

/// The date phrases this resolver understands. Deliberately small and
/// fully tested: a phrase outside the set resolves to nothing rather
/// than to a guess, and growing the set is a code change with tests,
/// never a loosening of what counts as understood.
///
/// - an absolute date written in the deadline itself ("by 12 August
///   2026") — read, not computed. Only where the phrase asks for no
///   arithmetic: a date inside a relative phrase is what the counting
///   starts from, not the answer (#435);
/// - "within N days", counted from the anchor when the anchor is a
///   date — whether that anchor arrived in its own field or was left in
///   the phrase — else from the letter's own date;
/// - "the end of the month", of that same base date's month.
pub fn resolve_deadline(deadline: &str, anchor: &str, letter_date: NaiveDate) -> Option<Resolved> {
    resolve(deadline, anchor, Some(letter_date))
}

/// [`resolve_deadline`], for a document whose own date was never found.
/// An absolute deadline still resolves — it needs no anchor — but a
/// relative one has nothing to count from and honestly stays undated.
fn resolve(deadline: &str, anchor: &str, letter_date: Option<NaiveDate>) -> Option<Resolved> {
    resolve_kinded(deadline, anchor, letter_date, Kind::WorkedOut)
}

/// A deadline resolved against a date the *person* supplied, after two
/// readings of a photographed letter disagreed about it (#412).
///
/// Only what actually depends on their answer becomes theirs. A date
/// written on the page never depended on the letter's date and stays
/// read; a deadline counted from a dated anchor counts from that
/// anchor, not from their answer. Claiming either as theirs would
/// quietly widen what their correction is taken to cover.
pub fn confirmed_deadline(deadline: &str, anchor: &str, given: NaiveDate) -> Option<Resolved> {
    resolve_kinded(deadline, anchor, Some(given), Kind::Yours)
}

/// `from_letter_date` is the kind to use when the answer was counted
/// from the document's own date, which is the only branch a supplied
/// date can reach.
fn resolve_kinded(
    deadline: &str,
    anchor: &str,
    letter_date: Option<NaiveDate>,
    from_letter_date: Kind,
) -> Option<Resolved> {
    // Whether the phrase asks for arithmetic at all is the first
    // question, because it decides how a date *inside* the phrase is
    // read. Asked before anything else, and only once (#435).
    let lowered = deadline.to_lowercase();
    // A refused phrase is refused whatever else it carries. Without
    // this the fall-through below reads "within 14 working days of 6
    // March 2026" as *6 March* — the anchor returned as the answer,
    // which is the confidently-wrong shape refusing exists to avoid.
    if refuses(&lowered) {
        return None;
    }
    let Some(counted) = counted_from(&lowered) else {
        // Nothing to count, so a date written here is the answer and the
        // page wrote it: nothing was computed.
        return first_full_date(deadline).map(|date| Resolved {
            date,
            kind: Kind::ReadAndVerified,
        });
    };

    // The base a relative phrase counts from: a dated anchor ("within
    // 14 days of the hearing on 1 June 2026") beats the letter's date.
    // Which of the two it was decides whose claim the answer is. The
    // anchor may have been left in the phrase rather than split out
    // ("within 30 days of 22 May 2026"), and a date there is the same
    // anchor by another route — never the answer itself.
    let (base, kind) = match first_full_date(anchor).or_else(|| first_full_date(deadline)) {
        Some(dated_anchor) => (dated_anchor, Kind::WorkedOut),
        None => (letter_date?, from_letter_date),
    };

    // Everything below this line is Rust's arithmetic over a date that
    // was read. The page never wrote the answer, so the report must not
    // present it in the same voice as one that did.
    let date = match counted {
        Counted::Days(days) => base.checked_add_days(Days::new(days))?,
        Counted::Months(months) => base.checked_add_months(Months::new(months))?,
        Counted::MonthEnd => end_of_month(base)?,
    };
    Some(Resolved { date, kind })
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
    MonthEnd,
}

fn counted_from(lowered: &str) -> Option<Counted> {
    if refuses(lowered) {
        return None;
    }
    interval(lowered).or_else(|| month_end_phrase(lowered).then_some(Counted::MonthEnd))
}

/// Phrases Kettle declines to count, and why.
///
/// A refusal is a design decision, not a gap: it displays the letter's
/// own words with no date beside them, which is honest, where a guess
/// would be a claim the page does not support. Both entries here are
/// cases where the arithmetic is *available* and wrong.
///
/// Checked before any counting and before any date is looked for, so a
/// phrase carrying both a refusal and a date — "within 14 working days
/// of 6 March 2026" — cannot fall through and return the anchor as
/// though it were the answer.
fn refuses(lowered: &str) -> bool {
    // Working days need a bank-holiday calendar Kettle does not have.
    // Counting them as calendar days is wrong by up to a week, and
    // wrong in the direction that makes a person late.
    lowered.contains("working day")
        || lowered.contains("business day")
        // Receipt is a day the letter does not state. Counting from the
        // letter's own date answers a question the page did not ask,
        // and presents it as worked out.
        || lowered.contains("of receipt")
        || lowered.contains("from receipt")
        || lowered.contains("receipt of")
}

/// Whether the phrase names the end of the month it counts in.
fn month_end_phrase(lowered: &str) -> bool {
    [
        "end of the month",
        "end of this month",
        "month end",
        "last day of the month",
    ]
    .iter()
    .any(|form| lowered.contains(form))
}

/// The interval a phrase counts, in whatever unit it names.
///
/// Scans for a count followed by its unit rather than keying on
/// "within", because a letter says the same thing many ways — "no later
/// than 14 days", "in the next 14 days", "14 days from the date of this
/// letter" — and the word before the number carries none of the
/// meaning. The unit does.
///
/// Keeps scanning past a count whose next word is not a unit, so a
/// phrase naming a day as well as an interval does not stop on the day.
fn interval(lowered: &str) -> Option<Counted> {
    let words: Vec<&str> = lowered.split_whitespace().collect();
    for (at, word) in words.iter().enumerate() {
        let Some(count) = count_word(word) else {
            continue;
        };
        // "calendar" and "clear" qualify the unit without changing it:
        // both mean every day, which is what Kettle counts anyway.
        let mut unit = match words.get(at + 1) {
            Some(&"calendar") | Some(&"clear") => words.get(at + 2),
            other => other,
        };
        let unit = match unit.take() {
            Some(word) => word.trim_matches(|c: char| !c.is_alphabetic()),
            None => continue,
        };
        match unit {
            "day" | "days" => return Some(Counted::Days(count)),
            "week" | "weeks" => return Some(Counted::Days(count * 7)),
            "fortnight" | "fortnights" => return Some(Counted::Days(count * 14)),
            "month" | "months" => return Some(Counted::Months(count as u32)),
            _ => continue,
        }
    }
    None
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
    /// Names no day and says where on the page one is (#544): "the
    /// date shown beside it".
    Pointed,
    /// None of the above: "as soon as you are able".
    Undated,
}

pub fn deadline_shape(deadline: &str) -> DeadlineShape {
    let lowered = deadline.to_lowercase();
    if counted_from(&lowered).is_some() {
        DeadlineShape::Counted
    } else if first_full_date(deadline).is_some() {
        DeadlineShape::Absolute
    } else if points_at_a_date(deadline) {
        DeadlineShape::Pointed
    } else {
        DeadlineShape::Undated
    }
}

/// Everything the model supplied about a deadline, before any
/// resolution (#554): the comparison a candidate that never reached the
/// resolver can be held to. Two signatures are equal exactly when the
/// resolver, given one letter date, would produce one identity from
/// them — so the ablation scorecard can say "these agree on everything
/// the model said" without running the arithmetic itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeadlineSignature {
    Counted {
        interval: Counted,
        /// The base the count starts from, where the words name one —
        /// in the anchor or left in the phrase, the same base by another
        /// route.
        base: Option<NaiveDate>,
    },
    Absolute(NaiveDate),
    Pointed(String),
    Undated {
        words: String,
        anchor_date: Option<NaiveDate>,
    },
}

pub(crate) fn deadline_signature(deadline: &str, anchor: &str) -> DeadlineSignature {
    let lowered = deadline.to_lowercase();
    if let Some(interval) = counted_from(&lowered) {
        return DeadlineSignature::Counted {
            interval,
            base: first_full_date(anchor).or_else(|| first_full_date(deadline)),
        };
    }
    if let Some(date) = first_full_date(deadline) {
        return DeadlineSignature::Absolute(date);
    }
    if points_at_a_date(deadline) {
        return DeadlineSignature::Pointed(
            lowered.split_whitespace().collect::<Vec<_>>().join(" "),
        );
    }
    DeadlineSignature::Undated {
        words: lowered.split_whitespace().collect::<Vec<_>>().join(" "),
        anchor_date: first_full_date(anchor),
    }
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
pub(crate) fn first_full_date(text: &str) -> Option<NaiveDate> {
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
            if theirs {
                obligation.due =
                    confirmed_deadline(&obligation.deadline.value, &obligation.anchor, given);
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

/// Each input document's own date, indexed by [`Segment::document`].
///
/// A run may pool several documents (#330), and "the date of this
/// letter" is a different date in each of them. Reading the first three
/// segments of the *pooled* collection answers only for whichever
/// document happened to be read first, and then silently applies that
/// answer to every other document's relative deadlines — a due date
/// months wrong, presented as resolved.
///
/// Each document's opening segments are searched on their own, so the
/// early stop in [`letter_date`] means the same thing per document as
/// it did when a run only ever had one.
pub fn document_dates(segments: &[Segment]) -> Vec<Option<NaiveDate>> {
    let count = segments
        .iter()
        .map(|segment| segment.document + 1)
        .max()
        .unwrap_or(0);
    (0..count)
        .map(|document| {
            let own: Vec<Segment> = segments
                .iter()
                .filter(|segment| segment.document == document)
                .cloned()
                .collect();
            letter_date(&own)
        })
        .collect()
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
    let document_dates = document_dates(segments);
    let mut merged: Vec<Obligation> = Vec::new();
    for mut obligation in obligations {
        // Each obligation counts from the date of the document it was
        // read out of (#330). An obligation with no evidence has no
        // document to ask, and `None` there is honest: it displays as
        // undated rather than as somebody else's date.
        let letter_date = obligation
            .evidence
            .first()
            .and_then(|segment| document_dates.get(segment.document).copied())
            .flatten();
        obligation.due = resolve(&obligation.deadline.value, &obligation.anchor, letter_date);
        if obligation.due.is_none() {
            // The row travels with the claim (#460 rule one): the
            // pointing passage contains no date, so a date asserted
            // without the row beside it is a claim whose own quote does
            // not carry it. In `dated_by` and never in `evidence` —
            // `evidence` is what the model was asked about and
            // answered, and a row added there reads downstream as an
            // obligation asserted on a due-date row, which is what the
            // bed measures as an invention.
            // The model named where the date is printed (`deadline.at`,
            // verified at read time); Rust reads one full date off that
            // passage and nothing else. The staged finder behind it is
            // the fallback only where nothing was named, never where a
            // naming was refused (#624): the model pointed, the page
            // could not vouch for it, and Rust does not then go looking.
            match named_row(&obligation, obligation.deadline.at, segments) {
                Some(row) => {
                    if let Some(date) = first_full_date(&row.text) {
                        obligation.due = Some(Resolved {
                            date,
                            kind: Kind::ReadAndVerified,
                        });
                        obligation.dated_by = Some(row);
                    }
                }
                None if !refused(&obligation, "deadline") => {
                    if let Some((resolved, row)) = pointed_at(&obligation, segments) {
                        obligation.due = Some(resolved);
                        obligation.dated_by = Some(row);
                    }
                }
                None => {}
            }
        }
        // The same shape for the sum (#612): the figure was verified
        // against the passage `amount.at` at read time (#460 rule one,
        // as a whole money token). Where that passage is not the ask's
        // own, it travels as `priced_by`. The staged finder runs only
        // where nothing was read and nothing was refused.
        // The first scratch loop (4 September) found the 4B names
        // the due-date row 27 times in 36 and the total row 4 in
        // 24, so the date finder is nearly retired and the amount
        // finder is not.
        if obligation.kind == "payment" {
            if !obligation.amount.is_absent() {
                obligation.priced_by = named_row(&obligation, obligation.amount.at, segments);
            } else if !refused(&obligation, "amount") {
                if let Some((figure, row)) = priced_at(&obligation, segments) {
                    let at = segments
                        .iter()
                        .position(|s| *s == row)
                        .unwrap_or(obligation.amount.at);
                    obligation.amount = crate::reading::Reading::new(at, figure);
                    obligation.priced_by = Some(row);
                }
            }
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

/// Deadlines that name no date but say where one is printed (#544).
///
/// Two conditions, and the second is the one doing the work. Naming
/// "the date" is common enough to mean little on its own; what makes a
/// phrase a pointer is that it also names a **direction on the page**.
/// "The date shown beside it" can only mean the layout. "The date shown
/// on your last statement" names a source instead — another document
/// this run may never have seen — and resolves to nothing, which is the
/// honest answer.
///
/// Stated this way rather than as a phrasebook because the bed's two
/// halves already word it differently ("shown beside it" against "given
/// against it"), and a rule tuned to the set it was written against
/// would leave the sealed set undated. Both lists are deliberately
/// small; growing either is a code change with a test, exactly as
/// [`counted_from`]'s set is.
fn points_at_a_date(deadline: &str) -> bool {
    const DIRECTIONS: [&str; 7] = [
        "beside",
        "against",
        "opposite",
        "alongside",
        "below",
        "above",
        "next to",
    ];
    let lower = deadline.to_lowercase();
    lower.contains("the date") && DIRECTIONS.iter().any(|where_| lower.contains(where_))
}

/// The date a pointing deadline points at, read off the same document.
///
/// The passage wanted is a due-date row and nothing else: a label the
/// page uses for the date it wants payment by, and the date itself.
/// That narrowness is the safety. A pointing phrase already had to be
/// recognised before this is asked at all, so the only way to reach a
/// wrong date is a letter that both defers to its layout and labels a
/// second date as its due date.
///
/// Nothing here is arithmetic — the answer is the page's own date,
/// which is why it comes back [`Kind::ReadAndVerified`] and not
/// [`Kind::WorkedOut`].
fn pointed_at(obligation: &Obligation, segments: &[Segment]) -> Option<(Resolved, Segment)> {
    const LABELS: [&str; 3] = ["due date", "date due", "payment due"];
    if !points_at_a_date(&obligation.deadline.value) {
        return None;
    }
    let document = obligation.evidence.first()?.document;
    segments
        .iter()
        .filter(|segment| segment.document == document)
        .find_map(|segment| {
            let lower = segment.text.to_lowercase();
            let label = LABELS.iter().find(|label| lower.starts_with(**label))?;
            let date = first_full_date(&segment.text[label.len()..])?;
            Some((
                Resolved {
                    date,
                    kind: Kind::ReadAndVerified,
                },
                segment.clone(),
            ))
        })
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

/// Whether the page refused this field's reading (review of #626,
/// Task 4): the model pointed, the page could not vouch for it, and
/// the staged finders do not then go looking.
fn refused(obligation: &Obligation, field: &str) -> bool {
    obligation.refused.iter().any(|r| r.field == field)
}

/// The sum a payment ask is for, read off the same document's own
/// labelled row when the ask's passage printed none (#612).
///
/// **Staged phrasebook** (tests/phrasebooks.rs): the fallback behind
/// `amount_from`, to go when the weekly run shows the model names the
/// row reliably.
///
/// Found on the first real letter after the field shipped: the ask
/// sentence named no figure and the page printed *Amount Due 41.21
/// GBP* two passages away. The passage wanted is a row that labels the
/// sum the page wants paid and prints it — nothing else. Labels are
/// tried in order of how specifically they name *what is owed*, and
/// the first label the document uses decides; if that label appears
/// with two different figures the page is ambiguous and nothing is
/// read, because a wrong sum is worse than a blank one. The figure is
/// copied exactly as printed — `£360.00`, `41.21 GBP` — and never
/// parsed here, so it is read-and-verified, not worked out.
fn priced_at(obligation: &Obligation, segments: &[Segment]) -> Option<(String, Segment)> {
    const LABELS: [&str; 7] = [
        "amount due",
        "total due",
        "balance due",
        "amount payable",
        "outstanding balance",
        "total",
        "balance",
    ];
    let document = obligation.evidence.first()?.document;
    let on_page: Vec<&Segment> = segments
        .iter()
        .filter(|segment| segment.document == document)
        .collect();
    for label in LABELS {
        let mut found: Option<(String, Segment)> = None;
        for segment in &on_page {
            let lower = segment.text.to_lowercase();
            let mut from = 0;
            while let Some(at) = lower[from..].find(label) {
                let start = from + at + label.len();
                from = start;
                // A label inside a longer word ("subtotal") is not this
                // label, and neither is "sub total": a sub total is
                // what the page prints *before* the sum it wants.
                let before = &lower[..from - label.len()];
                let preceded_by_letter = before
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
                if preceded_by_letter || before.trim_end().ends_with("sub") {
                    continue;
                }
                let Some(figure) = money_figure(&segment.text[start..]) else {
                    continue;
                };
                match &found {
                    Some((seen, _)) if seen != &figure => return None,
                    Some(_) => {}
                    None => found = Some((figure, (*segment).clone())),
                }
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// A printed sum at the start of `text`, after any label punctuation:
/// `£360.00`, `£1,250`, `41.21 GBP`, `€12.00`. Returned verbatim.
fn money_figure(text: &str) -> Option<String> {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '-');
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut sign = 0;
    for symbol in ["£", "€", "$"] {
        if text.starts_with(symbol) {
            sign = symbol.len();
        }
    }
    at += sign;
    if sign > 0 {
        at += text[at..].len() - text[at..].trim_start().len();
    }
    let digits_start = at;
    while at < bytes.len() && (bytes[at].is_ascii_digit() || bytes[at] == b',') {
        at += 1;
    }
    if at == digits_start || !bytes[digits_start].is_ascii_digit() {
        return None;
    }
    if at + 2 < bytes.len() + 1
        && bytes.get(at) == Some(&b'.')
        && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
        && bytes.get(at + 2).is_some_and(u8::is_ascii_digit)
    {
        at += 3;
    }
    // A figure with neither a currency sign nor pence is a count, not a
    // sum — unless a currency code follows it.
    let rest = &text[at..];
    let code = ["GBP", "EUR", "USD"]
        .iter()
        .find(|code| rest.trim_start().starts_with(**code) && rest.starts_with(' '));
    let end = match code {
        Some(code) => at + (rest.len() - rest.trim_start().len()) + code.len(),
        None => at,
    };
    let has_pence = text[digits_start..at].contains('.');
    if sign == 0 && code.is_none() && !has_pence {
        return None;
    }
    // Must end at a word boundary: "41.215" is not "41.21".
    if text[end..]
        .chars()
        .next()
        .is_some_and(char::is_alphanumeric)
    {
        return None;
    }
    Some(text[..end].to_owned())
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
        && a.anchor == b.anchor
        && a.amount == b.amount
}
