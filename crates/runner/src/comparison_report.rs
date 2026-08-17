//! The Comparison typology's report document (#66).
//!
//! [`crate::aggregate`] does this for an audit and
//! [`crate::letter_report`] for a letter. This is the same job for a
//! comparison, and a separate function for the same reason: the three
//! answer different questions, and a shared shape with half its fields
//! empty makes every reader guess which half meant anything (#238).
//!
//! Nothing here reads and nothing here subtracts. [`crate::terms`] has
//! already paired on the closed enum and done the arithmetic (#350);
//! this arranges what it decided, formats it once, and hands it on. The
//! template reformats nothing — its own rule, and the reason the money
//! arrives here as words rather than as a number to be styled later.

use crate::fmt::{self, count};
use crate::results::{
    ChangeState, ComparisonReport, ComparisonRunInfo, ComparisonSummary, Direction,
    NeedsReviewPassage, TermChangeOut, COMPARISON_REPORT_SCHEMA,
};
use crate::run::{ComparisonOutcome, ReviewItem};
use crate::terms::{NotComparedWhy, TermChange, TermDiff};

/// Build the comparison run's report document.
pub fn build_comparison_report(
    outcome: &ComparisonOutcome,
    review: &[ReviewItem],
    run: ComparisonRunInfo,
) -> ComparisonReport {
    let mut changes: Vec<TermChangeOut> = outcome
        .diff
        .iter()
        .map(|diff| row(diff, &run.documents))
        .collect();
    // `diff_terms` orders by term name, which is what makes two runs of
    // the same documents comparable. It is not the order a person came
    // for: they came to find what moved. Sorting is stable, so within a
    // group the diff's own order survives.
    changes.sort_by_key(|change| match change.state {
        ChangeState::Changed => 0,
        ChangeState::Added => 1,
        ChangeState::Removed => 2,
        ChangeState::Unchanged => 3,
    });

    let count = |state: ChangeState| changes.iter().filter(|row| row.state == state).count();
    let changed_count = count(ChangeState::Changed);
    let added_count = count(ChangeState::Added);
    let removed_count = count(ChangeState::Removed);
    let unchanged_count = count(ChangeState::Unchanged);

    let mut needs_review: Vec<NeedsReviewPassage> = review.iter().map(Into::into).collect();

    // Terms Rust declined to compare (#377), under the same heading. A
    // person looking for "did my excess change?" needs them in the
    // place they already look, and needs to be told *why* — the
    // difference between believing nothing changed and knowing Kettle
    // refused to guess.
    //
    // One entry per passage, because that is what this report renders.
    // Grouping the three passages behind one claim is #379's job, and
    // doing it here would be a second evidence layout to keep in step.
    for refused in &outcome.not_compared {
        let reason = not_compared_reason(&refused.term, refused.readings, &refused.why);
        for quote in &refused.quotes {
            needs_review.push(NeedsReviewPassage {
                text: quote.clone(),
                reason: reason.clone(),
                note: crate::aggregate::NOT_COUNTED_NOTE.to_owned(),
            });
        }
    }

    ComparisonReport {
        schema: COMPARISON_REPORT_SCHEMA.to_owned(),
        summary: ComparisonSummary {
            terms_count: changes.len(),
            changed_count,
            unchanged_count,
            added_count,
            removed_count,
            needs_review_count: needs_review.len(),
            note: note(
                changed_count,
                added_count,
                removed_count,
                needs_review.len(),
            ),
        },
        changes,
        needs_review,
        run,
    }
}

/// One diff row as the report shows it. `documents` resolves each
/// quote's document index to the pack's own label for it (#379) —
/// "Last year's policy", never the role key or a file name. An index
/// past the list is named honestly rather than guessed at or dropped:
/// a passage a reader cannot place is still a passage they can read.
fn row(diff: &TermDiff, documents: &[crate::results::ComparedDocument]) -> TermChangeOut {
    let (state, from, to, delta, direction) = match &diff.change {
        TermChange::Unchanged { value } => (
            ChangeState::Unchanged,
            Some(value.clone()),
            Some(value.clone()),
            None,
            None,
        ),
        TermChange::Changed { from, to, delta } => {
            // Size and direction are separate because "£65.50 more" is
            // what a person reads. The size is the move's magnitude:
            // the sign is the direction, and printing both would say it
            // twice in two vocabularies.
            let size = delta.map(|delta| fmt::money(delta.abs()));
            let direction = delta.map(|delta| {
                if delta.is_sign_negative() {
                    Direction::Down
                } else {
                    Direction::Up
                }
            });
            (
                ChangeState::Changed,
                Some(from.clone()),
                Some(to.clone()),
                size,
                direction,
            )
        }
        TermChange::Added { value } => (ChangeState::Added, None, Some(value.clone()), None, None),
        TermChange::Removed { value } => {
            (ChangeState::Removed, Some(value.clone()), None, None, None)
        }
    };

    // One side per compared document, in document order, and always
    // both — the earlier document's silence about an added term is
    // what makes it added, so a side with no value still renders.
    let sides = [from.clone(), to.clone()]
        .into_iter()
        .enumerate()
        .map(|(index, value)| crate::results::TermSideOut {
            label: document_label(documents, index),
            quote: diff
                .quotes
                .iter()
                .find(|quote| quote.document == index)
                .map(|quote| quote.text.clone()),
            value,
        })
        .collect();

    TermChangeOut {
        term: diff.term.clone(),
        label: label(&diff.term),
        basis: diff.basis.clone(),
        basis_label: basis_label(&diff.basis),
        state,
        from,
        to,
        delta,
        direction,
        kind: diff.kind,
        sides,
    }
}

/// The pack's own label for one of the run's documents. An index past
/// the list is named honestly rather than guessed at or dropped: a
/// passage a reader cannot place is still a passage they can read.
fn document_label(documents: &[crate::results::ComparedDocument], index: usize) -> String {
    documents
        .get(index)
        .map(|document| document.label.clone())
        .unwrap_or_else(|| format!("Document {}", index + 1))
}

/// Why a term was read and not compared, in a person's words (#377).
///
/// "Compulsory excess appears 3 times in this document, so Kettle
/// hasn't compared it." The count and the term are the whole of it: a
/// reader who is told only that something needs their eyes cannot tell
/// "Kettle looked and found nothing" from "Kettle refused to guess",
/// and those are opposite facts about their policy.
///
/// Lives here rather than in [`crate::terms`] for the reason the labels
/// do: that module decides what is true, this one decides how it is
/// said.
pub(crate) fn not_compared_reason(term: &str, readings: usize, why: &NotComparedWhy) -> String {
    match why {
        NotComparedWhy::StatedMoreThanOnce => format!(
            "{} appears {} times in this document, so Kettle hasn't compared it.",
            label(term),
            readings
        ),
        // #461: the two documents call the same kind of value different
        // things. Saying only "not compared" would leave a person
        // looking for a repetition that is not there — the fact is
        // about the two documents disagreeing, and it is the fact that
        // tells them what to go and check.
        NotComparedWhy::LabelledDifferently { earlier, later } => format!(
            "Last year's document calls this {} and this year's calls it {}. Kettle can't tell \
             whether the amount changed or only its name did, so it hasn't compared them.",
            spoken_list(earlier),
            spoken_list(later)
        ),
    }
}

/// "a compulsory excess", or "a compulsory excess and a voluntary
/// excess" — the labels a document used, as somebody would say them.
fn spoken_list(terms: &[String]) -> String {
    let named: Vec<String> = terms
        .iter()
        .map(|term| format!("a {}", label(term).to_lowercase()))
        .collect();
    match named.as_slice() {
        [] => "nothing".to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `compulsory_excess` → "Compulsory excess".
///
/// Derived from the pack's own enum rather than declared beside it, for
/// #367's reason: a declared label can disagree with the value it names
/// and nothing would catch it. It also means a pack that adds a term
/// gets words for it without a second edit somewhere else — the failure
/// mode being a row that renders blank because only one of the two
/// places was updated.
fn label(term: &str) -> String {
    let spaced = term.replace('_', " ");
    let mut characters = spaced.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => spaced,
    }
}

/// The basis as it reads in a sentence. Unknown bases fall through to
/// their own wording rather than to a guess: a pack may declare a basis
/// this function has never heard of, and printing it plainly is honest
/// where inventing a phrase for it would not be.
fn basis_label(basis: &str) -> String {
    match basis {
        "annual" => "a year".to_owned(),
        "monthly" => "a month".to_owned(),
        "per_claim" => "per claim".to_owned(),
        "per_policy" => "per policy".to_owned(),
        other => other.replace('_', " "),
    }
}

/// The summary in words, from the counts alone — the deterministic
/// fallback that makes an absent prose step honest (#33, #258).
///
/// Plain British English, and no advice: whether a rise is worth
/// shopping around for is the person's call, not Kettle's.
fn note(changed: usize, added: usize, removed: usize, review: usize) -> String {
    let mut sentences = Vec::new();
    if changed == 0 && added == 0 && removed == 0 {
        sentences.push(
            "Comparing the two documents, nothing Kettle reads has changed between them."
                .to_owned(),
        );
    } else {
        let mut moves = Vec::new();
        if changed > 0 {
            moves.push(format!("{} changed", count(changed, "value", "values")));
        }
        if added > 0 {
            moves.push(format!(
                "{} {} only in this year's document",
                count(added, "value", "values"),
                if added == 1 { "appears" } else { "appear" }
            ));
        }
        if removed > 0 {
            moves.push(format!(
                "{} {} gone from it",
                count(removed, "value", "values"),
                if removed == 1 { "is" } else { "are" }
            ));
        }
        sentences.push(format!("{}.", sentence_list(&moves)));
    }
    if review > 0 {
        sentences.push(format!(
            "{} {} your eyes.",
            count(review, "passage", "passages"),
            if review == 1 { "needs" } else { "need" }
        ));
    }
    sentences.join(" ")
}

/// "a, b and c" — the last join is a word, because a person reads this.
fn sentence_list(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
