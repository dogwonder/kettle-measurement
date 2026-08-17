//! Documents read into ordered segments (#239, part of #51): the
//! Extraction typology's preprocessing, as `parse` is the Audit
//! typology's.
//!
//! A letter has no rows, no amounts and no merchants. It has prose, in
//! an order that matters, and the unit worth asking a model about is
//! the paragraph — a line is where the page ran out of width, which is
//! a fact about typesetting rather than about what was written.
//!
//! Deliberately generalised. The name says `document`, not `letter`,
//! because tenancy agreements (#59), T&Cs (#57) and correspondence
//! (#92) all want the same step; a `builtin:letter-text` would be the
//! pack-specific runner code #51 exists to refuse.
//!
//! Extraction itself (pdfium) is feature-gated in [`crate::pdf`]; the
//! segmentation here is pure and always compiled, so it stays under
//! ordinary `cargo test` on a machine — and a CI — with no PDF library.

use crate::parse::ParseError;
use crate::pdf::{Fragment, Page};
use std::path::Path;

/// One passage of a document, with enough provenance to point back at
/// the page it came from.
///
/// No character offsets: a segment carries its own text, and evidence
/// means showing a person this passage on that page. Offsets into a
/// reconstructed whole-document string would only be meaningful if the
/// runner kept that string, and the model is never shown it (CLAUDE.md).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    /// Which input document this came from, from 0, in the order the
    /// run was given them (#330).
    ///
    /// A run may be given several documents, and `page` and `ordinal`
    /// are both document-local — two letters each have a page 1 and an
    /// ordinal 0. Without this, pooling them loses the only thing that
    /// tells them apart, and "the date of this letter" stops having an
    /// answer.
    pub document: usize,
    /// 1-based page number, as a person would cite it.
    pub page: usize,
    /// Position within its own document, from 0. This is what a batched
    /// model step counts, so it must not restart per page. It *does*
    /// restart per document, which is why `document` sorts before it.
    pub ordinal: usize,
    pub text: String,
    /// The rows this passage was set in, where it came from a table
    /// (#406) — empty for prose, which is nearly every passage.
    ///
    /// `text` stays canonical and stays first. Both of #460's rules ask
    /// whether a quote is *in* the document, via `quote_is_in`'s
    /// whitespace-squashed containment, and a passage that answered
    /// only in cells would have nothing for them to read. So structure
    /// is carried *beside* the text, never instead of it: everything
    /// scoring, joining and validation already do is untouched, and the
    /// report gains the one thing it could not reconstruct.
    ///
    /// Serde-defaulted, so every recording made before this existed
    /// still loads and reads as what it was — prose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Vec<String>>,
}

/// Lines closer together than this multiple of the document's usual
/// line spacing belong to one paragraph; a wider gap starts a new one.
///
/// 1.4 sits between the two things it has to tell apart: ordinary
/// leading (1.0–1.2× the line height) and the blank line between
/// paragraphs (roughly 2×). Pinned by the tests in `tests/document.rs`.
const PARAGRAPH_GAP: f32 = 1.4;

/// Read one document file into ordered segments.
///
/// Dispatch mirrors [`crate::parse::parse_input_file`] deliberately,
/// down to sharing [`ParseError`]: a person meets one Kettle, and a
/// scan dropped on a letter pack must get the same sentence a scan
/// dropped on the audit pack gets (#71's "this looks like a scan", not
/// a second wording invented here). What differs between the two packs
/// is what the text becomes, not what a file is.
///
/// `document` is this file's position in the run's input list, stamped
/// on every segment it produces. It is a required argument rather than
/// something a caller may remember to add afterwards: pooling several
/// documents without it is exactly the defect #330 exists to close, and
/// a defaulted 0 would let that mistake compile.
pub fn read_document(
    path: &Path,
    document: usize,
    pdfium_dir: Option<&Path>,
) -> Result<DocumentRead, ParseError> {
    let mut read = read_one(path, pdfium_dir)?;
    for segment in &mut read.segments {
        segment.document = document;
    }
    Ok(read)
}

/// One document, read — and what reading it a second time disagreed
/// about.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentRead {
    pub segments: Vec<Segment>,
    /// Only a photograph produces this (#412). A file with a text layer
    /// is read once, because reading it twice would give the same bytes
    /// twice — there is nothing to disagree about, and pretending to
    /// check would be theatre.
    pub date_dispute: Option<crate::timeline::DateDispute>,
    /// Lines the two readings did not agree about (#412 step 6), for
    /// the pipeline to attach to whichever passages they landed in.
    ///
    /// Separate from `date_dispute` because they answer different
    /// questions at different times. The letter's date stops the run —
    /// every relative deadline counts from it. A disputed line is a
    /// mark on one claim, shown in the review that already exists, and
    /// only if a claim was read from it. Empty for anything that was
    /// not a photograph.
    pub disagreements: Vec<crate::ocr::Disagreement>,
}

impl From<Vec<Segment>> for DocumentRead {
    fn from(segments: Vec<Segment>) -> Self {
        DocumentRead {
            segments,
            date_dispute: None,
            disagreements: Vec::new(),
        }
    }
}

/// What type of document a file is, by its name (#334 §2).
///
/// The vocabulary is the manifest's: a pack declares `accept` in media
/// types, so the binding check has to speak the same language. It reads
/// the extension and nothing else — sniffing the bytes is a different
/// and larger promise, and the honest failure of this one is "I cannot
/// tell", not a guess.
///
/// `None` for an extension no Kettle preprocessor reads, so a caller
/// can say "not a type this role takes" rather than assuming it passes.
///
/// Wider than [`read_one`] below on purpose: `text/csv` is read by
/// `builtin:statement-parse`, not by the document reader, and a
/// binding check has to speak for every preprocessor a pack can
/// declare. What must not happen is the reverse — a type named here
/// that nothing can read, which would move the failure back into the
/// run this check exists to keep it out of.
pub fn media_type(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "csv" => Some("text/csv"),
        "txt" => Some("text/plain"),
        "md" | "markdown" => Some("text/markdown"),
        "pdf" => Some("application/pdf"),
        // A photograph of a letter (#399). Not an edge case: the
        // letters that carry obligations arrive on paper, and a person
        // holding one has a phone in the other hand. `heic` is what an
        // iPhone produces without being asked, so leaving it out would
        // refuse the commonest file of all.
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "heic" | "heif" => Some("image/heic"),
        _ => None,
    }
}

/// Is this a picture of a document rather than a document?
fn is_image(extension: &str) -> bool {
    matches!(extension, "jpg" | "jpeg" | "png" | "heic" | "heif")
}

/// One document's segments, numbered from 0 within itself. Every caller
/// goes through [`read_document`], which is where they learn which
/// document they belong to.
fn read_one(path: &Path, pdfium_dir: Option<&Path>) -> Result<DocumentRead, ParseError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "txt" | "md" | "markdown" => {
            let text = std::fs::read_to_string(path).map_err(ParseError::Io)?;
            Ok(segments_from_text(&text).into())
        }
        "pdf" => {
            #[cfg(feature = "pdf")]
            {
                let pdfium_dir =
                    pdfium_dir.ok_or(ParseError::Pdf(crate::pdf::PdfError::LibraryMissing))?;
                // `extract_pages` is where an image-only PDF becomes
                // `PdfError::NoText` (#71). Passing it straight through
                // is the point: the scan message is already written and
                // already tested.
                let pages = crate::pdf::extract_pages(path, pdfium_dir)?;
                Ok(segments_from_pages(&pages).into())
            }
            #[cfg(not(feature = "pdf"))]
            {
                let _ = pdfium_dir;
                Err(ParseError::Pdf(crate::pdf::PdfError::LibraryMissing))
            }
        }
        // A photograph reaches the same segmentation everything else
        // does (#399). What a camera gives up is lines, and lines are
        // what `segments_from_text` reads — which is why #401 had to
        // land first, and why there is no picture-shaped preprocessing
        // below this line.
        // A photograph is read twice and the readings compared (#412).
        // Confidence cannot find a wrong character — the same page
        // photographed twice read a reference number differently, one
        // digit apart, both at 1.000 — so the check is disagreement,
        // not a threshold. The corrected reading is the one the run
        // uses; the literal one exists to contradict it.
        image if is_image(image) => {
            use crate::ocr::Correction;
            let read =
                |correction| crate::ocr::read_page_with(path, correction).map_err(ParseError::Ocr);
            let corrected_page = read(Correction::Applied)?;
            // A second pass that fails is not a contradiction of the
            // first — it is the check being unavailable, and the honest
            // answer is that nothing confirmed the date.
            let literal_page = read(Correction::Literal).ok();
            let corrected = segments_from_text(&corrected_page.text().map_err(ParseError::Ocr)?);
            let literal = literal_page
                .as_ref()
                .and_then(|page| page.text().ok())
                .map(|text| segments_from_text(&text))
                .unwrap_or_default();
            let date_dispute = crate::timeline::date_dispute(&corrected, &literal);
            // Line by line, from the readings rather than the segments:
            // a segment is paragraphs joined, and joining is what
            // destroys the position the two readings are matched on.
            let disagreements = literal_page
                .as_ref()
                .map(|literal| corrected_page.disagreements(literal))
                .unwrap_or_default();
            Ok(DocumentRead {
                segments: corrected,
                date_dispute,
                disagreements,
            })
        }
        "" => Err(ParseError::UnsupportedFileType(
            "no filename extension".to_owned(),
        )),
        other => Err(ParseError::UnsupportedFileType(format!(".{other}"))),
    }
}

/// Ordered segments for a whole document, page boundaries respected.
///
/// Pages are segmented independently and then numbered as one document.
/// Both halves matter: y restarts at every page break, so a gap rule
/// applied across pages reads the top of page two as continuous with
/// the bottom of page one; and a model step batches over the document,
/// so the ordinals it counts cannot restart either.
pub fn segments_from_pages(pages: &[Page]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        for passage in paragraphs(page) {
            segments.push(Segment {
                document: 0,
                page: index + 1,
                ordinal: segments.len(),
                text: passage.text,
                rows: passage.rows,
            });
        }
    }
    segments
}

/// Ordered segments for a plain-text or Markdown document.
///
/// A blank line is an explicit paragraph break and is always honoured.
/// It cannot be the *only* rule, which is what #401 cost us: a real
/// letter, OCR'd off a photograph, had 23 lines and no blank line at
/// all, so the whole document became one passage and the pack answered
/// "nothing here asks you to do anything" — a reasonable answer to a
/// badly formed question. A blank line is a typing convention and a
/// photograph has no typing in it, so OCR emits line breaks and never
/// paragraph breaks. That is the shape every photographed letter
/// arrives in (#399), and it was the shape this function read worst.
///
/// So the line rhythm decides within each block, the way the vertical
/// gap decides within a PDF page — see [`stopped_short`].
///
/// One page: a text file has no pages, and inventing page 1 is the
/// honest answer — it is where a person would look.
pub fn segments_from_text(text: &str) -> Vec<Segment> {
    let measure = measure(text);
    text.split("\n\n")
        .flat_map(|block| paragraphs_from_lines(block, measure))
        .filter(|passage| !passage.text.is_empty())
        .enumerate()
        .map(|(ordinal, passage)| Segment {
            document: 0,
            page: 1,
            ordinal,
            text: passage.text,
            rows: passage.rows,
        })
        .collect()
}

/// The document's own measure: its longest line, in characters.
///
/// Measured over the whole document rather than the block being split,
/// for the reason the PDF path measures a page rather than a paragraph
/// — a block of one line carries no rhythm to read, and the document it
/// sits in does.
///
/// The longest line rather than an average, because the two ways of
/// being wrong are not equally bad. Too *narrow* a measure makes lines
/// look wrapped when they stopped, and merges paragraphs — back towards
/// the one-passage failure above. Too *wide* makes lines look stopped
/// when they wrapped, and splits a paragraph into its lines, which the
/// pack's per-passage questions can still be answered from. So a
/// runaway line — an OCR run-on, a pasted URL — degrades this to one
/// segment per line, which is the fallback #401 asked for and not a
/// case needing its own branch.
fn measure(text: &str) -> usize {
    text.lines()
        .map(|line| line.trim().chars().count())
        .max()
        .unwrap_or_default()
}

/// One block's paragraphs, split where a line stopped short on purpose.
fn paragraphs_from_lines(block: &str, measure: usize) -> Vec<Passage> {
    // Columns first, because a table's lines all "stop short" and the
    // wrap rule would make each one its own paragraph — in the wrong
    // order, with the columns still interleaved (#406).
    if let Some(columned) = columns_from_text(block) {
        return columned;
    }

    let lines: Vec<&str> = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let mut paragraphs = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        current.push((*line).to_owned());
        let ends_here = match lines.get(index + 1) {
            Some(next) => stopped_short(line, next, measure),
            None => true,
        };
        if ends_here {
            paragraphs.push(Passage {
                text: join_lines(std::mem::take(&mut current)),
                rows: Vec::new(),
            });
        }
    }
    paragraphs
}

/// A monospaced block read as columns, or `None` if it is prose (#406).
///
/// A `.txt` file states no coordinates, but a page set in a monospaced
/// font states its geometry in character positions — so this is the
/// same rule as the PDF path's, in the same code, with columns for
/// points. It matters beyond tidiness: the letter bed is `.txt`
/// deliberately, so that measuring it needs no pdfium in CI, and
/// without this the bed cannot see the fix at all.
///
/// Each word becomes a fragment at its own character offset, and the
/// channel test then asks what it always asks — is there a corridor no
/// line crosses. Two spaces after a full stop cannot open one, because
/// a channel must still clear [`COLUMN_CHANNEL`] of the block's width.
fn columns_from_text(block: &str) -> Option<Vec<Passage>> {
    let fragments = fragments_from_text(block);
    if fragments.is_empty() {
        return None;
    }

    let blocks = blocks(&fragments);
    if blocks.len() > 1 {
        return Some(blocks.iter().flat_map(|block| passages_of(block)).collect());
    }

    // One block, but it may still be a table filling the whole width.
    let rows = rows_of_cells(&fragments);
    is_tabular(&rows).then(|| vec![Passage::from_rows(rows)])
}

/// A block's passages once it is known to be columned: one per block,
/// keeping cells where the block is a table.
fn passages_of(fragments: &[Fragment]) -> Vec<Passage> {
    let rows = rows_of_cells(fragments);
    if rows.is_empty() {
        return Vec::new();
    }
    vec![Passage::from_rows(rows)]
}

/// One fragment per word, placed by character offset.
///
/// `y` counts down a line at a time, scaled clear of [`ROW_TOLERANCE`]
/// so that adjacent lines are adjacent rows rather than one row.
fn fragments_from_text(block: &str) -> Vec<Fragment> {
    let mut fragments = Vec::new();
    for (index, line) in block.lines().enumerate() {
        let y = -(index as f32) * (ROW_TOLERANCE + 1.0);
        let mut column = 0usize;
        for part in line.split(' ') {
            let width = part.chars().count();
            if !part.trim().is_empty() {
                fragments.push(Fragment {
                    text: part.to_owned(),
                    x: column as f32,
                    right: (column + width) as f32,
                    y,
                });
            }
            // The space that separated them counts too.
            column += width + 1;
        }
    }
    fragments
}

/// Did this line end because the writer meant it to, or because the
/// page ran out of width?
///
/// The next line's first word answers it. If that word would have fitted
/// on this line and was not put there, the line ended deliberately and
/// what follows is a new paragraph. If it would not have fitted, the
/// line wrapped and the two lines are one thought — "within 14 days of
/// the date of this" and "letter." mean nothing apart.
///
/// This is the same shape of rule as [`paragraphs`]' gap test and it is
/// there for the same reason: the document states its own measure, and
/// a threshold in characters would segment a letter and an agreement
/// differently for no reason but their type size.
fn stopped_short(line: &str, next: &str, measure: usize) -> bool {
    let Some(word) = next.split_whitespace().next() else {
        return true;
    };
    // +1 for the space that would have joined them.
    line.chars().count() + 1 + word.chars().count() <= measure
}

/// One page's paragraphs, in reading order.
///
/// The page is first cut into blocks down any whitespace channel that
/// runs its whole height (#406), then each block is read on its own.
/// Without that, a page laid out in columns is assembled row-wise and
/// one column's values land between another's — see [`column_cut`].
fn paragraphs(page: &Page) -> Vec<Passage> {
    blocks(&page.fragments)
        .iter()
        .flat_map(|block| paragraphs_of(block))
        .collect()
}

/// One passage, with the rows it was set in where it had any.
///
/// `text` is what it has always been, byte for byte, and everything
/// downstream reads it: a passage's cells are additional, never
/// instead.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Passage {
    text: String,
    rows: Vec<Vec<String>>,
}

impl Passage {
    /// A passage from rows of cells. It keeps its cells only if it is
    /// laid out like a table — see [`is_tabular`].
    fn from_rows(rows: Vec<Vec<String>>) -> Self {
        let text = join_lines(rows.iter().map(|cells| cells.join(" ")).collect());
        let rows = if is_tabular(&rows) { rows } else { Vec::new() };
        Passage { text, rows }
    }
}

/// Is this passage laid out as a table, or is it prose that happens to
/// contain a gap?
///
/// Two rows of two cells is the shallowest thing worth calling a table,
/// and the threshold matters in one direction only: a paragraph wrongly
/// carrying cells would be *rendered* as a table, and prose in a table
/// is worse than the flattening this fixes. A table wrongly read as
/// prose renders exactly as it does today.
fn is_tabular(rows: &[Vec<String>]) -> bool {
    rows.iter().filter(|cells| cells.len() > 1).count() >= COLUMN_ROWS
}

/// One block's paragraphs, in reading order.
///
/// The gap rule is measured against the block's own line spacing rather
/// than a constant: letters, statements and agreements are set at
/// different sizes, and a fixed threshold in points would segment one
/// of them wrongly. The *median* gap is the usual line spacing on a
/// page of prose, and it is unmoved by the handful of larger gaps that
/// are exactly what is being detected.
fn paragraphs_of(fragments: &[Fragment]) -> Vec<Passage> {
    let lines = rows_of_cells(fragments);
    if lines.is_empty() {
        return Vec::new();
    }

    let gaps = line_gaps(fragments);
    let Some(usual) = median(&gaps) else {
        // One line, so no gap to measure and nothing to split.
        return vec![Passage::from_rows(lines)];
    };

    let mut paragraphs = Vec::new();
    let mut current: Vec<Vec<String>> = Vec::new();
    for (index, text) in lines.into_iter().enumerate() {
        // `gaps[i]` is the distance from line i to line i+1, so the gap
        // that decides whether *this* line starts a paragraph is the
        // one before it.
        let starts_paragraph = index > 0
            && gaps
                .get(index - 1)
                .is_some_and(|gap| *gap > usual * PARAGRAPH_GAP);
        if starts_paragraph && !current.is_empty() {
            paragraphs.push(Passage::from_rows(std::mem::take(&mut current)));
        }
        current.push(text);
    }
    if !current.is_empty() {
        paragraphs.push(Passage::from_rows(current));
    }

    paragraphs.retain(|paragraph| !paragraph.text.is_empty());
    paragraphs
}

/// One block's visual rows, each split into cells at the block's own
/// column channels, top of the block first.
///
/// For prose this is one cell per row and the joined text is what
/// [`lines_from_fragments`] has always produced — byte for byte, which
/// is what keeps 794 existing fixtures reading exactly as they did.
fn rows_of_cells(fragments: &[Fragment]) -> Vec<Vec<String>> {
    let placed: Vec<&Fragment> = fragments
        .iter()
        .filter(|f| !f.text.trim().is_empty())
        .collect();
    let channels: Vec<f32> = channels(&placed)
        .into_iter()
        .map(|channel| channel.at)
        .collect();

    let mut rows: Vec<(f32, Vec<&Fragment>)> = Vec::new();
    for fragment in placed {
        match rows
            .iter_mut()
            .find(|(y, _)| (fragment.y - y).abs() <= ROW_TOLERANCE)
        {
            Some((_, row)) => row.push(fragment),
            None => rows.push((fragment.y, vec![fragment])),
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));

    rows.into_iter()
        .map(|(_, mut row)| {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            // A fragment sits in the band its left edge falls in, so
            // every row of the block has the same number of cells and a
            // row missing a value leaves that cell empty rather than
            // shifting the rest one column left.
            let mut cells: Vec<Vec<&str>> = vec![Vec::new(); channels.len() + 1];
            for fragment in row {
                let band = channels
                    .iter()
                    .filter(|channel| fragment.x > **channel)
                    .count();
                cells[band].push(fragment.text.trim());
            }
            cells
                .into_iter()
                .map(|cell| cell.join(" "))
                .collect::<Vec<String>>()
        })
        .filter(|cells| cells.iter().any(|cell| !cell.is_empty()))
        .collect()
}

/// A vertical corridor of whitespace running the full height of a
/// block, wide enough to separate columns.
#[derive(Debug, Clone, Copy)]
struct Channel {
    /// How wide the corridor is.
    width: f32,
    /// The x it cuts at — the rightmost content edge to its left.
    at: f32,
}

/// Every whitespace channel wide enough to be a column boundary, left
/// to right.
///
/// Fragments are swept left to right as intervals: a gap between the
/// rightmost edge seen so far and the next fragment's left edge is
/// clear of every fragment in the block by construction, whichever rows
/// they sit on. That is what makes this safe on prose, where the long
/// lines span every candidate and no channel survives.
fn channels(placed: &[&Fragment]) -> Vec<Channel> {
    let mut spans: Vec<(f32, f32)> = placed.iter().map(|f| (f.x, f.right)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));

    let Some((left_edge, first_right)) = spans.first().copied() else {
        return Vec::new();
    };
    let width = placed.iter().map(|f| f.right).fold(f32::MIN, f32::max) - left_edge;
    if width <= 0.0 {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut reach = first_right;
    for (x, right) in spans.iter().skip(1) {
        if *x > reach && *x - reach > width * COLUMN_CHANNEL {
            found.push(Channel {
                width: *x - reach,
                at: reach,
            });
        }
        reach = reach.max(*right);
    }
    found
}

/// A whitespace channel must be this fraction of the block's own width
/// before it separates two blocks rather than two words.
///
/// The same 0.05 `ocr::COLUMN_GAP` uses, for the same reason and
/// measured the same way — against the content's own span, not a paper
/// size the runner never sees. A twentieth of the width sits well clear
/// of ordinary word spacing while catching the channel a table leaves
/// between its columns.
const COLUMN_CHANNEL: f32 = 0.05;

/// A block must be this many rows deep on both sides of a channel
/// before the channel is a column boundary.
///
/// One row is a right-aligned date or page number beside prose, and
/// moving it to the end of the page would reorder a document that was
/// never a table. Two rows is the shallowest thing that can be read as
/// a column.
const COLUMN_ROWS: usize = 2;

/// Cut a page into blocks down its widest full-height whitespace
/// channel (#406).
///
/// Deliberately **one cut, never recursive**. The invoice that prompted
/// this sets its due date on the left and `Sub total / VAT / Total` on
/// the right, and the right-hand block contains a second channel of its
/// own — between those labels and their amounts. Cutting again would
/// separate every label from its value and produce `Sub total VAT
/// Total` beside `£300 £60 £360`, which is the original defect wearing
/// different clothes. One cut separates the blocks; row-wise assembly
/// within a block is correct and stays.
///
/// Conservative in the direction that matters. A channel qualifies only
/// if **no fragment crosses it anywhere on the page** and both sides are
/// at least [`COLUMN_ROWS`] deep, so a page of prose — where the long
/// lines span every candidate channel — is never cut, and neither is a
/// lone right-aligned figure. Failing to cut leaves today's behaviour;
/// cutting wrongly reorders a document a person is relying on.
fn blocks(fragments: &[Fragment]) -> Vec<Vec<Fragment>> {
    let placed: Vec<&Fragment> = fragments
        .iter()
        .filter(|f| !f.text.trim().is_empty())
        .collect();
    let whole = |fragments: &[&Fragment]| vec![fragments.iter().map(|f| (*f).clone()).collect()];

    // Widest first, but **width alone does not choose the cut**. A
    // table's rows all reach both sides of a channel; two blocks set
    // side by side do not. So the boundary is the widest channel whose
    // sides do not pair row for row, and a channel that merely happens
    // to be wide — the one inside a label/value table, between the
    // labels and their amounts — is passed over rather than taken and
    // then regretted.
    //
    // Choosing on width alone was wrong on a real fixture within hours
    // of shipping. An invoice whose due date read `14 September 2026`
    // left a five-column channel beside it, exactly tying the channel
    // inside its totals table; the tie resolved to the wrong one, the
    // sides paired, and the page was read row-wise after all —
    // `Due date Sub total £325.00 14 September 2026 VAT £65.00 ...`,
    // the original defect, on the very shape written to catch it.
    let mut candidates = channels(&placed);
    candidates.sort_by(|a, b| b.width.total_cmp(&a.width));

    for channel in candidates {
        let (left, right): (Vec<&Fragment>, Vec<&Fragment>) =
            placed.iter().partition(|f| f.right <= channel.at);
        if rows_in(&left) < COLUMN_ROWS || rows_in(&right) < COLUMN_ROWS {
            continue;
        }
        // Both sides as deep as the whole means every row reaches
        // across: one table, not two blocks.
        if rows_in(&left) == rows_in(&placed) && rows_in(&right) == rows_in(&placed) {
            continue;
        }
        return vec![
            left.iter().map(|f| (*f).clone()).collect(),
            right.iter().map(|f| (*f).clone()).collect(),
        ];
    }

    whole(&placed)
}

/// How many distinct visual rows these fragments occupy.
fn rows_in(placed: &[&Fragment]) -> usize {
    let mut baselines: Vec<f32> = Vec::new();
    for fragment in placed {
        if !baselines
            .iter()
            .any(|y| (fragment.y - y).abs() <= ROW_TOLERANCE)
        {
            baselines.push(fragment.y);
        }
    }
    baselines.len()
}

/// The vertical distance between each pair of adjacent lines, top of
/// the page first. Rows are found the way `pdf` finds them, so a
/// paragraph break is judged on the same geometry a table row is.
fn line_gaps(fragments: &[Fragment]) -> Vec<f32> {
    let mut baselines: Vec<f32> = Vec::new();
    for fragment in fragments.iter().filter(|f| !f.text.trim().is_empty()) {
        if !baselines
            .iter()
            .any(|y| (fragment.y - y).abs() <= ROW_TOLERANCE)
        {
            baselines.push(fragment.y);
        }
    }
    baselines.sort_by(|a, b| b.total_cmp(a));
    baselines.windows(2).map(|pair| pair[0] - pair[1]).collect()
}

/// Fragments within this vertical distance sit on one visual row —
/// the same tolerance `pdf` reconstructs statement rows with, for the
/// same reason: baseline jitter is not a new line.
const ROW_TOLERANCE: f32 = 3.0;

fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    Some(sorted[sorted.len() / 2])
}

/// Rejoin a paragraph's lines into running prose.
///
/// A sentence broken across two lines is one sentence: "within 14 days
/// of the date of this" and "letter." mean nothing apart, and a model
/// asked about either half would answer about half a deadline. A
/// hyphen at a line end is a word split by the typesetter, so it is
/// closed up rather than kept.
fn join_lines(lines: Vec<String>) -> String {
    let mut joined = String::new();
    for line in lines.iter().map(|line| line.trim()) {
        if line.is_empty() {
            continue;
        }
        match joined.strip_suffix('-') {
            Some(hyphenated) => joined = format!("{hyphenated}{line}"),
            None => {
                if !joined.is_empty() {
                    joined.push(' ');
                }
                joined.push_str(line);
            }
        }
    }
    joined
}
