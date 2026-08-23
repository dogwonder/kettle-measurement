//! Deadlines resolved in Rust, duplicates merged (#241).
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
    let Some(counted) = counted_from(&deadline.to_lowercase()) else {
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
    MonthEnd,
}

fn counted_from(lowered: &str) -> Option<Counted> {
    if let Some(days) = within_days(lowered) {
        return Some(Counted::Days(days));
    }
    lowered
        .contains("end of the month")
        .then_some(Counted::MonthEnd)
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

/// "within 14 days" -> 14. Calendar days only: "working days" needs a
/// bank-holiday calendar nobody has declared, and a wrong resolution
/// presented as arithmetic is worse than an honest phrase.
fn within_days(lowered: &str) -> Option<u64> {
    let rest = lowered.split("within").nth(1)?;
    let mut words = rest.split_whitespace();
    let count: u64 = words.next()?.parse().ok()?;
    match words.next()? {
        "days" | "day" => Some(count),
        _ => None,
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
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '(' | ')'))
        .filter(|w| !w.is_empty())
        .flat_map(split_run_together_day)
        .collect();
    words.windows(3).find_map(|window| {
        let day: u32 = day_number(window[0])?;
        let month = month_number(window[1])?;
        let year: i32 = window[2]
            .parse()
            .ok()
            .filter(|y| (1900..=2200).contains(y))?;
        NaiveDate::from_ymd_opt(year, month, day)
    })
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
/// Only a leading run of digits is split off, so `M14` and `18521R` are
/// untouched — neither begins with the shape a day does.
fn split_run_together_day(word: &str) -> Vec<&str> {
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 2 || digits == word.len() {
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

fn month_number(word: &str) -> Option<u32> {
    let month = match word.to_lowercase().as_str() {
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
        // itself.
        if let Some(date) = first_full_date(&segment.text) {
            return Some(date);
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
                    confirmed_deadline(&obligation.deadline, &obligation.anchor, given);
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
        obligation.due = resolve(&obligation.deadline, &obligation.anchor, letter_date);
        if obligation.due.is_none() {
            // The row travels with the claim (#460 rule one): the
            // pointing passage contains no date, so a date asserted
            // without the row beside it is a claim whose own quote does
            // not carry it. In `dated_by` and never in `evidence` —
            // `evidence` is what the model was asked about and
            // answered, and a row added there reads downstream as an
            // obligation asserted on a due-date row, which is what the
            // bed measures as an invention.
            if let Some((resolved, row)) = pointed_at(&obligation, segments) {
                obligation.due = Some(resolved);
                obligation.dated_by = Some(row);
            }
        }
        match merged
            .iter_mut()
            .find(|kept| same_obligation(kept, &obligation))
        {
            Some(kept) => {
                kept.evidence.extend(obligation.evidence);
                if obligation.confidence == crate::run::LOW_CONFIDENCE {
                    kept.confidence = obligation.confidence;
                }
            }
            None => merged.push(obligation),
        }
    }
    for obligation in &mut merged {
        // Document before ordinal: an ordinal is document-local, so
        // sorting on it alone interleaves two documents' evidence as
        // though the second continued the first (#330).
        obligation
            .evidence
            .sort_by_key(|segment| (segment.document, segment.ordinal));
        obligation.evidence.dedup();
    }
    merged.sort_by(|a, b| {
        // `None` sorts after every date: undated last, never dropped.
        let key = |o: &Obligation| {
            let due = o.due.map(|resolved| resolved.date);
            (due.is_none(), due, o.kind.clone(), o.party.clone())
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
    if !points_at_a_date(&obligation.deadline) {
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

/// One ask, however many segments said it: identity is what is being
/// asked and by when, case-insensitive, never the evidence text.
fn same_obligation(a: &Obligation, b: &Obligation) -> bool {
    a.kind == b.kind
        && a.party.eq_ignore_ascii_case(&b.party)
        && a.deadline.eq_ignore_ascii_case(&b.deadline)
        && a.anchor.eq_ignore_ascii_case(&b.anchor)
}
