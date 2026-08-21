//! The Extraction typology's report document (#243).
//!
//! [`crate::aggregate`] does this for an audit: turn a `RunOutcome`
//! into the document the template and the app read. This is the same
//! job for a letter, and it is a separate function rather than a
//! branch inside that one because the two answer different questions.
//! An audit totals money; a letter lists what somebody must do. A
//! shared shape with half its fields empty would make every reader
//! guess which half meant anything.
//!
//! Nothing here computes a date. `builtin:timeline-sort` (#241) has
//! already resolved what could be resolved, and an obligation whose
//! deadline it declined arrives with `due: None` — which this carries
//! through as an obligation to *check*, never as one due today.

use crate::fmt::count;
use crate::results::{
    CellOut, Confidence, LetterReport, LetterRunInfo, LetterSummary, NeedsReviewPassage,
    ObligationOut, PassageOut, LETTER_REPORT_SCHEMA,
};
use crate::run::{ExtractionOutcome, ReviewItem};

/// Build the letter run's report document.
///
/// Order is the timeline's: soonest first, undated last. The report
/// does not re-sort, because the order a person should act in was
/// decided by the step that resolved the dates.
pub fn build_letter_report(outcome: &ExtractionOutcome, run: LetterRunInfo) -> LetterReport {
    build_letter_report_with_review(outcome, &[], run)
}

/// [`build_letter_report`], carrying the passages nobody could answer
/// for. They are counted in `needs_review_count` and nowhere else.
pub fn build_letter_report_with_review(
    outcome: &ExtractionOutcome,
    review: &[ReviewItem],
    run: LetterRunInfo,
) -> LetterReport {
    let obligations: Vec<ObligationOut> = outcome
        .obligations
        .iter()
        .map(|obligation| ObligationOut {
            kind: obligation.kind.clone(),
            party: obligation.party.clone(),
            ask: obligation.ask.clone(),
            deadline: obligation.deadline.clone(),
            due: obligation.due.map(Into::into),
            confidence: Confidence::parse(&obligation.confidence),
            evidence: obligation
                .evidence
                .iter()
                .map(|segment| PassageOut {
                    page: segment.page,
                    text: segment.text.clone(),
                    rows: cells(&segment.rows),
                })
                .collect(),
            dated_by: obligation.dated_by.as_ref().map(|segment| PassageOut {
                page: segment.page,
                text: segment.text.clone(),
                rows: cells(&segment.rows),
            }),
            disputed: obligation.disputed.iter().map(Into::into).collect(),
        })
        .collect();

    let dated_count = obligations.iter().filter(|o| o.due.is_some()).count();
    let undated_count = obligations.len() - dated_count;
    let needs_review: Vec<NeedsReviewPassage> = review.iter().map(Into::into).collect();

    LetterReport {
        schema: LETTER_REPORT_SCHEMA.to_owned(),
        summary: LetterSummary {
            obligations_count: obligations.len(),
            dated_count,
            undated_count,
            needs_review_count: needs_review.len(),
            note: note(obligations.len(), undated_count, needs_review.len()),
        },
        obligations,
        needs_review,
        run,
    }
}

/// The summary in words, from the counts alone — the deterministic
/// fallback that makes an absent prose step honest (#33).
///
/// A quoted table's cells, each knowing whether its column is figures
/// (#406).
///
/// The judgement is made per **column**, never per cell. A column is
/// figures only if every cell in it that says anything is one, so a
/// stray `£` in a column of prose cannot right-align the paragraph
/// beside it, and one blank cell in a money column cannot stop it
/// lining up.
fn cells(rows: &[Vec<String>]) -> Vec<Vec<CellOut>> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
    let numeric: Vec<bool> = (0..columns)
        .map(|column| {
            let mut said_something = false;
            for cell in rows.iter().filter_map(|row| row.get(column)) {
                if cell.trim().is_empty() {
                    continue;
                }
                said_something = true;
                if !is_figure(cell) {
                    return false;
                }
            }
            said_something
        })
        .collect();

    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(column, text)| CellOut {
                    text: text.clone(),
                    numeric: numeric.get(column).copied().unwrap_or_default(),
                })
                .collect()
        })
        .collect()
}

/// Is this cell a figure a person would compare with another figure?
///
/// Deliberately narrow. The question is only whether a column lines up
/// on its last digit, and the cost of a wrong yes — a column of words
/// set hard right — is worse than the cost of a wrong no, which is a
/// money column set left and perfectly readable.
fn is_figure(cell: &str) -> bool {
    let bare: String = cell
        .chars()
        .filter(|character| !matches!(character, '£' | '$' | '€' | ',' | ' ' | '%'))
        .collect();
    !bare.is_empty() && bare.parse::<f64>().is_ok()
}

/// Plain British English, no advice: the actions do advice.
fn note(total: usize, undated: usize, review: usize) -> String {
    if total == 0 {
        return "Kettle found nothing in this letter that asks you to do anything.".to_owned();
    }
    let mut sentences = vec![format!(
        "This letter asks you to do {}.",
        count(total, "thing", "things")
    )];
    if undated > 0 {
        sentences.push(format!(
            "{} of them {} no date Kettle could work out, so {} shown with the letter's own words instead.",
            undated,
            if undated == 1 { "has" } else { "have" },
            if undated == 1 { "it is" } else { "they are" },
        ));
    }
    if review > 0 {
        sentences.push(format!(
            "{} needs your eyes.",
            count(review, "passage", "passages")
        ));
    }
    sentences.join(" ")
}
