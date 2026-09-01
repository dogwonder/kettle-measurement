//! Text recovered from a photograph (#399).
//!
//! The letter pack's real corpus is paper. The letters that carry
//! obligations — council, housing, NHS, HMRC, courts, schools, insurers
//! on renewal — are the ones organisations still send on paper,
//! precisely because they matter, and paper reaches a computer as a
//! photograph. A pack that reads letters but not posted ones has not
//! met its users' documents.
//!
//! Split the way [`crate::pdf`] is split: the judgement below is pure
//! and always compiled, so it runs in CI and on every machine, while
//! the call that produces a [`Reading`] is macOS-only and thin. What is
//! worth reviewing is the policy, not the framework binding.

use std::path::Path;

#[cfg(all(target_os = "macos", feature = "vision"))]
mod vision;

/// Read a picture of a document into text, or refuse it.
///
/// The reader is behind a feature and a target because Vision is
/// macOS-only, and the failure when it is absent is honest rather than
/// silent: a build that cannot read pictures says so, and does not tell
/// a person their photograph was poor.
pub fn read_image(path: &Path) -> Result<String, OcrError> {
    read_page(path)?.text()
}

/// One photograph, as the reader saw it, before any judgement.
pub fn read_page(path: &Path) -> Result<Reading, OcrError> {
    read_page_with(path, Correction::Applied)
}

/// Whether the reader may use the system's dictionaries to correct what
/// it thinks it saw.
///
/// Two settings, so a page can be read twice and the readings compared.
/// Confidence cannot find a wrong character — the same page photographed
/// twice read a reference number differently, one digit apart, both at
/// 1.000 (#399). Disagreement can, and this is what produces two
/// readings to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Correction {
    /// The reader's best guess at what was meant.
    Applied,
    /// Only what is on the page. Where the two differ, the reader is
    /// guessing, and a guess about a date is worth a person's eye.
    Literal,
}

/// One photograph, read a particular way.
pub fn read_page_with(path: &Path, correction: Correction) -> Result<Reading, OcrError> {
    #[cfg(all(target_os = "macos", feature = "vision"))]
    {
        vision::recognise(path, correction)
    }
    #[cfg(not(all(target_os = "macos", feature = "vision")))]
    {
        let _ = (path, correction);
        Err(OcrError::Unavailable(
            "this copy of Kettle was built without the picture reader".to_owned(),
        ))
    }
}

/// One line as the camera saw it, with how sure the reader was.
///
/// A line, not a paragraph. A photograph has no paragraphs in it — a
/// blank line is a typing convention, and OCR emits line breaks only.
/// [`crate::document::segments_from_text`] reads the line rhythm (#401),
/// so leaving the wrap where the page put it is what lets it work.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    /// 0.0 to 1.0, as Vision reports per observation.
    pub confidence: f32,
    /// Where the line sat on the page: normalised, 0.0 at the foot.
    ///
    /// Kept because two readings of one page cannot be compared by
    /// position in a list — a second pass may split or join lines
    /// differently, and comparing index against index would report the
    /// whole page as disputed the first time one line moved. Position
    /// is the only thing both readings agree about by construction.
    pub top: f64,
    /// Where the line started, for the same reason — and because the
    /// top alone is ambiguous exactly where letters put two columns:
    /// an address block sets the recipient left and the sender right,
    /// so two lines share a row and only `left` tells them apart.
    pub left: f64,
    /// Where the line ended. Carried so the segmenter can see the
    /// page's geometry — a line's *width*, not its character count, is
    /// what says whether it wrapped or stopped, and a column channel
    /// is a gap between one line's right and the next's left.
    pub right: f64,
    /// Where the line's box ended, 0.0 at the foot. With `top` this
    /// gives the line's centre, which is what the segmenter measures
    /// gaps from: a box's top moves with whether the line has capitals
    /// and its bottom with whether it has descenders, by a few points
    /// each way, and on a photograph a few points is a third of the
    /// leading. The centre moves half as much.
    pub bottom: f64,
}

/// Everything one photograph gave up.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub lines: Vec<Line>,
}

/// One piece of text and where it sat on the page.
///
/// Normalised to the page, origin at the bottom left as Vision reports
/// it, so a larger `top` is higher up.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Top edge of the text, 0.0 at the foot of the page.
    pub top: f64,
    /// Left edge, 0.0 at the left margin.
    pub left: f64,
    /// Right edge. Carried because the distance from one piece of text
    /// to the next is the only thing that tells a word space from a
    /// column gap, and `left` alone cannot say where the previous one
    /// ended.
    pub right: f64,
    pub line: Line,
}

/// Two pieces of text whose tops are within this fraction of the page
/// height sit on one line. A letter runs 40–60 lines to a page, so a
/// line is roughly 0.02 of it; half that groups a line without
/// swallowing its neighbour.
const SAME_LINE: f64 = 0.01;

/// A horizontal gap wider than this fraction of the page is a column
/// boundary, not a word space.
///
/// Measured against a real letter, which set the recipient's address on
/// the left and the sender's on the right — roughly half a page apart —
/// while ordinary word spaces and the gaps the reader leaves between
/// fragments of one sentence run an order of magnitude smaller. A
/// twentieth of the page sits well clear of both.
///
/// Deliberately generous towards joining. Splitting a sentence that
/// should have joined recreates the fragmenting this merge exists to
/// fix; failing to split two columns leaves two intact lines in an
/// awkward order, which a person can still read.
const COLUMN_GAP: f64 = 0.05;

/// A gap narrower than `COLUMN_GAP` is still a column boundary when
/// the text after it **lines up** with text on a neighbouring line.
///
/// Found on the photo bed's invoices (30 August 2026): the totals table
/// sets its columns a fiftieth of the page apart, under `COLUMN_GAP`,
/// so `18 October 2026` and `VAT` merged into one line and the due
/// date could never get its own row back. Width alone cannot tell a
/// tight column from a word space; alignment can. A word space in
/// prose does not start where the words on the rows beside it start,
/// except by coincidence — which is why the gap must also be wider
/// than a word space, [`WORD_GAP`], and the aligned line must be
/// within a few lines, [`NEARBY_LINES`].
const ALIGNED: f64 = 0.01;

/// Narrower than this is a word space whatever lines up with it.
/// Ordinary word spaces run about 0.006–0.01 of the page.
const WORD_GAP: f64 = 0.015;

/// How far up or down the page an aligned line may sit — three lines,
/// which is a table's reach and not a coincidence's.
const NEARBY_LINES: f64 = 0.06;

/// Does some text on a nearby, different line start where this one
/// does?
fn lines_up(item: &Placed, placed: &[(f64, f64)]) -> bool {
    placed.iter().any(|(top, left)| {
        (top - item.top).abs() > SAME_LINE
            && (top - item.top).abs() <= NEARBY_LINES
            && (left - item.left).abs() <= ALIGNED
    })
}

impl Reading {
    /// Assemble a page from placed observations: reading order first,
    /// then one line per line.
    ///
    /// Both halves were found the hard way on the first real
    /// photographed letter (#399).
    ///
    /// **Order** cannot be taken from the reader's array. It is not
    /// documented as reading order and is demonstrably not one — the
    /// same page came back with its sign-off among its address lines.
    /// A letter read out of order attaches a deadline to the wrong ask.
    ///
    /// **A line is not an observation.** The reader splits one where
    /// the typography changes, so a company name set in bold mid
    /// sentence becomes its own observation, and taken literally the
    /// sentence around it arrives as fragments. The model would then be
    /// asked its closed question about "We (Anytown" — #401's
    /// badly-formed question arriving by a different door. It has to be
    /// closed here: by the time fragments reach the segmenter, nothing
    /// distinguishes one from a genuinely short line.
    pub fn from_placed(mut placed: Vec<Placed>) -> Self {
        placed.sort_by(|a, b| {
            if (a.top - b.top).abs() <= SAME_LINE {
                a.left.total_cmp(&b.left)
            } else {
                b.top.total_cmp(&a.top)
            }
        });

        let edges: Vec<(f64, f64)> = placed.iter().map(|p| (p.top, p.left)).collect();
        let mut lines: Vec<(f64, f64, Line)> = Vec::new();
        for item in placed {
            let column_edge = |right: f64| {
                let gap = item.left - right;
                gap > COLUMN_GAP || (gap > WORD_GAP && lines_up(&item, &edges))
            };
            match lines.last_mut() {
                // Same line, and near enough to be the same sentence:
                // join with the space the page put there.
                Some((top, right, line))
                    if (*top - item.top).abs() <= SAME_LINE && !column_edge(*right) =>
                {
                    line.text.push(' ');
                    line.text.push_str(item.line.text.trim());
                    // The least certain part decides. A confident
                    // fragment beside a doubtful one must not launder
                    // the doubt — the floor exists to catch exactly the
                    // doubtful part, and an average would hide it.
                    line.confidence = line.confidence.min(item.line.confidence);
                    line.right = item.right;
                    *right = item.right;
                }
                _ => {
                    let mut line = item.line;
                    line.right = item.right;
                    lines.push((item.top, item.right, line));
                }
            }
        }

        Reading {
            lines: lines
                .into_iter()
                .map(|(_, _, mut line)| {
                    line.text = read_pound_sign(&line.text);
                    line
                })
                .collect(),
        }
    }
}

/// Put back a `£` the reader saw as `E`.
///
/// Vision reads a printed pound sign as a capital E about a third of
/// the time on the synthetic photo bed, sometimes both ways in one
/// line, and no language setting changes it. An `E` at the start of a
/// word, directly before digits with pence (`E87.44`, `E1,967.41`), is
/// a pound sign: no British letter writes an amount that way. A
/// postcode (`E3 8ZA`), a reference (`E12`) or a grade (`E2.1`) has no
/// pence and is left exactly as read. Applied to both readings, because
/// it is a glyph the reader cannot draw, not a word it might correct —
/// so it never manufactures a disagreement (#412).
fn read_pound_sign(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let word_start = i == 0 || matches!(chars[i - 1], ' ' | '(' | '[' | '\u{a0}');
        if c == 'E' && word_start && amount_with_pence_follows(&chars[i + 1..]) {
            out.push('£');
        } else {
            out.push(c);
        }
    }
    out
}

/// `\d[\d,]*\.\d\d` and then not another digit.
fn amount_with_pence_follows(rest: &[char]) -> bool {
    let mut i = 0;
    while i < rest.len() && (rest[i].is_ascii_digit() || (i > 0 && rest[i] == ',')) {
        i += 1;
    }
    if i == 0 || rest.get(i) != Some(&'.') {
        return false;
    }
    let pence = &rest[i + 1..];
    pence.len() >= 2
        && pence[0].is_ascii_digit()
        && pence[1].is_ascii_digit()
        && !pence.get(2).is_some_and(|c| c.is_ascii_digit())
}

/// One line two readings of the same page did not agree about.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Disagreement {
    /// 1-based page within the logical document. A photographed page
    /// starts at one and ordered page groups offset it when combined.
    #[serde(default = "first_page")]
    pub page: usize,
    /// Where on the page, so the dispute can be attached to the passage
    /// it lands in rather than to a line number nobody can see.
    pub top: f64,
    /// What the first reading made of it.
    pub read: String,
    /// What the second made of it — empty if it saw nothing there.
    pub also_read: String,
}

fn first_page() -> usize {
    1
}

impl Disagreement {
    /// Does this dispute land in `passage` (#412 step 6)?
    ///
    /// Segments are built from the same reading `read` came from, so a
    /// disputed line is a substring of the passage that swallowed it —
    /// paragraphs join lines, and joining is the only transformation
    /// between the two.
    ///
    /// Compared the way [`same_reading`] compares, and for the same
    /// reason: the join inserts a space where the line ended, so an
    /// exact search would miss every dispute that is not a whole
    /// paragraph. Case follows for free and costs nothing — a dispute
    /// this matches leniently is shown to a person, never hidden from
    /// one.
    pub fn lands_in(&self, passage: &str) -> bool {
        letters(passage).contains(&letters(&self.read))
    }
}

/// The page a normalised reading is laid onto, in points: A4. A
/// photograph states no paper size, and the segmenter's tolerances —
/// a 3pt row, a gap measured against the page's own leading — were
/// written for a page in points. A4 is the paper every letter in the
/// bed is printed on, and the only thing the choice changes is the
/// unit `ROW_TOLERANCE` is read in.
const PAGE_WIDTH_PT: f64 = 595.0;
const PAGE_HEIGHT_PT: f64 = 842.0;

impl Reading {
    /// The reading as a positioned page, so a photograph is segmented
    /// by the same geometry a PDF page is (#399, #256).
    ///
    /// `text()` flattened the lines to a string and left the segmenter
    /// to guess, from character counts, which lines had wrapped; on the
    /// synthetic photo bed that guess split 26 of 29 asks at a line
    /// break. The camera saw where every line sat. This keeps it.
    pub fn page(&self) -> crate::pdf::Page {
        crate::pdf::Page {
            fragments: self
                .lines
                .iter()
                .filter(|line| !line.text.trim().is_empty())
                .map(|line| crate::pdf::Fragment {
                    text: line.text.trim().to_owned(),
                    x: (line.left * PAGE_WIDTH_PT) as f32,
                    right: (line.right * PAGE_WIDTH_PT) as f32,
                    y: ((line.top + line.bottom) / 2.0 * PAGE_HEIGHT_PT) as f32,
                })
                .collect(),
        }
    }

    /// Where this reading and another disagree about the same page.
    ///
    /// The reason this exists rather than a higher confidence
    /// threshold: confidence cannot find a wrong character. The same
    /// page photographed twice read a reference number differently, one
    /// digit apart, **both at 1.000** (#399) — so no threshold over
    /// those numbers can separate a right reading from a wrong one.
    /// Disagreement can, and it points at the characters neither pass
    /// was sure of.
    ///
    /// Matched by position, not by index. A second pass may split or
    /// join lines differently, and comparing list positions would call
    /// the whole page disputed the first time one line moved.
    ///
    /// A line only one pass saw is a disagreement too: nothing
    /// confirmed it, and a deadline sitting in it must not pass
    /// unremarked.
    pub fn disagreements(&self, other: &Reading) -> Vec<Disagreement> {
        self.lines
            .iter()
            .filter_map(|line| {
                // The nearest line on the same row, not the first one.
                // A letter sets the recipient's address left and the
                // sender's right, so two lines share a row, and the two
                // readings need not list them in the same order —
                // matching on the row alone compared one column against
                // the other and called two different sentences a
                // disagreement.
                let against = other
                    .lines
                    .iter()
                    .filter(|candidate| (candidate.top - line.top).abs() <= SAME_LINE)
                    .min_by(|one, two| {
                        (one.left - line.left)
                            .abs()
                            .total_cmp(&(two.left - line.left).abs())
                    })
                    .filter(|candidate| (candidate.left - line.left).abs() <= COLUMN_GAP);
                match against {
                    Some(found) if same_reading(&found.text, &line.text) => None,
                    Some(found) => Some(Disagreement {
                        page: 1,
                        top: line.top,
                        read: line.text.clone(),
                        also_read: found.text.clone(),
                    }),
                    None => Some(Disagreement {
                        page: 1,
                        top: line.top,
                        read: line.text.clone(),
                        also_read: String::new(),
                    }),
                }
            })
            .collect()
    }
}

/// Do two readings say the same thing?
///
/// Spacing and case are not disagreements, and **a missing space is
/// not one either**. Measured on a real letter: the literal pass drops
/// word spaces wholesale — `28thApril`, `herebyauthorise`,
/// `BuildingSafety` — so comparing word by word made five lines in
/// thirty disputed on spacing alone, and buried the one line where the
/// readings genuinely differed. A gate that fires on a fifth of every
/// page is the gate nobody reads, which is the failure this design
/// exists to avoid.
///
/// Ignoring spacing costs nothing here. A deadline whose only defect is
/// a missing space still reads as the same date to a person, and to the
/// model that is asked about it.
fn same_reading(one: &str, other: &str) -> bool {
    letters(one) == letters(other)
}

/// What is left of a line once spacing and case stop counting.
fn letters(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Below this, a line is not worth reading. Vision reports clean
/// printed text well above 0.9; the values that fall this far are
/// garble, not slightly-worse text.
const LINE_FLOOR: f32 = 0.5;

/// The share of a page that may read badly before the page is refused.
///
/// Not zero, and that is the harder half. There is always a smudged
/// reference number or a fold across one line, and a refusal that fires
/// on any imperfection refuses every real photograph — after three
/// tries a person stops using Kettle rather than concluding their
/// lighting is at fault.
///
/// Not generous either, for the reason [`OcrError::TooUncertain`]
/// gives. A quarter sits between the two, and the tests in
/// `tests/ocr.rs` pin both sides: one bad line in six is read, three in
/// eight is refused.
const PAGE_TOLERANCE: f32 = 0.25;

/// The shortest edge a photographed page needs before its characters
/// are large enough for a deadline to be trusted. The first field
/// measurements bracketed the boundary: 612×792 failed while
/// 1855×2400 read cleanly (#399). 1200 refuses the known-bad rendition
/// with room for ordinary phone photographs.
const MIN_SHORT_EDGE: usize = 1200;

/// Refuse a small rendition before OCR can turn a fuzzy digit into a
/// tidy but wrong deadline.
pub fn check_dimensions(width: usize, height: usize) -> Result<(), OcrError> {
    if width.min(height) < MIN_SHORT_EDGE {
        Err(OcrError::TooSmall { width, height })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// Nothing resolved at all — a photograph of a desk, or a page so
    /// dark no text came out of it.
    NoText,
    /// The picture is a preview or thumbnail rather than a full-size
    /// camera image.
    TooSmall { width: usize, height: usize },
    /// Enough of the page read badly that none of it can be trusted.
    TooUncertain {
        /// How many lines fell below the floor, and out of how many.
        /// Carried for the run log, never shown to a person: "9 of 23
        /// observations below 0.5" is a sentence about Kettle, not
        /// about their letter.
        poor: usize,
        total: usize,
    },
    /// The reader itself could not be used — not built in, or the
    /// platform has none.
    Unavailable(String),
    /// The file could not be opened or decoded as an image.
    Unreadable(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The same event as an image-only PDF, so the same words:
            // a person meets one Kettle (`PdfError::NoText`).
            OcrError::NoText => write!(
                f,
                "found no text in this picture — check the letter fills the frame \
                 and the light is even, then take it again"
            ),
            OcrError::TooSmall { .. } => write!(
                f,
                "this picture is too small to read safely — choose the original, full-size photo"
            ),
            // Says what they did and what to do about it. No score, no
            // threshold, no "OCR": the fix is in their hands and it
            // takes seconds.
            OcrError::TooUncertain { .. } => write!(
                f,
                "could not read this photo clearly enough to trust it. \
                 Flatten the letter, avoid shadow across the page, and take it again"
            ),
            OcrError::Unavailable(_) => {
                write!(f, "this copy of Kettle cannot read pictures")
            }
            // The reader's own words are kept for the run log and never
            // shown. A real photograph that had not finished copying
            // produced "CRImage Reader Detector was given
            // zero-dimensioned image (0 x 0)", which tells a person
            // nothing they can act on and is the sort of sentence the
            // plain-language rule exists to keep out (CLAUDE.md).
            OcrError::Unreadable(_) => write!(
                f,
                "could not open this picture — it may be damaged, or still copying"
            ),
        }
    }
}

impl std::error::Error for OcrError {}

impl Reading {
    /// The letter as text, or a refusal.
    ///
    /// Refusing outright is deliberate, and it is the whole of #399's
    /// second half. Kettle asserts deadlines from letters; digit/letter
    /// confusion is the characteristic OCR error — the first real
    /// letter turned `EC1M 5LA` into `ECIM SLA`, twice in five
    /// characters — and the same slip inside a date is a wrong deadline
    /// stated at full confidence, which nothing downstream can question.
    ///
    /// Surfacing a partly-trusted page as a tidy task list is the one
    /// outcome that must not happen. Refusing is affordable here in a
    /// way it would not be for a document that arrived by post and
    /// cannot be re-taken: the person is holding the letter and a
    /// camera.
    pub fn text(&self) -> Result<String, OcrError> {
        if self.lines.iter().all(|line| line.text.trim().is_empty()) {
            return Err(OcrError::NoText);
        }

        let poor = self
            .lines
            .iter()
            .filter(|line| line.confidence < LINE_FLOOR)
            .count();
        let total = self.lines.len();
        if poor as f32 > total as f32 * PAGE_TOLERANCE {
            return Err(OcrError::TooUncertain { poor, total });
        }

        Ok(self
            .lines
            .iter()
            .map(|line| line.text.trim())
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
