//! Documents read into ordered segments (#239): the Extraction
//! typology's preprocessing, as `builtin:statement-parse` is the
//! Audit typology's.
//!
//! Segmentation is pure and always compiled, exactly as PDF table
//! reconstruction is — pdfium turns a file into positioned fragments,
//! and everything after that is arithmetic this test can do without a
//! library CI does not have.

use runner::document::{media_type, read_document, segments_from_pages, segments_from_text};
use runner::ocr::{Line, Reading};
use runner::parse::ParseError;
use runner::pdf::{Fragment, Page};
use std::path::Path;

/// One line of a letter, laid out as pdfium reports it: text placed at
/// a baseline, y growing *up* the page.
fn line(text: &str, y: f32) -> Fragment {
    Fragment {
        text: text.to_owned(),
        x: 72.0,
        right: 72.0 + text.len() as f32 * 5.0,
        y,
    }
}

/// A one-page letter with three paragraphs, set 15pt apart with a
/// 30pt gap between paragraphs — ordinary business-letter leading.
fn letter() -> Page {
    Page {
        fragments: vec![
            line("Dear Ms Okafor", 700.0),
            line(
                "Thank you for getting in touch about the leak in your",
                655.0,
            ),
            line(
                "kitchen ceiling. We have logged this as a repair and an",
                640.0,
            ),
            line(
                "engineer will attend within 14 days of the date of this",
                625.0,
            ),
            line("letter.", 610.0),
            line("Please keep a note of any further damage, and take", 565.0),
            line("photographs if you are able to.", 550.0),
            line("Yours sincerely,", 505.0),
        ],
    }
}

#[test]
fn document_text_reads_a_letter_into_ordered_segments() {
    let segments = segments_from_pages(&[letter()]);

    // Four paragraphs — salutation, body, note, sign-off — not eight
    // lines and not one blob. A line is a typesetting accident; a
    // paragraph is the unit a person wrote and the unit a model can be
    // asked a question about.
    assert_eq!(
        segments.len(),
        4,
        "expected four paragraphs: {:#?}",
        segments.iter().map(|s| &s.text).collect::<Vec<_>>()
    );

    // Reading order, top of the page first.
    assert_eq!(segments[0].text, "Dear Ms Okafor");
    assert!(
        segments[1]
            .text
            .starts_with("Thank you for getting in touch"),
        "{:?}",
        segments[1].text
    );
    assert!(
        segments[2].text.starts_with("Please keep a note"),
        "{:?}",
        segments[2].text
    );
    assert_eq!(segments[3].text, "Yours sincerely,");

    // A paragraph's lines are rejoined into running prose: the model is
    // asked about a sentence, and "within 14 days" must not arrive split
    // across two segments because the page ran out of width.
    assert!(
        segments[1]
            .text
            .contains("within 14 days of the date of this letter."),
        "lines rejoin into sentences: {:?}",
        segments[1].text
    );

    // Every segment can point back at where it came from, because a
    // finding a person cannot check on the page is not evidence.
    for (ordinal, segment) in segments.iter().enumerate() {
        assert_eq!(segment.page, 1, "one-page letter");
        assert_eq!(segment.ordinal, ordinal);
        assert!(!segment.text.trim().is_empty());
    }
}

/// One fragment placed exactly, for pages whose columns matter. `line`
/// above sets everything at the same left margin, which is what a page
/// of prose looks like; a table does not.
fn at(text: &str, x: f32, y: f32) -> Fragment {
    Fragment {
        text: text.to_owned(),
        x,
        right: x + text.len() as f32 * 5.0,
        y,
    }
}

/// run-07's invoice totals, laid out as the page sets them: when it is
/// due on the left, what is owed on the right, sharing three print rows.
///
/// The left column is deliberately two rows deep against the right's
/// three, because that is what made the flattening misleading rather
/// than merely ugly — the due date landed in the gap left by a row the
/// left column does not have.
fn invoice_totals() -> Page {
    Page {
        fragments: vec![
            at("Due date", 72.0, 400.0),
            at("Sub total", 300.0, 400.0),
            at("£300", 480.0, 400.0),
            at("1 September 2026", 72.0, 385.0),
            at("VAT", 300.0, 385.0),
            at("£60", 480.0, 385.0),
            at("Total", 300.0, 370.0),
            at("£360", 480.0, 370.0),
        ],
    }
}

#[test]
fn a_table_is_not_flattened_into_a_sentence() {
    // #406. Row-wise assembly joins each print row left to right, so
    // the due date arrives between the sub total and the VAT:
    //
    //     Due date Sub total £300 1 September 2026 VAT £60 Total £360
    //
    // Presented in a blockquote, that reads as prose, and the figures
    // appear to attach to the wrong labels. The pack states no amount
    // anywhere, so this passage *is* the whole claim about the money —
    // a quote that reorders a table is an unlabelled false claim
    // carrying the authority of a direct quotation.
    let segments = segments_from_pages(&[invoice_totals()]);
    let whole: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    // The defect, stated as the issue states it: a value from one column
    // must not sit between two values of the other.
    let due = whole
        .find("1 September 2026")
        .expect("the due date is read");
    let sub_total = whole.find("£300").expect("the sub total is read");
    let vat = whole.find("£60").expect("the VAT is read");
    assert!(
        !(sub_total < due && due < vat),
        "the due date sits between the sub total and the VAT: {whole:?}"
    );

    // Stated positively, so a fix that merely reshuffles cannot pass:
    // each column holds together, and a label keeps its own value.
    assert!(
        whole.contains("Due date 1 September 2026"),
        "the due date keeps its own label: {whole:?}"
    );
    assert!(
        whole.contains("Sub total £300"),
        "the sub total keeps its own value: {whole:?}"
    );
    assert!(
        whole.contains("Total £360"),
        "the total keeps its own value: {whole:?}"
    );
}

#[test]
fn a_table_that_fills_the_page_is_not_split_into_two_columns() {
    // The failure mode the block cut can *cause*. A page that is only a
    // label/value table has one channel — between the labels and their
    // amounts — and cutting there separates every label from its value,
    // giving "Sub total VAT Total" beside "£300 £60 £360". That is the
    // original defect wearing different clothes.
    //
    // What tells the two apart is whether the sides pair row for row. A
    // table's rows all reach both sides; the invoice's do not, because
    // the totals column is a row deeper than the due date beside it.
    let page = Page {
        fragments: vec![
            at("Sub total", 72.0, 400.0),
            at("£300", 400.0, 400.0),
            at("VAT", 72.0, 385.0),
            at("£60", 400.0, 385.0),
            at("Total", 72.0, 370.0),
            at("£360", 400.0, 370.0),
        ],
    };
    let segments = segments_from_pages(&[page]);

    for (label, value) in [("Sub total", "£300"), ("VAT", "£60"), ("Total", "£360")] {
        assert!(
            segments
                .iter()
                .any(|s| s.text.contains(&format!("{label} {value}"))),
            "{label} keeps its own value: {segments:#?}"
        );
    }
    assert!(
        !segments.iter().any(|s| s.text.contains("Sub total VAT")),
        "the labels are not gathered away from their values: {segments:#?}"
    );

    // And it is carried as a table, so the report can show it as one.
    let rows: Vec<&Vec<Vec<String>>> = segments
        .iter()
        .map(|s| &s.rows)
        .filter(|rows| !rows.is_empty())
        .collect();
    assert_eq!(
        rows,
        vec![&vec![
            vec!["Sub total".to_owned(), "£300".to_owned()],
            vec!["VAT".to_owned(), "£60".to_owned()],
            vec!["Total".to_owned(), "£360".to_owned()],
        ]],
        "the whole table is one passage of cells"
    );
}

#[test]
fn prose_is_not_mistaken_for_a_table() {
    // The other way to be wrong, and the one the exit runs measure
    // across 794 existing prose fixtures: a letter has one column, so
    // column detection must not fire on it and must not reorder a word.
    let segments = segments_from_pages(&[letter()]);

    assert!(
        segments.iter().any(|s| s
            .text
            .contains("engineer will attend within 14 days of the date of this")),
        "prose reads exactly as before: {segments:#?}"
    );
    assert_eq!(
        segments.first().expect("segments").text,
        "Dear Ms Okafor",
        "the salutation is untouched"
    );
}

#[test]
fn a_text_table_is_not_flattened_into_a_sentence() {
    // A `.txt` file states no coordinates, so the geometry rule cannot
    // reach it — but the defect reproduces there exactly, because
    // aligned columns join through the same line rule. That matters
    // beyond tidiness: the letter bed is `.txt`, deliberately, so that
    // it needs no pdfium in CI. Without this the bed cannot measure the
    // fix at all.
    //
    // A monospaced page states its geometry in character positions, so
    // the rule is the same rule: a corridor of blank columns that no
    // line crosses.
    // Built as separate lines rather than one escaped literal: a `\`
    // continuation strips the next line's leading whitespace, which is
    // exactly the alignment this fixture is made of.
    let text = [
        "Due date              Sub total   £300",
        "1 September 2026      VAT          £60",
        "                      Total       £360",
    ]
    .join("\n");
    let segments = segments_from_text(&text);
    let whole: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let due = whole
        .find("1 September 2026")
        .expect("the due date is read");
    let sub_total = whole.find("£300").expect("the sub total is read");
    let vat = whole.find("£60").expect("the VAT is read");
    assert!(
        !(sub_total < due && due < vat),
        "the due date sits between the sub total and the VAT: {whole:?}"
    );
    assert!(
        whole.contains("Due date 1 September 2026"),
        "the due date keeps its own label: {whole:?}"
    );
    assert!(
        whole.contains("Sub total £300"),
        "the sub total keeps its own value: {whole:?}"
    );
}

#[test]
fn text_prose_is_not_mistaken_for_a_table() {
    // The guard, for the path that carries the whole letter bed. Two
    // spaces after a full stop must never read as a column.
    let text = "Dear Ms Okafor\n\n\
                Thank you for getting in touch.  We have logged this as a\n\
                repair.  An engineer will attend within 14 days of the date\n\
                of this letter.\n";
    let segments = segments_from_text(text);

    assert!(
        segments.iter().any(|s| s
            .text
            .contains("within 14 days of the date of this letter.")),
        "prose reads exactly as before: {segments:#?}"
    );
    assert!(
        segments.iter().all(|s| s.rows.is_empty()),
        "no passage of prose claims to be a table: {segments:#?}"
    );
}

#[test]
fn no_committed_letter_fixture_is_read_as_a_table() {
    // The false-positive risk, asked of the real bed rather than of a
    // fixture written to pass. Every committed letter is prose except
    // the one shape that is deliberately not, so any *other* passage
    // arriving with cells is column detection firing where it should
    // not — and the exit runs would pay for it in a changed
    // `segment.text` on hundreds of fixtures, which is the expensive way
    // to find this out.
    //
    // The exemption is named rather than inferred. A test that skipped
    // "anything that looks tabular" would stop being able to fail: the
    // regression it exists to catch is prose *looking* tabular.
    //
    // Reads committed fixtures only: nothing here touches `models/` or
    // `sidecars/`, so it means the same thing locally and in CI.
    const DELIBERATELY_TABULAR: &str = "invoice_totals";

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.letter-to-actions/fixtures");
    let mut checked = 0usize;
    let mut exempt = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&fixtures).expect("the letter pack's fixtures") {
        let path = entry.expect("a fixture").path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.contains(DELIBERATELY_TABULAR) {
            exempt += 1;
            continue;
        }
        checked += 1;
        let text = std::fs::read_to_string(&path).expect("a readable fixture");
        let segments = segments_from_text(&text);

        for segment in &segments {
            if !segment.rows.is_empty() {
                offenders.push(format!("{name}: read as a table {:?}", segment.rows));
            }
        }

        // Cells are only half the risk. A block cut *reorders* a page
        // without necessarily producing cells, and reordered prose is
        // what would quietly change `segment.text` on hundreds of
        // fixtures — the thing that costs a measurement cycle. Prose
        // segmentation only ever joins lines, so the words must still
        // arrive in the order the file wrote them.
        let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let read = words(
            &segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
        if read != words(&text) {
            offenders.push(format!("{name}: reading order changed"));
        }
    }

    assert!(
        checked > 700,
        "the whole letter bed was read, not a corner of it: {checked} fixtures"
    );
    assert_eq!(
        exempt, 24,
        "the tabular shape is the only exemption, and it has not quietly grown"
    );
    assert!(
        offenders.is_empty(),
        "{} of {checked} prose fixtures were read as tables:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn segments_never_span_a_page_boundary() {
    // Page breaks restart y, so a naive gap rule reads the top of page
    // two as continuous with the bottom of page one — and glues the last
    // paragraph of one page to the first of the next.
    let second = Page {
        fragments: vec![
            line("Enclosed is a copy of our repairs policy.", 700.0),
            line("It explains what we are responsible for.", 685.0),
        ],
    };
    let segments = segments_from_pages(&[letter(), second]);

    assert_eq!(segments.last().expect("segments").page, 2);
    assert!(
        segments.iter().any(|s| s.page == 1) && segments.iter().any(|s| s.page == 2),
        "both pages contribute segments"
    );
    assert!(
        !segments
            .iter()
            .any(|s| s.text.contains("Yours sincerely,") && s.text.contains("Enclosed")),
        "a paragraph must not span pages: {segments:#?}"
    );
    // Ordinals number the document, not each page: they are what a
    // batched model step counts.
    let ordinals: Vec<usize> = segments.iter().map(|s| s.ordinal).collect();
    assert_eq!(ordinals, (0..segments.len()).collect::<Vec<_>>());
}

#[test]
fn a_text_file_is_read_into_segments_without_a_pdf_library() {
    // Not every document arrives as a PDF, and a text file must not
    // need pdfium to be read — CI has none, and neither will most of
    // the machines this runs on.
    let dir = std::env::temp_dir().join(format!("kettle-document-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = dir.join("letter.txt");
    std::fs::write(
        &path,
        "Dear Ms Okafor\n\nYour appointment is on 3 March 2026 at 9.15am.\n\
         Please bring a list of your medicines.\n\nYours sincerely,\n",
    )
    .expect("write letter");

    let segments = read_document(&path, 0, None)
        .expect("a text file needs no PDF library")
        .segments;

    assert_eq!(segments.len(), 3, "{segments:#?}");
    assert_eq!(segments[0].text, "Dear Ms Okafor");
    assert!(
        segments[1]
            .text
            .contains("appointment is on 3 March 2026 at 9.15am. Please bring"),
        "the blank line separates paragraphs; a single newline does not: {:?}",
        segments[1].text
    );
    assert_eq!(segments[1].page, 1, "a text file is all one page");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_without_blank_lines_segments_into_paragraphs() {
    // #401, found on the first real letter: a housing-association
    // notice OCR'd off a photograph had 23 lines and not one blank
    // line, so the whole letter became a single passage and the pack
    // answered "nothing here asks you to do anything" — a reasonable
    // answer to a badly formed question.
    //
    // A blank line is a typing convention. A photograph has no typing
    // in it, so OCR emits line breaks and never paragraph breaks,
    // which makes this the shape every photographed letter arrives in
    // (#399).
    let letter = "Ashgrove Housing Association\n\
                  12 Bramley Road\n\
                  Dear Ms Okafor\n\
                  We will begin works on your building in March.\n\
                  Please contact us on 0161 496 0000 to arrange access.\n";

    let segments = segments_from_text(letter);

    assert!(
        segments.len() > 1,
        "a letter whose paragraphs are single lines must not collapse to one passage: {segments:#?}"
    );
    assert!(
        segments
            .iter()
            .any(|segment| segment.text.contains("arrange access")),
        "the ask must reach a passage of its own: {segments:#?}"
    );
}

#[test]
fn wrapped_prose_stays_one_paragraph() {
    // The other half of the constraint, and the reason the fix is not
    // "one segment per line": a wrapped paragraph is several lines and
    // one thought, and splitting it would hand the model half a
    // deadline to answer about.
    //
    // The document tells you its own rhythm. A line that ran to the
    // measure wrapped; a line that stopped short with room for the
    // next line's first word stopped on purpose. That is the text
    // equivalent of the median line gap the PDF path already uses.
    let letter = "You must pay the outstanding balance of £412.60 within 14 days of\n\
                  the date of this letter, or the account will be passed to our\n\
                  collections agent.\n\
                  Please quote reference HP-4471 when you pay.\n";

    let segments = segments_from_text(letter);

    assert_eq!(segments.len(), 2, "{segments:#?}");
    assert!(
        segments[0]
            .text
            .contains("within 14 days of the date of this letter"),
        "a sentence broken across lines is one sentence: {:?}",
        segments[0].text
    );
    assert_eq!(
        segments[1].text,
        "Please quote reference HP-4471 when you pay."
    );
}

#[test]
fn a_photograph_is_a_kind_of_document_a_pack_can_ask_for() {
    // #399. The letter pack's real corpus is paper, and paper reaches a
    // computer as a photograph. Until an image is a media type, a pack
    // cannot declare it in `accept`, so the file cannot even be chosen
    // — the most likely input to the pack is the one input it refuses
    // at the picker.
    assert_eq!(media_type(Path::new("letter.jpg")), Some("image/jpeg"));
    assert_eq!(media_type(Path::new("letter.jpeg")), Some("image/jpeg"));
    assert_eq!(media_type(Path::new("letter.png")), Some("image/png"));
    // What an iPhone actually produces, which is the whole point.
    assert_eq!(media_type(Path::new("IMG_4471.HEIC")), Some("image/heic"));

    // Unchanged: a type named here that nothing can read would move the
    // failure back into the run this check exists to keep it out of.
    assert_eq!(media_type(Path::new("accounts.xlsx")), None);
}

#[test]
fn a_photograph_reaches_segments_by_its_line_rhythm() {
    // The route, end to end, with the reader stubbed: what a camera
    // gives up is lines, and lines are what `segments_from_text` now
    // reads (#401). This is the join between the two halves of #399 —
    // and the reason #401 had to land first.
    let line = |text: &str, confidence: f32, top: f64| Line {
        text: text.to_owned(),
        confidence,
        top,
        left: 0.08,
    };
    let reading = Reading {
        lines: vec![
            line("Ashgrove Housing Association", 0.98, 0.90),
            line("3 March 2026", 0.97, 0.85),
            line(
                "Please pay £120.00 within 14 days of the date of",
                0.96,
                0.80,
            ),
            line("this letter.", 0.95, 0.78),
        ],
    };

    let segments = segments_from_text(&reading.text().expect("a clean page reads"));

    assert!(segments.len() > 1, "{segments:#?}");
    assert!(
        segments
            .iter()
            .any(|segment| segment.text.contains("within 14 days of the date of this letter.")),
        "the wrap the camera saw is rejoined into the sentence the model is asked about: {segments:#?}"
    );
}

#[test]
fn a_photograph_with_no_reader_says_so_without_blaming_the_photograph() {
    // A build without the reader, or a platform that has none. The
    // person did nothing wrong and must not be told to take a better
    // picture — the honest failure is that this copy of Kettle cannot
    // read pictures at all.
    let dir = std::env::temp_dir().join(format!("kettle-photo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = dir.join("letter.jpg");
    std::fs::write(&path, "not really a photograph").expect("write file");

    let error = read_document(&path, 0, None);

    // On a machine with the reader this is a decode failure; without
    // it, an unavailable reader. Either way it must not be
    // "unsupported file type" — that sentence tells a person to choose
    // a different kind of file, and they chose the right kind.
    match error {
        Err(ParseError::Ocr(_)) => {}
        other => panic!("a photograph is a document Kettle knows about: {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unsupported_document_says_so_in_the_words_statements_already_use() {
    // A spreadsheet dropped on a letter pack is a mistake a person can
    // fix, and they should hear about it in the same words the audit
    // pack uses — one vocabulary for one app.
    let dir = std::env::temp_dir().join(format!("kettle-document-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = dir.join("accounts.xlsx");
    std::fs::write(&path, "not a document").expect("write file");

    let error = read_document(&path, 0, None).expect_err("a spreadsheet is not a document");
    assert!(
        matches!(error, ParseError::UnsupportedFileType(ref what) if what == ".xlsx"),
        "{error}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
