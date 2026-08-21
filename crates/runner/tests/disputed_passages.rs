//! #412 step 6: a deadline read from a passage the two readings
//! disagreed about is marked for checking, in the review that already
//! exists.
//!
//! Step 4 stops the run for the letter's own date, because every
//! relative deadline is counted from it and one wrong digit moves them
//! all. Nothing else earns a screen. A deadline written on the page is
//! one claim in one passage, and the place to check it is the actions
//! screen a person already reads before exporting anything.
//!
//! The mark is decided here and never re-derived by a template (#361):
//! a screen that worked out for itself which passages were disputed
//! would be a second implementation of this rule, free to disagree
//! with it.

use runner::document::Segment;
use runner::ocr::Reading;
use runner::run::{mark_disputed, Obligation};

fn segment(ordinal: usize, text: &str) -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    }
}

fn obligation(ask: &str, deadline: &str, evidence: Segment) -> Obligation {
    Obligation {
        kind: "payment".to_owned(),
        party: "Anytown Housing Association".to_owned(),
        ask: ask.to_owned(),
        deadline: deadline.to_owned(),
        anchor: "28 April 2026".to_owned(),
        confidence: "high".to_owned(),
        due: None,
        evidence: vec![evidence],
        dated_by: None,
        disputed: vec![],
    }
}

/// Two readings of one photographed page, differing in the line that
/// carries the deadline — the shape the first real letter produced.
fn readings() -> (Reading, Reading) {
    let page = |year: &str| Reading {
        lines: vec![
            runner::ocr::Line {
                text: "Anytown Housing Association".to_owned(),
                confidence: 0.99,
                top: 0.90,
                left: 0.08,
            },
            runner::ocr::Line {
                text: format!("Please pay £120.00 by 28 April {year}."),
                confidence: 0.97,
                top: 0.60,
                left: 0.08,
            },
            runner::ocr::Line {
                text: "Please also send a meter reading.".to_owned(),
                confidence: 0.98,
                top: 0.50,
                left: 0.08,
            },
        ],
    };
    (page("2026"), page("2028"))
}

#[test]
fn a_deadline_from_a_disputed_passage_is_marked_for_checking() {
    let (corrected, literal) = readings();
    let disputes = corrected.disagreements(&literal);
    assert_eq!(disputes.len(), 1, "the fixture disputes one line");

    let paying = obligation(
        "Pay £120.00",
        "by 28 April 2026",
        segment(1, "Please pay £120.00 by 28 April 2026."),
    );
    let reading = obligation(
        "Send a meter reading",
        "no particular date",
        segment(2, "Please also send a meter reading."),
    );

    let marked = mark_disputed(vec![paying, reading], 0, &disputes);

    assert_eq!(
        marked[0].disputed.len(),
        1,
        "the deadline sat in the disputed line"
    );
    assert_eq!(
        marked[0].disputed[0].read,
        "Please pay £120.00 by 28 April 2026."
    );
    assert_eq!(
        marked[0].disputed[0].also_read,
        "Please pay £120.00 by 28 April 2028."
    );
    assert!(
        marked[1].disputed.is_empty(),
        "the meter reading's passage was read the same way twice"
    );
}

#[test]
fn a_dispute_in_the_letterhead_marks_nothing() {
    // The gate has to be rare to be read. A letterhead disputes on most
    // photographs — a logo, a strapline, a registered office — and none
    // of it is a claim Kettle makes. Marking an obligation because
    // something elsewhere on the page was uncertain is the
    // clicked-through gate arriving by the back door.
    let corrected = Reading {
        lines: vec![
            runner::ocr::Line {
                text: "Registered office: London EC1M 5LA".to_owned(),
                confidence: 0.88,
                top: 0.95,
                left: 0.08,
            },
            runner::ocr::Line {
                text: "Please pay £120.00 by 28 April 2026.".to_owned(),
                confidence: 0.97,
                top: 0.60,
                left: 0.08,
            },
        ],
    };
    let literal = Reading {
        lines: vec![
            runner::ocr::Line {
                text: "Registered office: London ECIM SLA".to_owned(),
                confidence: 0.88,
                top: 0.95,
                left: 0.08,
            },
            runner::ocr::Line {
                text: "Please pay £120.00 by 28 April 2026.".to_owned(),
                confidence: 0.97,
                top: 0.60,
                left: 0.08,
            },
        ],
    };

    let disputes = corrected.disagreements(&literal);
    assert_eq!(disputes.len(), 1, "the postcode is genuinely disputed");

    let paying = obligation(
        "Pay £120.00",
        "by 28 April 2026",
        segment(1, "Please pay £120.00 by 28 April 2026."),
    );

    let marked = mark_disputed(vec![paying], 0, &disputes);

    assert!(
        marked[0].disputed.is_empty(),
        "a disputed postcode is not a disputed deadline"
    );
}

#[test]
fn one_letters_dispute_never_marks_another_letters_deadline() {
    // A run may hold several letters (#330). Two letters from the same
    // sender share their boilerplate almost word for word — a chaser
    // repeats the original's demand — so matching a dispute by text
    // alone would let a bad photograph of one letter mark a deadline
    // read cleanly off another. The mark has to mean "this passage, in
    // this document, was read two ways".
    let (corrected, literal) = readings();
    let disputes = corrected.disagreements(&literal);

    let mine = obligation(
        "Pay £120.00",
        "by 28 April 2026",
        segment(1, "Please pay £120.00 by 28 April 2026."),
    );
    let theirs = obligation("Pay £120.00", "by 28 April 2026", {
        let mut passage = segment(1, "Please pay £120.00 by 28 April 2026.");
        passage.document = 1;
        passage
    });

    // The disputed photograph was document 0.
    let marked = mark_disputed(vec![mine, theirs], 0, &disputes);

    assert_eq!(marked[0].disputed.len(), 1, "its own letter was disputed");
    assert!(
        marked[1].disputed.is_empty(),
        "the second letter's photograph was read the same way twice"
    );
}

/// The report a person actually reads. #412 step 6 is only done when
/// the mark reaches them: a deadline the two readings disputed has to
/// say so, beside the passage it was read from.
mod report {
    use super::*;
    use runner::letter_report::build_letter_report;
    use runner::ocr::Disagreement;
    use runner::results::LetterRunInfo;
    use runner::run::ExtractionOutcome;

    fn run() -> LetterRunInfo {
        LetterRunInfo {
            id: "letter-01".to_owned(),
            pack: "app.kttl.letter-to-actions".to_owned(),
            pack_version: "0.2.0".to_owned(),
            file: "letter-01.heic".to_owned(),
            passages: 3,
            started: "2026-04-28T09:00:00Z".to_owned(),
            finished: "2026-04-28T09:00:12Z".to_owned(),
        }
    }

    #[test]
    fn a_disputed_deadline_reaches_the_report_with_both_readings() {
        let mut paying = obligation(
            "Pay £120.00",
            "by 28 April 2026",
            segment(1, "Please pay £120.00 by 28 April 2026."),
        );
        paying.disputed = vec![Disagreement {
            top: 0.60,
            read: "Please pay £120.00 by 28 April 2026.".to_owned(),
            also_read: "Please pay £120.00 by 28 April 2028.".to_owned(),
        }];

        let outcome = ExtractionOutcome {
            obligations: vec![paying],
            date_disputes: vec![],
        };

        let report = build_letter_report(&outcome, run());
        let shown = &report.obligations[0].disputed;

        assert_eq!(shown.len(), 1, "the dispute reaches the report");
        assert_eq!(shown[0].read, "Please pay £120.00 by 28 April 2026.");
        assert_eq!(shown[0].also_read, "Please pay £120.00 by 28 April 2028.");
    }

    fn html(outcome: &ExtractionOutcome) -> String {
        let template = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packs/app.kttl.letter-to-actions/report.html.tera"),
        )
        .expect("the letter pack's template");
        runner::render::render_letter_report(&template, &build_letter_report(outcome, run()))
            .expect("the letter report renders")
    }

    #[test]
    fn the_report_shows_a_person_what_the_second_reading_made_of_it() {
        // The mark is only worth carrying if it is rendered. #412 asks
        // the question beside the sentence it came from: not "was this
        // uncertain?" but "here is what the other reading said — is
        // this date right?".
        let mut paying = obligation(
            "Pay £120.00",
            "by 28 April 2026",
            segment(1, "Please pay £120.00 by 28 April 2026."),
        );
        paying.disputed = vec![Disagreement {
            top: 0.60,
            read: "Please pay £120.00 by 28 April 2026.".to_owned(),
            also_read: "Please pay £120.00 by 28 April 2028.".to_owned(),
        }];
        let disputed = ExtractionOutcome {
            obligations: vec![paying],
            date_disputes: vec![],
        };

        let shown = html(&disputed);
        assert!(
            shown.contains("Please pay £120.00 by 28 April 2028."),
            "the second reading is what makes the mark answerable"
        );

        // And the same letter, read the same way twice, says none of it.
        let agreed = ExtractionOutcome {
            obligations: vec![obligation(
                "Pay £120.00",
                "by 28 April 2026",
                segment(1, "Please pay £120.00 by 28 April 2026."),
            )],
            date_disputes: vec![],
        };
        assert!(
            !html(&agreed).contains("28 April 2028"),
            "an undisputed letter carries no mark at all"
        );
    }

    #[test]
    fn an_undisputed_deadline_says_nothing_at_all() {
        // The common case, and the one that keeps the mark worth
        // reading: on a good photograph the measured dispute rate is
        // 3%, so almost every report must carry no mark whatsoever.
        let outcome = ExtractionOutcome {
            obligations: vec![obligation(
                "Pay £120.00",
                "by 28 April 2026",
                segment(1, "Please pay £120.00 by 28 April 2026."),
            )],
            date_disputes: vec![],
        };

        let report = build_letter_report(&outcome, run());

        assert!(report.obligations[0].disputed.is_empty());
    }
}
