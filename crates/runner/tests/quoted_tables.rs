//! #406, the report half: a passage that came from a table is quoted as
//! a table.
//!
//! The pack states no amount anywhere — the obligations schema carries
//! `kind`, `party`, `ask`, `deadline` and `anchor`, deliberately,
//! because the model never does maths. So every number a person sees in
//! a letter report arrives through the quoted passage, and for anything
//! the schema does not model the quote *is* the whole claim.
//!
//! That inverts the usual reading of "the report only quotes, so it
//! cannot be wrong". A blockquote tells a reader these are the letter's
//! own words in the letter's own order; for a table flattened row-wise,
//! half of that is true.
//!
//! Segmentation keeping the columns apart (the other half of #406) is
//! most of the fix, and this is the rest: where the structure survived,
//! show it.

use runner::document::Segment;
use runner::letter_report::build_letter_report;
use runner::results::LetterRunInfo;
use runner::run::{ExtractionOutcome, Obligation};
use runner::timeline::Resolved;
use std::path::Path;

fn letter_template() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.letter-to-actions/report.html.tera");
    std::fs::read_to_string(&path).expect("the letter pack's report template")
}

/// run-07's invoice totals, as segmentation now hands them on: the text
/// a person can search for, and the rows the page set them in.
fn totals() -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal: 4,
        text: "Sub total £300 VAT £60 Total £360".to_owned(),
        rows: vec![
            vec!["Sub total".to_owned(), "£300".to_owned()],
            vec!["VAT".to_owned(), "£60".to_owned()],
            vec!["Total".to_owned(), "£360".to_owned()],
        ],
    }
}

fn prose() -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal: 2,
        text: "Please pay the balance shown below within 14 days.".to_owned(),
        rows: Vec::new(),
    }
}

fn html(evidence: Vec<Segment>) -> String {
    html_dated_by(evidence, None)
}

fn html_dated_by(evidence: Vec<Segment>, dated_by: Option<Segment>) -> String {
    let outcome = ExtractionOutcome {
        date_disputes: vec![],
        obligations: vec![Obligation {
            kind: "payment".to_owned(),
            party: runner::reading::Reading::new(0, "Belwood Joinery".to_owned()),
            ask: "Pay the invoice".to_owned(),
            deadline: runner::reading::Reading::new(0, "within 14 days".to_owned()),
            anchor: "the date of this letter".to_owned(),
            amount: runner::reading::Reading::absent(0),
            refused: Vec::new(),
            confidence: "high".to_owned(),
            due: Some(Resolved {
                date: "2026-09-01".parse().expect("a real day"),
                kind: runner::claim::Kind::ReadAndVerified,
            }),
            evidence,
            dated_by,
            priced_by: None,
            shown: Default::default(),
            disputed: vec![],
        }],
    };
    let run = LetterRunInfo {
        id: "letter-07".to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        pack_version: "0.1.0".to_owned(),
        file: "invoice.pdf".to_owned(),
        passages: 5,
        started: "2026-08-05T09:00:00Z".to_owned(),
        finished: "2026-08-05T09:00:12Z".to_owned(),
    };
    runner::render::render_letter_report(&letter_template(), &build_letter_report(&outcome, run))
        .expect("the letter report renders")
}

/// The cells of each `<tr>` in the rendered report, in document order.
///
/// Deliberately crude: this asks what a reader sees side by side, and
/// the answer is whatever ends up in one row of one table. Anything
/// cleverer would be re-implementing the template's own logic and could
/// agree with a bug.
fn table_rows(html: &str) -> Vec<String> {
    html.split("<tr")
        .skip(1)
        .map(|row| {
            let row = row.split("</tr>").next().unwrap_or_default();
            let mut text = String::new();
            let mut inside_tag = false;
            for character in row.chars() {
                match character {
                    '<' => inside_tag = true,
                    '>' => inside_tag = false,
                    _ if !inside_tag => text.push(character),
                    _ => {}
                }
            }
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[test]
fn a_quoted_table_renders_as_a_table() {
    let rendered = html(vec![totals()]);
    let rows = table_rows(&rendered);

    // Each label sits beside its own value, in one row a person reads
    // across. This is the assertion the issue names.
    for (label, value) in [("Sub total", "£300"), ("VAT", "£60"), ("Total", "£360")] {
        assert!(
            rows.iter()
                .any(|row| row.contains(label) && row.contains(value)),
            "{label} and {value} share a row: {rows:#?}"
        );
    }

    // And no row pairs a label with someone else's money, which is the
    // defect stated as a property rather than as a string.
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("Sub total") && (row.contains("£60") || row.contains("£360"))),
        "the sub total keeps only its own value: {rows:#?}"
    );

    // The govuk component, because a rule should say where it came from
    // and `provenance.test.ts` expects the markup to exist for what the
    // stylesheet ships.
    assert!(
        rendered.contains("govuk-table"),
        "a quoted table uses the documented table component"
    );
}

#[test]
fn a_quoted_paragraph_is_still_a_blockquote() {
    // The fallback is not a leftover: most passages are prose, and prose
    // in a table would be worse than the bug this fixes. A passage with
    // no rows renders exactly as it does today.
    let rendered = html(vec![prose()]);

    assert!(
        rendered.contains("govuk-inset-text"),
        "prose evidence stays in the inset-text block"
    );
    assert!(
        rendered.contains("Please pay the balance shown below within 14 days."),
        "the passage is quoted verbatim"
    );
    assert!(
        !table_rows(&rendered)
            .iter()
            .any(|row| row.contains("Please pay the balance")),
        "prose is not dressed as a table"
    );
}

/// A schedule whose second column is words, not money.
fn cover_terms() -> Segment {
    Segment {
        document: 0,
        page: 2,
        ordinal: 7,
        text: "Buildings Included Contents Included Accidental damage Not included".to_owned(),
        rows: vec![
            vec!["Buildings".to_owned(), "Included".to_owned()],
            vec!["Contents".to_owned(), "Included".to_owned()],
            vec!["Accidental damage".to_owned(), "Not included".to_owned()],
        ],
    }
}

#[test]
fn only_a_column_of_figures_is_set_right() {
    // govuk right-aligns numeric cells so a column of money can be
    // compared down its last digit. Applied to a column of words it is
    // simply wrong, and "every cell after the first" is a guess about
    // the data made in the copy layer — which is the thing the claim
    // marks (#366) already establish the runner must decide instead.
    let figures = table_rows(&html(vec![totals()]));
    let words = table_rows(&html(vec![cover_terms()]));

    assert!(
        !figures.is_empty() && !words.is_empty(),
        "both render as tables"
    );

    // Counted as a class *attribute*, not as a bare string: the report
    // inlines its stylesheet, so the selector is in the file either way
    // and a substring count reads three rules as three cells.
    let numeric_cells = |rendered: &str| rendered.matches("govuk-table__cell--numeric\">").count();
    assert_eq!(
        numeric_cells(&html(vec![totals()])),
        3,
        "each of the three amounts is set right"
    );
    assert_eq!(
        numeric_cells(&html(vec![cover_terms()])),
        0,
        "a column of words is not set right: {words:#?}"
    );
}

#[test]
fn a_quoted_table_still_carries_its_page() {
    // Evidence a person cannot find on the page is not evidence, and
    // that does not change because the passage has columns.
    let rendered = html(vec![totals()]);
    assert!(
        rendered.contains("Page 1"),
        "the quoted table says which page it came from"
    );
}

/// #544: a date read out of a row is shown with the row.
///
/// The pointing passage — *"Payment of the total is due by the date
/// shown beside it"* — contains no date, and the report asserts one. So
/// the passage quoted beside the claim supports every part of it except
/// the part a person is most likely to act on, which is the shape of
/// unbacked claim #460's first rule refuses. The row the resolver read
/// is what backs it, so the report has to show it.
///
/// It is shown *as a table*, for #406's reason: it is one, and a
/// blockquote would claim the page ran those characters together.
#[test]
fn a_date_read_out_of_a_row_is_shown_with_the_row() {
    let pointing = Segment {
        document: 0,
        page: 1,
        ordinal: 2,
        text: "Payment of the total is due by the date shown beside it.".to_owned(),
        rows: Vec::new(),
    };
    let row = Segment {
        document: 0,
        page: 1,
        ordinal: 3,
        text: "Due date 1 September 2026".to_owned(),
        rows: vec![
            vec!["Due date".to_owned()],
            vec!["1 September 2026".to_owned()],
        ],
    };

    let rendered = html_dated_by(vec![pointing], Some(row));

    assert!(
        rendered.contains("1 September 2026"),
        "the date's own passage reaches the page: {rendered}"
    );
    let rows = table_rows(&rendered);
    assert!(
        rows.iter().any(|row| row.contains("1 September 2026")),
        "and reaches it as the table it was set as: {rows:#?}"
    );
}

/// #612: the row a sum was read off is shown with the sum, as the
/// table it was set as, labelled as where the figure comes from.
#[test]
fn a_sum_read_out_of_a_row_is_shown_with_the_row() {
    let ask = Segment {
        document: 0,
        page: 1,
        ordinal: 2,
        text: "Unless payment of all overdue invoices is received within 7 calendar days, \
                we may commence legal action."
            .to_owned(),
        rows: Vec::new(),
    };
    let row = Segment {
        document: 0,
        page: 1,
        ordinal: 3,
        text: "Amount Due 41.21 GBP".to_owned(),
        rows: vec![vec!["Amount Due".to_owned()], vec!["41.21 GBP".to_owned()]],
    };
    let mut outcome = ExtractionOutcome {
        date_disputes: vec![],
        obligations: vec![Obligation {
            kind: "payment".to_owned(),
            party: runner::reading::Reading::new(ask.ordinal, "Halverson Parcels Ltd".to_owned()),
            ask: "Pay all overdue invoices".to_owned(),
            deadline: runner::reading::Reading::new(
                ask.ordinal,
                "within 7 calendar days".to_owned(),
            ),
            anchor: "no particular date".to_owned(),
            amount: runner::reading::Reading::new(ask.ordinal, "41.21 GBP".to_owned()),
            refused: Vec::new(),
            confidence: "high".to_owned(),
            due: None,
            evidence: vec![ask],
            dated_by: None,
            priced_by: Some(row),
            shown: Default::default(),
            disputed: vec![],
        }],
    };
    outcome.obligations[0].ask = "Pay all overdue invoices".to_owned();
    let run = LetterRunInfo {
        id: "letter-08".to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        pack_version: "0.1.0".to_owned(),
        file: "reminder.jpg".to_owned(),
        passages: 5,
        started: "2026-09-04T09:00:00Z".to_owned(),
        finished: "2026-09-04T09:00:12Z".to_owned(),
    };
    let rendered = runner::render::render_letter_report(
        &letter_template(),
        &build_letter_report(&outcome, run),
    )
    .expect("the letter report renders");
    assert!(
        rendered.contains("The amount comes from this"),
        "the row is labelled as where the figure comes from: {rendered}"
    );
    let rows = table_rows(&rendered);
    assert!(
        rows.iter().any(|row| row.contains("41.21 GBP")),
        "and reaches the page as the table it was set as: {rows:#?}"
    );
}
