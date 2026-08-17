//! #399: a photographed letter reaches segments, or is refused in
//! words that fit what the person actually did.
//!
//! The judgement lives here and is pure, so it is tested on every
//! machine and in CI. The Vision call that produces a [`Reading`] is
//! macOS-only and thin by design — exactly the split `pdf` uses, where
//! extraction is gated and reconstruction is arithmetic anyone can run.

use runner::ocr::{Line, OcrError, Placed, Reading};

fn line(text: &str, confidence: f32) -> Line {
    at_line(text, confidence, 0.0)
}

fn at_line(text: &str, confidence: f32, top: f64) -> Line {
    placed_line(text, confidence, top, 0.0)
}

fn placed_line(text: &str, confidence: f32, top: f64, left: f64) -> Line {
    Line {
        text: text.to_owned(),
        confidence,
        top,
        left,
    }
}

/// A letter read cleanly, as Live Text managed on the first real one.
fn clean() -> Reading {
    Reading {
        lines: vec![
            line("Ashgrove Housing Association", 0.98),
            line("3 March 2026", 0.97),
            line("Dear Ms Okafor", 0.99),
            line(
                "Please pay £120.00 within 14 days of the date of this",
                0.96,
            ),
            line("letter.", 0.95),
        ],
    }
}

#[test]
fn a_clean_reading_becomes_the_lines_the_camera_saw() {
    // One line per line, joined the way a page is laid out — not
    // paragraphs. OCR emits line breaks and never paragraph breaks, and
    // inventing blank lines here would be inventing a structure the
    // photograph does not have. `document::segments_from_text` reads
    // the line rhythm (#401), which is why it can be given this
    // directly.
    let text = clean().text().expect("a clean page reads");

    assert_eq!(text.lines().count(), 5, "{text:?}");
    assert!(text.starts_with("Ashgrove Housing Association\n"));
    assert!(
        text.contains("within 14 days of the date of this\nletter."),
        "the wrap is left where the page put it: {text:?}"
    );
}

#[test]
fn a_page_that_read_poorly_is_refused_rather_than_half_trusted() {
    // The failure this pack cannot have. Kettle asserts deadlines from
    // letters, and digit/letter confusion is the *characteristic* OCR
    // error — the first real letter turned EC1M 5LA into ECIM SLA. The
    // same error inside a date is a wrong deadline stated at full
    // confidence, and nothing downstream can tell.
    //
    // So a page that read badly produces no reading at all. That is
    // affordable here in a way it would not be for a bank statement
    // arriving by post: the person is holding the letter and a camera,
    // and taking the photograph again is seconds of work.
    let mut poor = clean();
    poor.lines.push(line("P|ease pay £I2O.OO", 0.21));
    poor.lines.push(line("wthn l4 dys", 0.18));
    poor.lines.push(line("ECIM SLA", 0.30));

    let refused = poor.text().expect_err("a badly read page is refused");

    assert!(
        matches!(refused, OcrError::TooUncertain { .. }),
        "{refused:?}"
    );
    // The person is told what they did and what to do about it — not
    // told about confidence scores.
    let said = refused.to_string();
    assert!(
        said.contains("photo") || said.contains("picture"),
        "the refusal names what they actually did: {said:?}"
    );
}

#[test]
fn one_bad_line_on_a_good_page_does_not_lose_the_letter() {
    // The other half of the constraint. A refusal that fires on any
    // imperfection refuses every real photograph — there is always a
    // smudged reference number or a fold across one line — and a person
    // who cannot get a clean read after three tries stops using Kettle
    // rather than concluding their lighting is at fault.
    let mut nearly = clean();
    nearly.lines.push(line("Ref: HP-447l", 0.22));

    assert!(
        nearly.text().is_ok(),
        "one poor line in six must not refuse the page"
    );
}

/// Where a piece of text sat on the page, as Vision reports it:
/// normalised, origin bottom left, so a larger `top` is higher up.
fn at(top: f64, left: f64, text: &str) -> Placed {
    // Width guessed from the text, which is all these tests need: what
    // matters is whether the *next* thing starts near where this one
    // ended, or far away across a column gap.
    Placed {
        top,
        left,
        right: left + 0.012 * text.chars().count() as f64,
        line: placed_line(text, 1.0, top, left),
    }
}

#[test]
fn words_on_one_line_are_one_line_and_not_several() {
    // Found on the first real photographed letter (#399). Vision
    // returns *observations*, not lines, and it splits one where the
    // typography changes — a company name set in bold mid-sentence is
    // its own observation. Taken literally that turned
    //
    //     We (Anytown Housing Limited) hereby authorise …
    //
    // into a passage reading "We (Anytown" and another holding the
    // rest, so the model was asked its closed question about two
    // fragments of one sentence. That is #401's badly-formed question
    // arriving by a different door, and it must be closed here rather
    // than downstream, because by the time it reaches the segmenter
    // nothing distinguishes it from a genuinely short line.
    let reading = Reading::from_placed(vec![
        at(0.900, 0.10, "We (Anytown"),
        at(
            0.901,
            0.245,
            "Housing Limited) hereby authorise an inspection.",
        ),
        at(0.870, 0.10, "The surveyor will carry identification."),
    ]);

    assert_eq!(reading.lines.len(), 2, "{reading:#?}");
    assert_eq!(
        reading.lines[0].text,
        "We (Anytown Housing Limited) hereby authorise an inspection."
    );
}

#[test]
fn a_merged_line_is_only_as_certain_as_its_least_certain_part() {
    // A confidently-read fragment beside a doubtful one must not
    // launder the doubt: the floor exists to catch the doubtful part,
    // and averaging it away would hide exactly what it looks for.
    let reading = Reading::from_placed(vec![
        Placed {
            top: 0.5,
            left: 0.1,
            right: 0.2,
            line: at_line("Reference", 0.99, 0.5),
        },
        Placed {
            top: 0.5,
            left: 0.21,
            right: 0.3,
            line: at_line("HP-447l", 0.20, 0.5),
        },
    ]);

    assert_eq!(reading.lines.len(), 1);
    assert!(
        (reading.lines[0].confidence - 0.20).abs() < f32::EPSILON,
        "{:?}",
        reading.lines[0]
    );
}

#[test]
fn the_page_is_read_down_and_across_whatever_order_it_arrives_in() {
    // Vision's array order is not documented as reading order and
    // demonstrably is not one. A letter read out of order attaches a
    // deadline to the wrong ask.
    let reading = Reading::from_placed(vec![
        at(0.20, 0.1, "Yours sincerely"),
        at(0.80, 0.1, "Dear Ms Okafor"),
        at(0.50, 0.46, "and bring this letter."),
        at(0.50, 0.1, "Please attend on 3 March 2026"),
    ]);

    let text: Vec<&str> = reading.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        text,
        vec![
            "Dear Ms Okafor",
            "Please attend on 3 March 2026 and bring this letter.",
            "Yours sincerely",
        ]
    );
}

#[test]
fn a_photograph_of_nothing_says_so_in_the_words_scans_already_use() {
    // A picture of a desk, or a page so dark nothing resolved. This is
    // the same event as an image-only PDF (`PdfError::NoText`) and a
    // person meets one Kettle, so it must not grow a second vocabulary.
    let empty = Reading { lines: vec![] };

    let refused = empty.text().expect_err("no text is not a reading");
    assert!(matches!(refused, OcrError::NoText), "{refused:?}");
}

/// A page read twice, as `Correction::Applied` and `Correction::Literal`
/// would give it.
fn read_as(lines: &[(f64, &str)]) -> Reading {
    Reading {
        lines: lines
            .iter()
            .map(|(top, text)| at_line(text, 1.0, *top))
            .collect(),
    }
}

#[test]
fn two_readings_that_agree_dispute_nothing() {
    // The common case must be silent. A gate that fires on a good
    // photograph is the naive "approve this transcription" screen
    // wearing a disguise: everybody clicks through it, nobody is
    // protected, and the liability has quietly moved to them.
    let page = [
        (0.90, "Anytown Housing Association"),
        (0.85, "28th April 2026"),
        (0.80, "Please reply within 14 days."),
    ];

    let disputes = read_as(&page).disagreements(&read_as(&page));

    assert!(disputes.is_empty(), "{disputes:#?}");
}

#[test]
fn two_readings_that_disagree_mark_the_line_they_disagree_on() {
    // The whole mechanism. Confidence cannot find a wrong character —
    // the same page photographed twice read a reference number
    // differently, one digit apart, both at 1.000. Disagreement can.
    let applied = read_as(&[
        (0.90, "Anytown Housing Association"),
        (0.85, "28th April 2026"),
        (0.80, "Reference 18597121"),
    ]);
    let literal = read_as(&[
        (0.90, "Anytown Housing Association"),
        (0.85, "28th April 2026"),
        (0.801, "Reference 18597171"),
    ]);

    let disputes = applied.disagreements(&literal);

    assert_eq!(disputes.len(), 1, "{disputes:#?}");
    assert_eq!(disputes[0].read, "Reference 18597121");
    assert_eq!(disputes[0].also_read, "Reference 18597171");
}

#[test]
fn a_line_only_one_reading_saw_is_disputed() {
    // A structural disagreement is a disagreement. If one pass found a
    // line and the other did not, nothing has confirmed it, and a
    // deadline sitting in it must not pass unremarked.
    let applied = read_as(&[
        (0.90, "Please reply within 14 days."),
        (0.50, "Yours sincerely"),
    ]);
    let literal = read_as(&[(0.90, "Please reply within 14 days.")]);

    let disputes = applied.disagreements(&literal);

    assert_eq!(disputes.len(), 1, "{disputes:#?}");
    assert_eq!(disputes[0].read, "Yours sincerely");
    assert!(
        disputes[0].also_read.is_empty(),
        "the other reading saw nothing there: {disputes:#?}"
    );
}

#[test]
fn spacing_and_case_are_not_disagreements() {
    // The two passes differ in how they normalise, and a gate that
    // fires because one wrote "M14  5QT" and the other "M14 5QT" would
    // fire on every page. Only what a person would call a different
    // reading counts.
    let applied = read_as(&[(0.9, "Flat 47,  Anytown Road")]);
    let literal = read_as(&[(0.9, "Flat 47, Anytown Road ")]);

    assert!(
        applied.disagreements(&literal).is_empty(),
        "whitespace is not a disagreement"
    );
}

#[test]
fn a_missing_space_is_not_a_disagreement_either() {
    // Measured, not assumed. On a real letter the literal pass dropped
    // word spaces wholesale — "28thApril", "herebyauthorise",
    // "BuildingSafety" — which made five lines in thirty disputed on
    // spacing alone and buried the single line where the two readings
    // genuinely differed. A fifth of every page is a gate nobody reads.
    //
    // It costs nothing to ignore: a date missing a space still reads as
    // that date, to a person and to the model asked about it.
    let applied = read_as(&[(0.9, "Thursday 28th April 2026")]);
    let literal = read_as(&[(0.9, "Thursday 28thApril 2026")]);

    assert!(
        applied.disagreements(&literal).is_empty(),
        "a dropped space is the reader's habit, not a different reading"
    );

    // And the real one still surfaces: a digit read as a letter.
    let wrong = read_as(&[(0.9, "Thursday 28th Apri1 2026")]);
    assert_eq!(
        applied.disagreements(&wrong).len(),
        1,
        "a character that differs is still a disagreement"
    );
}

#[test]
fn two_columns_on_one_line_are_two_lines() {
    // Found on a real two-page letter (#399). A letter sets the
    // recipient's address on the left and the sender's on the right, so
    // both sit on the same visual line — and merging by line fused them
    // into "Mr Okafor Building 800" and "12 Anytown Road Science Park".
    //
    // Neither address survives that, and the damage is not cosmetic: an
    // address block is where a letter says who it is from and who it is
    // to, and a passage the model is asked about must be something
    // somebody wrote.
    //
    // Reading two columns *in the right order* is a larger problem and
    // is not solved here. Keeping them apart is: two intact lines in an
    // awkward order beats one line of nonsense, because a person can
    // still read either.
    let reading = Reading::from_placed(vec![
        at(0.90, 0.08, "Mr Okafor"),
        at(0.90, 0.60, "Building 800"),
        at(0.87, 0.08, "12 Anytown Road"),
        at(0.87, 0.60, "Science Park"),
    ]);

    let text: Vec<&str> = reading.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        text,
        vec![
            "Mr Okafor",
            "Building 800",
            "12 Anytown Road",
            "Science Park"
        ],
        "a column gap is not a word space"
    );
}

#[test]
fn words_a_space_apart_still_join() {
    // The other half. The reader splits an observation where the
    // typography changes, and those fragments sit a word space apart —
    // they must still become one line, or every letter with a bold
    // company name mid-sentence fragments again.
    let reading = Reading::from_placed(vec![
        at(0.90, 0.08, "We ("),
        at(0.90, 0.128, "Anytown Housing"),
    ]);

    assert_eq!(reading.lines.len(), 1, "{reading:#?}");
}

#[test]
fn two_columns_are_compared_column_against_column() {
    // Measured on a real letter, where this produced eleven "disputes"
    // in forty-eight lines and only one of them was real. An address
    // block sets the recipient left and the sender right, so two lines
    // share a top — and matching on the top alone paired
    //
    //     "Building 800"   against   "Mr Okafor"
    //
    // which is not a disagreement, it is two different sentences. Every
    // letterhead and every two-column footer generates these, so the
    // gate would fire on almost every letter and be worthless.
    //
    // The two readings need not list a shared top in the same order,
    // which is why this cannot be fixed by pairing in sequence: here
    // the second reading lists the right column first.
    let corrected = Reading {
        lines: vec![
            placed_line("Mr Okafor", 1.0, 0.90, 0.08),
            placed_line("Building 800", 1.0, 0.90, 0.60),
            placed_line("12 Anytown Road", 1.0, 0.87, 0.08),
            placed_line("Science Park", 1.0, 0.87, 0.60),
        ],
    };
    let literal = Reading {
        lines: vec![
            placed_line("Building 800", 1.0, 0.901, 0.60),
            placed_line("Mr Okafor", 1.0, 0.90, 0.08),
            placed_line("Science Park", 1.0, 0.87, 0.60),
            placed_line("12 Anytown Road", 1.0, 0.871, 0.08),
        ],
    };

    assert!(
        corrected.disagreements(&literal).is_empty(),
        "the same two columns, read twice, disagree about nothing: {:#?}",
        corrected.disagreements(&literal)
    );
}

#[test]
fn a_column_that_differs_is_still_a_disagreement() {
    // The other half: matching by position must not become so
    // forgiving that a real difference finds some other line to agree
    // with.
    let corrected = Reading {
        lines: vec![
            placed_line("Mr Okafor", 1.0, 0.90, 0.08),
            placed_line("Building 800", 1.0, 0.90, 0.60),
        ],
    };
    let literal = Reading {
        lines: vec![
            placed_line("Mr Okafor", 1.0, 0.90, 0.08),
            placed_line("Building 8OO", 1.0, 0.90, 0.60),
        ],
    };

    let disputes = corrected.disagreements(&literal);
    assert_eq!(disputes.len(), 1, "{disputes:#?}");
    assert_eq!(disputes[0].read, "Building 800");
}

#[test]
fn a_disputed_line_is_found_in_the_passage_that_contains_it() {
    // #412 step 6. A dispute is only worth a person's time if it lands
    // in a passage that produced a claim, and the passage is what the
    // review already shows. The letterhead disputes on every other
    // photograph and is never worth showing — so the question is not
    // "did this page dispute?" but "did it dispute *here*?".
    let corrected = read_as(&[
        (0.90, "Anytown Housing Association"),
        (0.85, "Please pay £120.00 by 28th April 2026."),
    ]);
    let literal = read_as(&[
        (0.90, "Anytown Housing Association"),
        (0.85, "Please pay £120.00 by 28th April 2028."),
    ]);

    let disputes = corrected.disagreements(&literal);
    assert_eq!(disputes.len(), 1, "{disputes:#?}");

    assert!(
        disputes[0].lands_in("Please pay £120.00 by 28th April 2026."),
        "the passage the disputed line was read from"
    );
    assert!(
        !disputes[0].lands_in("Dear Ms Okafor"),
        "a passage the dispute never touched"
    );
}
