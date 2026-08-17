//! PDF statements, best effort (#13, #137): retain positioned text,
//! reconstruct conservative table rows, and keep reading-order lines
//! for diagnostics. A statement PDF has no table structure, only
//! text placed at coordinates, so direction is accepted only when a
//! value sits unambiguously under Paid Out or Paid In.
//!
//! Extraction itself (pdfium) is feature-gated; this reconstruction is
//! pure and always compiled, so it stays under ordinary `cargo test`.

use crate::parse::{Direction, ParsedStatement, Transaction};
use chrono::NaiveDate;
use rust_decimal::Decimal;

#[cfg(feature = "pdf")]
pub use extract::{extract_lines, extract_pages, library_filename, library_loads, library_present};

/// A piece of text at a position, in PDF user space: x grows rightward,
/// y grows *up* the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub text: String,
    /// Left edge of the fragment.
    pub x: f32,
    /// Right edge of the fragment. Statement money columns are normally
    /// right-aligned, so this — not `x` — carries their column identity.
    pub right: f32,
    pub y: f32,
}

/// Positioned fragments from one page. Page boundaries cannot be
/// flattened away: headers and furniture repeat independently on every
/// page, and y coordinates restart.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub fragments: Vec<Fragment>,
}

/// Fragments within this vertical distance sit on the same visual row.
/// Statement body text runs ~8–10pt with rows a line-height apart, so
/// half a small line height absorbs baseline jitter without merging
/// adjacent rows. Pinned by the reconstruction tests.
const ROW_TOLERANCE: f32 = 3.0;

struct Row<'a> {
    y: f32,
    fragments: Vec<&'a Fragment>,
}

fn positioned_rows(fragments: &[Fragment]) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    for fragment in fragments.iter().filter(|f| !f.text.trim().is_empty()) {
        match rows
            .iter_mut()
            .find(|row| (fragment.y - row.y).abs() <= ROW_TOLERANCE)
        {
            Some(row) => row.fragments.push(fragment),
            None => rows.push(Row {
                y: fragment.y,
                fragments: vec![fragment],
            }),
        }
    }

    rows.sort_by(|a, b| b.y.total_cmp(&a.y));
    for row in &mut rows {
        row.fragments.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    rows
}

/// Rebuild reading order: group fragments into rows by y (greedy,
/// order-stable — a fragment joins the first row within tolerance),
/// sort rows top of page first, fragments left to right, one space
/// between fragments.
pub fn lines_from_fragments(fragments: &[Fragment]) -> Vec<String> {
    positioned_rows(fragments)
        .into_iter()
        .map(|row| {
            row.fragments
                .iter()
                .map(|f| f.text.trim())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[derive(Debug)]
pub enum PdfError {
    /// The vendored libpdfium isn't in sidecars/ — soft failure, CSV
    /// remains available.
    LibraryMissing,
    /// The PDF reader is installed but couldn't be started (#140).
    ReaderUnavailable(String),
    Unreadable(String),
    /// Pages held no text at all — almost certainly a scan (#71).
    NoText,
    /// Text exists, but no supported statement table can be established.
    UnrecognisedLayout,
    /// A single signed money column was found and read, but nothing in
    /// the document establishes whether a positive amount is money in
    /// or money out (#218).
    ///
    /// Separate from [`PdfError::UnrecognisedLayout`] because the
    /// columns *were* recognised. Sending someone to look for a layout
    /// problem that isn't there wastes the one thing they have, which
    /// is patience.
    UndeterminedDirection,
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfError::LibraryMissing => write!(
                f,
                "this copy of Kettle is missing its PDF reader — \
                 try another statement format if your bank offers one"
            ),
            PdfError::ReaderUnavailable(reason) => write!(
                f,
                "Kettle's PDF reader wouldn't start just now ({reason}) — \
                 try again, or try another statement format if your bank offers one"
            ),
            PdfError::Unreadable(reason) => write!(f, "could not read the PDF: {reason}"),
            PdfError::NoText => write!(
                f,
                "found no text in this PDF — it looks like a scan \
                 (pictures of pages). Try another statement format if your bank offers one"
            ),
            PdfError::UnrecognisedLayout => write!(
                f,
                "could not recognise the transaction columns in this PDF — \
                 try another statement format if your bank offers one"
            ),
            PdfError::UndeterminedDirection => write!(
                f,
                "this statement has one Amount column, and nothing in it \
                 settles whether a positive amount is money in or money \
                 out — reading it either way could invert your spending, \
                 so Kettle won't guess. A statement showing a running \
                 balance, or separate money-in and money-out columns, \
                 can be read; so can another statement format if your \
                 bank offers one"
            ),
        }
    }
}

impl std::error::Error for PdfError {}

#[derive(Clone, Copy)]
struct Headers {
    date_limit: f32,
    description_left: f32,
    description_right: f32,
    paid_out_right: f32,
    paid_in_right: f32,
    balance_right: f32,
}

fn normalise_words(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Find a header even when pdfium emits its words as separate
/// fragments. The returned extent belongs to the fragments that spell
/// the phrase, rather than to the space-joined diagnostic line.
fn phrase_extent(row: &Row<'_>, phrase: &str) -> Option<(f32, f32)> {
    let phrase = normalise_words(phrase);
    for start in 0..row.fragments.len() {
        let mut words = String::new();
        for end in start..row.fragments.len() {
            if !words.is_empty() {
                words.push(' ');
            }
            words.push_str(row.fragments[end].text.trim());
            let candidate = normalise_words(&words);
            if candidate == phrase {
                return Some((row.fragments[start].x, row.fragments[end].right));
            }
            if !phrase.starts_with(&candidate) {
                break;
            }
        }
    }
    None
}

fn headers(row: &Row<'_>) -> Option<Headers> {
    let (date_left, date_right) = phrase_extent(row, "Date")?;
    let (description_left, description_right) = phrase_extent(row, "Description")?;
    let (paid_out_left, paid_out_right) = phrase_extent(row, "Paid Out")?;
    let (paid_in_left, paid_in_right) = phrase_extent(row, "Paid In")?;
    let (balance_left, balance_right) = phrase_extent(row, "Balance")?;

    // A matching sentence elsewhere on the page is not a table header.
    // The five cells have to occupy distinct columns in this order.
    (date_left < date_right
        && date_right < description_left
        && description_left < description_right
        && description_right < paid_out_left
        && paid_out_right < paid_in_left
        && paid_in_right < balance_left
        && balance_left < balance_right)
        .then_some(Headers {
            date_limit: (date_right + description_left) / 2.0,
            description_left,
            description_right,
            paid_out_right,
            paid_in_right,
            balance_right,
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MoneyColumn {
    PaidOut,
    PaidIn,
    Balance,
}

impl Headers {
    fn money_column(self, right: f32) -> Option<MoneyColumn> {
        let anchors = [
            (self.paid_out_right, MoneyColumn::PaidOut),
            (self.paid_in_right, MoneyColumn::PaidIn),
            (self.balance_right, MoneyColumn::Balance),
        ];
        let closest = anchors
            .into_iter()
            .min_by(|(a, _), (b, _)| (right - *a).abs().total_cmp(&(right - *b).abs()))?;
        let narrowest_gap = (self.paid_in_right - self.paid_out_right)
            .abs()
            .min((self.balance_right - self.paid_in_right).abs());

        // A merchant containing a standalone number must not become an
        // amount merely because it is the nearest of three distant
        // columns. Real right-aligned values sit close to their header's
        // right edge; one third of the narrowest gap tolerates ordinary
        // font/layout jitter without allowing columns to overlap.
        ((right - closest.0).abs() <= narrowest_gap / 3.0).then_some(closest.1)
    }

    fn merchant_limit(self) -> f32 {
        (self.description_right + self.paid_out_right) / 2.0
    }
}

/// The second supported table (#218): Date, Description, a single
/// signed Amount, and a running Balance.
///
/// The Balance column is not decoration here — it is the *only* reason
/// this layout can be supported at all. One signed money column cannot
/// say whether `-24.99` is money out or money in; both conventions
/// occur in real exports. A running balance can say, because the
/// arithmetic reconciles one way and not the other. Without it the
/// layout is refused, which is why `Balance` is required rather than
/// optional.
#[derive(Clone, Copy)]
struct AmountHeaders {
    date_limit: f32,
    description_left: f32,
    description_right: f32,
    amount_right: f32,
    balance_right: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AmountColumn {
    Amount,
    Balance,
}

impl AmountHeaders {
    /// Same right-edge matching and same one-third-of-the-gap tolerance
    /// as [`Headers::money_column`], for the same reason: a merchant
    /// containing a standalone number must not become an amount merely
    /// because it is the nearest of two distant columns.
    fn money_column(self, right: f32) -> Option<AmountColumn> {
        let gap = (self.balance_right - self.amount_right).abs();
        let anchors = [
            (self.amount_right, AmountColumn::Amount),
            (self.balance_right, AmountColumn::Balance),
        ];
        let closest = anchors
            .into_iter()
            .min_by(|(a, _), (b, _)| (right - *a).abs().total_cmp(&(right - *b).abs()))?;
        ((right - closest.0).abs() <= gap / 3.0).then_some(closest.1)
    }

    fn merchant_limit(self) -> f32 {
        (self.description_right + self.amount_right) / 2.0
    }
}

fn amount_headers(row: &Row<'_>) -> Option<AmountHeaders> {
    let (date_left, date_right) = phrase_extent(row, "Date")?;
    let (description_left, description_right) = phrase_extent(row, "Description")?;
    let (amount_left, amount_right) = phrase_extent(row, "Amount")?;
    let (balance_left, balance_right) = phrase_extent(row, "Balance")?;

    (date_left < date_right
        && date_right < description_left
        && description_left < description_right
        && description_right < amount_left
        && amount_right < balance_left
        && balance_left < balance_right)
        .then_some(AmountHeaders {
            date_limit: (date_right + description_left) / 2.0,
            description_left,
            description_right,
            amount_right,
            balance_right,
        })
}

/// Which supported table a page turned out to be.
enum Layout {
    /// Date, Description, Paid Out, Paid In, Balance — direction is in
    /// the column a value lands in, so it is known row by row.
    Split(Headers),
    /// Date, Description, Amount, Balance — direction is a property of
    /// the *document*, and cannot be settled until the balances have
    /// been read.
    Signed(AmountHeaders),
}

fn layout(row: &Row<'_>) -> Option<Layout> {
    headers(row)
        .map(Layout::Split)
        .or_else(|| amount_headers(row).map(Layout::Signed))
}

/// Date, Description and a single Amount, with no Balance beside it.
///
/// Not a supported layout — there is nothing here to establish what the
/// sign means. It is worth recognising anyway, purely so the refusal
/// can say the true thing. "Could not recognise the transaction
/// columns" would send someone hunting for a layout problem that does
/// not exist, when the actual answer is that this table cannot say
/// which way its money goes.
fn signed_amount_without_balance(row: &Row<'_>) -> bool {
    let Some((date_left, date_right)) = phrase_extent(row, "Date") else {
        return false;
    };
    let Some((description_left, description_right)) = phrase_extent(row, "Description") else {
        return false;
    };
    let Some((amount_left, amount_right)) = phrase_extent(row, "Amount") else {
        return false;
    };
    date_left < date_right
        && date_right < description_left
        && description_left < description_right
        && description_right < amount_left
        && amount_left < amount_right
}

/// What a positive number in a single Amount column means. Established
/// from the document, never assumed — see [`establish_convention`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SignConvention {
    /// `balance[n] == balance[n-1] + amount[n]`, so a negative amount is
    /// money out.
    AddsToBalance,
    /// `balance[n] == balance[n-1] - amount[n]`, so a positive amount is
    /// money out.
    SubtractsFromBalance,
}

impl SignConvention {
    fn direction_of(self, amount: Decimal) -> Direction {
        let negative = amount.is_sign_negative();
        match self {
            SignConvention::AddsToBalance if negative => Direction::Debit,
            SignConvention::AddsToBalance => Direction::Credit,
            SignConvention::SubtractsFromBalance if negative => Direction::Credit,
            SignConvention::SubtractsFromBalance => Direction::Debit,
        }
    }
}

/// A row read from a signed-Amount table, before the document has said
/// what its sign means.
struct SignedRow {
    date: NaiveDate,
    merchant: String,
    amount: Decimal,
    balance: Option<Decimal>,
}

/// Read the sign convention out of the running balance, or refuse.
///
/// Every consecutive pair of rows that both carry a balance is one
/// piece of arithmetic evidence: the change in balance either equals
/// the signed amount, or its negation, or neither. The rules are
/// deliberately strict, because the failure this guards against is
/// silent — a wrong convention inverts every figure in the report
/// while producing a document that looks entirely normal.
///
/// - **Any disagreement refuses the file.** Two pairs implying opposite
///   conventions means at least one is wrong, and nothing here says
///   which.
/// - **At least two agreeing pairs are required.** One identity holding
///   exactly is suggestive, but a column headed "Balance" that is not
///   in fact a running balance can satisfy a single row by coincidence.
///   Two is cheap insurance against reading a statement backwards.
/// - **A pair that reconciles neither way proves nothing** and is
///   tolerated with a warning: a real statement can carry a row Kettle
///   skipped. It cannot, on its own, establish anything either.
/// - **A zero amount reconciles both ways** and so is not evidence.
fn establish_convention(rows: &[SignedRow], warnings: &mut Vec<String>) -> Option<SignConvention> {
    let mut agreed: Option<SignConvention> = None;
    let mut supporting = 0usize;

    for pair in rows.windows(2) {
        let (Some(previous), Some(current)) = (pair[0].balance, pair[1].balance) else {
            continue;
        };
        let delta = current - previous;
        let adds = delta == pair[1].amount;
        let subtracts = delta == -pair[1].amount;

        let implied = match (adds, subtracts) {
            // A zero amount satisfies both. No information.
            (true, true) => continue,
            (true, false) => SignConvention::AddsToBalance,
            (false, true) => SignConvention::SubtractsFromBalance,
            (false, false) => {
                warnings.push(format!(
                    "the balance after {} does not match the amount beside it — that row \
                     was not used to work out which sign means money out",
                    pair[1].date
                ));
                continue;
            }
        };

        match agreed {
            Some(convention) if convention != implied => return None,
            Some(_) => supporting += 1,
            None => {
                agreed = Some(implied);
                supporting = 1;
            }
        }
    }

    agreed.filter(|_| supporting >= 2)
}

const PDF_DATES: &[&str] = &["%d %b %y", "%d %b %Y", "%d/%m/%Y"];

fn pdf_date(value: &str) -> Option<NaiveDate> {
    PDF_DATES
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(value, format).ok())
}

fn decimal(value: &str) -> Option<Decimal> {
    let cleaned = value
        .trim()
        .trim_start_matches('£')
        .replace(',', "")
        .trim()
        .to_owned();
    Decimal::from_str_exact(&cleaned).ok()
}

fn warning(page: usize, row: usize, reason: &str) -> String {
    format!("page {page}, row {row}: {reason} — row skipped")
}

/// Turn positioned text into transactions.
///
/// Two table shapes are supported, both of which say what a figure
/// *means* rather than leaving it to be inferred:
///
/// - **Date, Description, Paid Out, Paid In, Balance** — the proved
///   HSBC-style table. Direction is which column a value lands in, so
///   it is settled row by row.
/// - **Date, Description, Amount, Balance** (#218) — one signed money
///   column, where the running balance establishes what the sign means.
///   Direction is a property of the whole document here, so it cannot
///   be settled until every balance has been read.
///
/// Anything else fails closed, including a signed Amount column with no
/// balance to check it against. Guessing which sign means money out is
/// how you silently invert somebody's spending, and a report that is
/// confidently backwards is worse than no report.
pub fn reconstruct_statement(pages: &[Page]) -> Result<ParsedStatement, PdfError> {
    let mut transactions = Vec::new();
    let mut warnings = Vec::new();
    let mut signed_rows: Vec<SignedRow> = Vec::new();
    let mut saw_layout = false;
    let mut saw_amount_without_balance = false;

    for (page_index, page) in pages.iter().enumerate() {
        let rows = positioned_rows(&page.fragments);
        let Some((header_index, found)) = rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| layout(row).map(|found| (index, found)))
        else {
            saw_amount_without_balance |= rows.iter().any(signed_amount_without_balance);
            warnings.push(format!(
                "page {}: could not recognise the transaction columns — page skipped",
                page_index + 1
            ));
            continue;
        };
        saw_layout = true;

        let headers = match found {
            Layout::Split(headers) => headers,
            Layout::Signed(headers) => {
                read_signed_page(
                    &rows,
                    header_index,
                    headers,
                    page_index,
                    &mut signed_rows,
                    &mut warnings,
                );
                continue;
            }
        };

        for (row_index, row) in rows.iter().enumerate().skip(header_index + 1) {
            let date_text = row
                .fragments
                .iter()
                .filter(|fragment| fragment.x < headers.date_limit)
                .map(|fragment| fragment.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let Some(date) = pdf_date(&date_text) else {
                // Headers, page furniture and summary boxes are not
                // transactions merely because they contain numbers.
                continue;
            };

            let mut paid_out = Vec::new();
            let mut paid_in = Vec::new();
            let mut first_money_left: Option<f32> = None;
            for fragment in &row.fragments {
                let Some(column) = headers.money_column(fragment.right) else {
                    continue;
                };
                first_money_left = Some(
                    first_money_left
                        .map(|left| left.min(fragment.x))
                        .unwrap_or(fragment.x),
                );
                let Some(value) = decimal(&fragment.text) else {
                    continue;
                };
                match column {
                    MoneyColumn::PaidOut => paid_out.push(value),
                    MoneyColumn::PaidIn => paid_in.push(value),
                    MoneyColumn::Balance => {}
                }
            }

            let visual_row = row_index + 1;
            let amount = match (paid_out.as_slice(), paid_in.as_slice()) {
                ([amount], []) => (*amount, Direction::Debit),
                ([], [amount]) => (*amount, Direction::Credit),
                ([], []) => {
                    warnings.push(warning(
                        page_index + 1,
                        visual_row,
                        "could not read an amount in Paid Out or Paid In",
                    ));
                    continue;
                }
                _ => {
                    warnings.push(warning(
                        page_index + 1,
                        visual_row,
                        "Paid Out and Paid In were ambiguous",
                    ));
                    continue;
                }
            };
            let magnitude = if amount.0.is_sign_negative() {
                -amount.0
            } else {
                amount.0
            };
            if magnitude == Decimal::ZERO {
                warnings.push(warning(page_index + 1, visual_row, "the amount was zero"));
                continue;
            }

            let merchant_limit = first_money_left.unwrap_or_else(|| headers.merchant_limit());
            let merchant = row
                .fragments
                .iter()
                .filter(|fragment| {
                    fragment.x >= headers.description_left - ROW_TOLERANCE
                        && fragment.x < merchant_limit
                })
                .map(|fragment| fragment.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            if merchant.is_empty() {
                warnings.push(warning(
                    page_index + 1,
                    visual_row,
                    "the description was empty",
                ));
                continue;
            }

            transactions.push(Transaction {
                date,
                raw_merchant: merchant,
                amount: magnitude,
                direction: amount.1,
            });
        }
    }

    if !saw_layout {
        return Err(if saw_amount_without_balance {
            PdfError::UndeterminedDirection
        } else {
            PdfError::UnrecognisedLayout
        });
    }

    if !signed_rows.is_empty() {
        // The columns were read fine; what the sign means is the open
        // question, and only the document can close it.
        let Some(convention) = establish_convention(&signed_rows, &mut warnings) else {
            return Err(PdfError::UndeterminedDirection);
        };
        for row in signed_rows {
            let magnitude = row.amount.abs();
            if magnitude == Decimal::ZERO {
                warnings.push(format!(
                    "the amount beside {} was zero — row skipped",
                    row.date
                ));
                continue;
            }
            transactions.push(Transaction {
                date: row.date,
                raw_merchant: row.merchant,
                amount: magnitude,
                direction: convention.direction_of(row.amount),
            });
        }
    }

    Ok(ParsedStatement {
        transactions,
        warnings,
    })
}

/// Read a Date / Description / Amount / Balance page into rows whose
/// direction is still open. Nothing here decides what a sign means —
/// that is [`establish_convention`]'s job, once every page has been
/// read, because the evidence is the balance running across all of them.
fn read_signed_page(
    rows: &[Row<'_>],
    header_index: usize,
    headers: AmountHeaders,
    page_index: usize,
    into: &mut Vec<SignedRow>,
    warnings: &mut Vec<String>,
) {
    for (row_index, row) in rows.iter().enumerate().skip(header_index + 1) {
        let date_text = row
            .fragments
            .iter()
            .filter(|fragment| fragment.x < headers.date_limit)
            .map(|fragment| fragment.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let Some(date) = pdf_date(&date_text) else {
            // Page furniture and summary boxes are not transactions
            // merely because they contain numbers.
            continue;
        };

        let mut amounts = Vec::new();
        let mut balances = Vec::new();
        let mut first_money_left: Option<f32> = None;
        for fragment in &row.fragments {
            let Some(column) = headers.money_column(fragment.right) else {
                continue;
            };
            first_money_left = Some(
                first_money_left
                    .map(|left| left.min(fragment.x))
                    .unwrap_or(fragment.x),
            );
            let Some(value) = decimal(&fragment.text) else {
                continue;
            };
            match column {
                AmountColumn::Amount => amounts.push(value),
                AmountColumn::Balance => balances.push(value),
            }
        }

        let visual_row = row_index + 1;
        let amount = match amounts.as_slice() {
            [amount] => *amount,
            [] => {
                warnings.push(warning(
                    page_index + 1,
                    visual_row,
                    "could not read an amount",
                ));
                continue;
            }
            _ => {
                warnings.push(warning(
                    page_index + 1,
                    visual_row,
                    "the Amount column held more than one figure",
                ));
                continue;
            }
        };
        // A row with two figures under Balance is not evidence of
        // anything. Dropping the balance costs one piece of arithmetic;
        // picking one of them at random could cost the whole document.
        let balance = match balances.as_slice() {
            [balance] => Some(*balance),
            _ => None,
        };

        let merchant_limit = first_money_left.unwrap_or_else(|| headers.merchant_limit());
        let merchant = row
            .fragments
            .iter()
            .filter(|fragment| {
                fragment.x >= headers.description_left - ROW_TOLERANCE
                    && fragment.x < merchant_limit
            })
            .map(|fragment| fragment.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        if merchant.is_empty() {
            warnings.push(warning(
                page_index + 1,
                visual_row,
                "the description was empty",
            ));
            continue;
        }

        into.push(SignedRow {
            date,
            merchant,
            amount,
            balance,
        });
    }
}

#[cfg(feature = "pdf")]
mod extract {
    use super::{lines_from_fragments, Fragment};
    use super::{Page, PdfError};
    use pdfium_render::prelude::*;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    /// The platform's libpdfium file name (e.g. `libpdfium.dylib`), for
    /// doctor messages and tests.
    pub fn library_filename() -> String {
        Pdfium::pdfium_platform_library_name()
            .to_string_lossy()
            .into_owned()
    }

    /// Is the platform's libpdfium vendored in this directory?
    pub fn library_present(sidecars_dir: &Path) -> bool {
        Path::new(&Pdfium::pdfium_platform_library_name_at_path(sidecars_dir)).is_file()
    }

    /// Does it actually load? (#50)
    ///
    /// Present and loadable are different claims, and the gap between
    /// them is where a signed bundle fails: the hardened runtime
    /// enforces library validation, so a libpdfium signed by another
    /// team — or left ad-hoc by the build — is refused by dyld at
    /// `dlopen` while sitting exactly where it should be. A file check
    /// reports healthy for that bundle; this does not.
    ///
    /// Binding is the same memoized singleton a real extraction uses,
    /// so asking costs nothing the first read would not have paid.
    pub fn library_loads(sidecars_dir: &Path) -> bool {
        reader(sidecars_dir).is_ok()
    }

    /// The one libpdfium binding this process will ever have (#140).
    ///
    /// pdfium is a process-global singleton by design:
    /// `bind_to_library` refuses a second bind, and `Pdfium::new`
    /// asserts on the crate's own global bindings cell. Binding per call
    /// therefore meant the *second* statement read in a session failed —
    /// and failed claiming the PDF reader was missing, seconds after it
    /// had plainly worked. Two at once corrupted memory outright.
    ///
    /// Reading a current account and then a card is an ordinary thing to
    /// do, so this is bound once and shared. `Pdfium` declares itself
    /// `Send + Sync`, which is what makes a `static` legitimate rather
    /// than a gamble.
    static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

    /// Held for the whole of an extraction, not merely while binding.
    ///
    /// pdfium is single-threaded in practice: reading two documents at
    /// once through one binding aborts the process, so the crate's
    /// `unsafe impl Sync for Pdfium` is more optimistic than the library
    /// underneath it. Serialising here is cheap — Kettle runs one audit
    /// at a time (#117) and extraction is not a hot path — and it is the
    /// only arrangement that cannot corrupt memory.
    ///
    /// Deliberately not `OnceLock::get_or_init` for the binding: that
    /// would cache a *failure* for the life of the process, and a
    /// missing library is a thing that can be fixed while the app is
    /// open. The next attempt should get to find it.
    static PDFIUM_IN_USE: Mutex<()> = Mutex::new(());

    /// Bind if we haven't yet, then hand back the shared reader. Must be
    /// called with `PDFIUM_IN_USE` held.
    fn reader(sidecars_dir: &Path) -> Result<&'static Pdfium, PdfError> {
        if let Some(pdfium) = PDFIUM.get() {
            return Ok(pdfium);
        }

        let bindings =
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(sidecars_dir))
                .map_err(|e| match e {
                    // The only failure that really means "not installed".
                    PdfiumError::LoadLibraryError(_) => PdfError::LibraryMissing,
                    // Anything else is the reader being unavailable for
                    // some other reason, and must not be reported as an
                    // install problem — that was the original bug.
                    other => PdfError::ReaderUnavailable(other.to_string()),
                })?;
        let _ = PDFIUM.set(Pdfium::new(bindings));
        Ok(PDFIUM.get().expect("set under PDFIUM_IN_USE, just above"))
    }

    /// Positioned text from a statement PDF, preserving page boundaries
    /// and both horizontal edges for deterministic reconstruction.
    ///
    /// One caller at a time, for as long as the reading takes (#140).
    pub fn extract_pages(pdf: &Path, sidecars_dir: &Path) -> Result<Vec<Page>, PdfError> {
        // A poisoned lock still has to work: a panic while reading one
        // statement must not cost the ability to read any others.
        let _sole_user = PDFIUM_IN_USE
            .lock()
            .unwrap_or_else(|held| held.into_inner());
        let pdfium = reader(sidecars_dir)?;

        let document = pdfium
            .load_pdf_from_file(pdf, None)
            .map_err(|e| PdfError::Unreadable(e.to_string()))?;

        let mut pages = Vec::new();
        for page in document.pages().iter() {
            let text = page
                .text()
                .map_err(|e| PdfError::Unreadable(e.to_string()))?;
            let fragments: Vec<Fragment> = text
                .segments()
                .iter()
                .map(|segment| {
                    let bounds = segment.bounds();
                    Fragment {
                        text: segment.text(),
                        x: bounds.left().value,
                        right: bounds.right().value,
                        y: bounds.bottom().value,
                    }
                })
                .collect();
            pages.push(Page { fragments });
        }

        if pages.iter().all(|page| page.fragments.is_empty()) {
            return Err(PdfError::NoText);
        }
        Ok(pages)
    }

    /// Best-effort reading-order text for diagnostics (#13).
    pub fn extract_lines(pdf: &Path, sidecars_dir: &Path) -> Result<Vec<String>, PdfError> {
        Ok(extract_pages(pdf, sidecars_dir)?
            .iter()
            .flat_map(|page| lines_from_fragments(&page.fragments))
            .collect())
    }
}
