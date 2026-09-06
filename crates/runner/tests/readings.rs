//! One reading shape, one verifier (`app/METHOD.md` §3; #625; review
//! of #626, Task 4). The acceptance matrix, one case per row.

use runner::document::Segment;
use runner::reading::{check, Checked, Kind, Reading, Refusal, Warning};
use std::collections::BTreeSet;

fn segment(document: usize, ordinal: usize, text: &str) -> Segment {
    Segment {
        document,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    }
}

/// A one-letter run: ids are ordinals. Passage 2 is a row-wise totals
/// line as a photograph reads one.
fn letter() -> Vec<Segment> {
    vec![
        segment(0, 0, "Selly Oak Water\n12 March 2026"),
        segment(
            0,
            1,
            "Please pay the total shown below within 14 days of the date of this letter.",
        ),
        segment(0, 2, "Sub total £300.00 VAT £60.00 Total £360.00"),
        segment(0, 3, "Yours sincerely,\nSelly Oak Water"),
    ]
}

fn shown_all() -> BTreeSet<usize> {
    (0..4).collect()
}

#[test]
fn a_sum_copied_from_a_row_of_three_is_supported_with_a_warning_only() {
    let letter = letter();
    let checked = check(
        &Reading::new(2, "£360.00"),
        Kind::Money,
        &letter[1],
        &shown_all(),
        &letter,
    );
    let Checked::Supported {
        passage,
        parses,
        warnings,
    } = checked
    else {
        panic!("supported: {checked:?}");
    };
    assert_eq!(passage.ordinal, 2, "points to the row it was read from");
    assert!(parses, "£360.00 is a sum");
    assert_eq!(
        warnings,
        vec![Warning::SeveralSumsInPassage { at: 2, sums: 3 }],
        "three sums on the row is a fact about the page, and a warning"
    );
}

#[test]
fn a_sum_the_row_does_not_print_is_refused_and_nothing_is_searched_for() {
    let letter = letter();
    let checked = check(
        &Reading::new(2, "£361.00"),
        Kind::Money,
        &letter[1],
        &shown_all(),
        &letter,
    );
    assert_eq!(checked, Checked::Refused(Refusal::NotInPassage { at: 2 }));
}

#[test]
fn a_truncated_sum_is_not_in_the_larger_figure() {
    // The explicit complete-money-token rule: a substring check accepts
    // `£360` in `£360.00`, and that is exactly the wrong sum.
    let letter = letter();
    let checked = check(
        &Reading::new(2, "£360"),
        Kind::Money,
        &letter[1],
        &shown_all(),
        &letter,
    );
    assert_eq!(checked, Checked::Refused(Refusal::NotInPassage { at: 2 }));

    // Nor is the tail of one a sum of its own.
    let row = vec![segment(0, 0, "Total £1,250.00")];
    let checked = check(
        &Reading::new(0, "£250.00"),
        Kind::Money,
        &row[0],
        &[0].into_iter().collect(),
        &row,
    );
    assert_eq!(checked, Checked::Refused(Refusal::NotInPassage { at: 0 }));
}

#[test]
fn correct_words_at_an_unseen_passage_are_refused_wherever_else_they_appear() {
    let letter = letter();
    let shown: BTreeSet<usize> = [0, 1].into_iter().collect();
    let checked = check(
        &Reading::new(2, "£360.00"),
        Kind::Money,
        &letter[1],
        &shown,
        &letter,
    );
    assert_eq!(
        checked,
        Checked::Refused(Refusal::NotShown { at: 2, shown }),
        "the model never saw passage 2 in the request it answered from"
    );
}

#[test]
fn correct_words_at_another_documents_passage_are_refused_not_named_nothing() {
    let mut run = letter();
    run.push(segment(1, 0, "Total £360.00"));
    let shown: BTreeSet<usize> = (0..5).collect();
    let checked = check(
        &Reading::new(4, "£360.00"),
        Kind::Money,
        &run[1],
        &shown,
        &run,
    );
    assert_eq!(
        checked,
        Checked::Refused(Refusal::NotThisDocument { at: 4 })
    );

    let checked = check(
        &Reading::new(9, "£360.00"),
        Kind::Money,
        &run[1],
        &(0..10).collect(),
        &run,
    );
    assert_eq!(
        checked,
        Checked::Refused(Refusal::NotThisDocument { at: 9 })
    );
}

#[test]
fn whitespace_changed_by_line_wrapping_is_still_the_same_words() {
    let letter = letter();
    let checked = check(
        &Reading::new(0, "Selly Oak Water 12 March 2026"),
        Kind::Phrase,
        &letter[1],
        &shown_all(),
        &letter,
    );
    assert!(matches!(checked, Checked::Supported { .. }), "{checked:?}");
}

#[test]
fn supported_words_the_parser_cannot_handle_are_kept_without_a_derivation() {
    let row = vec![segment(0, 0, "Amount due: three hundred pounds")];
    let checked = check(
        &Reading::new(0, "three hundred pounds"),
        Kind::Money,
        &row[0],
        &[0].into_iter().collect(),
        &row,
    );
    let Checked::Supported { parses, .. } = checked else {
        panic!("the words are on the page: {checked:?}");
    };
    assert!(!parses, "kept as words; nothing is derived from them");
}

#[test]
fn an_empty_value_is_absence_and_never_matches_anything() {
    let letter = letter();
    for kind in [Kind::Name, Kind::Money, Kind::Phrase] {
        assert_eq!(
            check(&Reading::absent(1), kind, &letter[1], &shown_all(), &letter),
            Checked::Absent
        );
        assert_eq!(
            check(
                &Reading::new(2, "  "),
                kind,
                &letter[1],
                &shown_all(),
                &letter
            ),
            Checked::Absent,
            "whitespace is not a value that substring containment can pass"
        );
    }
}

#[test]
fn a_party_is_a_name_the_letter_prints_never_case_folded_or_repaired() {
    let letter = letter();
    let checked = check(
        &Reading::new(3, "Selly Oak Water"),
        Kind::Name,
        &letter[1],
        &shown_all(),
        &letter,
    );
    let Checked::Supported { warnings, .. } = checked else {
        panic!("{checked:?}");
    };
    assert_eq!(
        warnings,
        vec![Warning::ValueInSeveralPassages { passages: 2 }],
        "the letterhead and the sign-off both print it: a warning"
    );

    let checked = check(
        &Reading::new(3, "selly oak water"),
        Kind::Name,
        &letter[1],
        &shown_all(),
        &letter,
    );
    assert_eq!(checked, Checked::Refused(Refusal::NotInPassage { at: 3 }));
}
