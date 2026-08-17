//! Statement preprocessing: CSV → `Transaction`, deterministically.
//! Amounts are exact decimals (never floats); direction is explicit.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub date: NaiveDate,
    pub raw_merchant: String,
    /// Magnitude — always non-negative; see `direction`.
    pub amount: Decimal,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Money out of the account.
    Debit,
    /// Money into the account.
    Credit,
}

impl Direction {
    fn opposite(self) -> Self {
        match self {
            Self::Debit => Self::Credit,
            Self::Credit => Self::Debit,
        }
    }
}

/// File-level failures only — a row Kettle can't make sense of becomes a
/// `ParsedStatement` warning, never an error.
#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Csv(csv::Error),
    Pdf(crate::pdf::PdfError),
    /// A picture of a document could not be turned into text (#399).
    ///
    /// Its own variant rather than folded into
    /// [`ParseError::UnsupportedFileType`], because that sentence tells
    /// a person to choose a different kind of file and they chose the
    /// right kind. What went wrong is about this photograph, or about
    /// this build of Kettle, and the wording differs accordingly.
    Ocr(crate::ocr::OcrError),
    UnsupportedFileType(String),
    UnrecognisedColumns {
        headers: Vec<String>,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(e) => write!(f, "could not open the file: {e}"),
            ParseError::Csv(e) => write!(f, "could not read the file: {e}"),
            ParseError::Pdf(e) => write!(f, "{e}"),
            ParseError::Ocr(e) => write!(f, "{e}"),
            ParseError::UnsupportedFileType(kind) => write!(
                f,
                "cannot read this kind of file ({kind}) — choose a CSV or text-layer PDF statement"
            ),
            ParseError::UnrecognisedColumns { headers } => {
                let mut banks: Vec<&str> = FORMATS.iter().filter_map(|f| f.bank).collect();
                banks.extend(["Monzo", "Barclaycard Business"]);
                banks.sort_unstable();
                banks.dedup();
                write!(
                    f,
                    "did not recognise the columns: {}. Kettle can read statements \
                     exported from {}, or any file with Date, Description and \
                     Amount (or Debit and Credit) columns",
                    headers.join(", "),
                    banks.join(" or ")
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<csv::Error> for ParseError {
    fn from(e: csv::Error) -> Self {
        ParseError::Csv(e)
    }
}

impl From<crate::pdf::PdfError> for ParseError {
    fn from(e: crate::pdf::PdfError) -> Self {
        ParseError::Pdf(e)
    }
}

/// How a statement expresses money movement, worked out from its header
/// row.
#[derive(Clone, Copy)]
enum AmountColumns {
    /// One signed column. Its name establishes the positive direction;
    /// a negative value is the opposite direction.
    Signed {
        amount: usize,
        positive_direction: Direction,
    },
    /// Separate columns; whichever is populated gives the direction.
    DebitCredit { debit: usize, credit: usize },
}

/// One bank's CSV export shape (#12). Growable: add a row to `FORMATS`.
/// A format claims a file when every named header is present — so
/// bank-specific formats must come before the generic ones, which use
/// column names (Date, Description, Amount) that banks also use.
struct BankFormat {
    /// Bank name for the unrecognised-columns message; `None` for the
    /// generic formats, which that message describes by their columns.
    bank: Option<&'static str>,
    date: &'static str,
    date_formats: &'static [&'static str],
    merchant: &'static str,
    amounts: AmountHeaders,
    /// Extra headers that must also be present for this format to claim
    /// the file — distinctive columns we don't otherwise read.
    signature: &'static [&'static str],
}

enum AmountHeaders {
    Signed {
        header: &'static str,
        positive_direction: Direction,
    },
    DebitCredit {
        debit: &'static str,
        credit: &'static str,
    },
}

/// Checked in order; first match wins.
const FORMATS: &[BankFormat] = &[
    // Monzo app export ("Monzo Data Export"). `Name` is the friendly
    // merchant; `Description` is raw scheme text, so generic must not win.
    BankFormat {
        bank: Some("Monzo"),
        date: "Date",
        date_formats: &["%d/%m/%Y"],
        merchant: "Name",
        amounts: AmountHeaders::Signed {
            header: "Amount",
            positive_direction: Direction::Credit,
        },
        signature: &["Transaction ID", "Emoji", "Category"],
    },
    // Starling app export.
    BankFormat {
        bank: Some("Starling"),
        date: "Date",
        date_formats: &["%d/%m/%Y"],
        merchant: "Counter Party",
        amounts: AmountHeaders::Signed {
            header: "Amount (GBP)",
            positive_direction: Direction::Credit,
        },
        signature: &[],
    },
    // HSBC. The current account and the business card export the same
    // four columns, so one row serves both (#136). Year width varies by
    // export — the CSV writes 2025, the PDF statement of the same
    // account writes 25 (#137) — and `date_formats` is tried in order,
    // so both are simply listed.
    //
    // On the card, `Paid In` is a repayment rather than earnings; that
    // is still money coming in, so the mapping is the same as a current
    // account's and needs no special case. It does need a test, because
    // getting it backwards reports every purchase as income.
    BankFormat {
        bank: Some("HSBC"),
        date: "Date",
        // Two-digit year FIRST. `%Y` is variable-width and will read
        // "25" as year 0025 rather than failing, so a wider format
        // ahead of a narrower one doesn't fall through to it — it
        // silently wins with a date nineteen centuries out. `%y`
        // requires exactly two digits, so it rejects "2025" cleanly and
        // the fall-through works in this order and not the other.
        date_formats: &["%d %b %y", "%d %b %Y", "%d/%m/%Y"],
        merchant: "Description",
        amounts: AmountHeaders::DebitCredit {
            debit: "Paid Out",
            credit: "Paid In",
        },
        signature: &[],
    },
    // First Direct exports one signed `Inflow` column. Positive values
    // are money in; negative values imply money out.
    BankFormat {
        bank: Some("First Direct"),
        date: "Date",
        date_formats: &["%d %b %y", "%d/%m/%Y", "%Y-%m-%d"],
        merchant: "Payee",
        amounts: AmountHeaders::Signed {
            header: "Inflow",
            positive_direction: Direction::Credit,
        },
        signature: &[],
    },
    // Both John Lewis formats use one signed `Outflow` column. The
    // pre-2022 export uses named months; NewDay uses slashes. The
    // space-separated date also covers the equivalent Barclaycard CSV.
    BankFormat {
        bank: Some("John Lewis Partnership Card"),
        date: "Date",
        // Narrow before wide, for the reason spelled out on the HSBC
        // row: `%d-%b-%Y` alone reads "18-Dec-25" as year 0025 rather
        // than failing through to a two-digit format behind it.
        date_formats: &["%d-%b-%y", "%d-%b-%Y", "%d/%m/%Y", "%d %b %y"],
        merchant: "Payee",
        amounts: AmountHeaders::Signed {
            header: "Outflow",
            positive_direction: Direction::Debit,
        },
        signature: &[],
    },
    // Generic statements (own-bank fixtures): ISO dates, signed amount.
    BankFormat {
        bank: None,
        date: "Date",
        date_formats: &["%Y-%m-%d"],
        merchant: "Description",
        amounts: AmountHeaders::Signed {
            header: "Amount",
            positive_direction: Direction::Credit,
        },
        signature: &[],
    },
    // Generic with separate Debit/Credit columns.
    BankFormat {
        bank: None,
        date: "Date",
        date_formats: &["%Y-%m-%d"],
        merchant: "Description",
        amounts: AmountHeaders::DebitCredit {
            debit: "Debit",
            credit: "Credit",
        },
        signature: &[],
    },
];

struct Layout {
    date: usize,
    date_formats: &'static [&'static str],
    description: usize,
    amounts: AmountColumns,
}

impl Layout {
    fn detect_named(headers: &csv::StringRecord) -> Option<Self> {
        let normalised_headers: Vec<String> = headers.iter().map(normalise_header).collect();
        let find = |name: &str| {
            let name = normalise_header(name);
            normalised_headers.iter().position(|header| header == &name)
        };

        for format in FORMATS {
            if !format.signature.iter().all(|h| find(h).is_some()) {
                continue;
            }
            let columns = (find(format.date), find(format.merchant));
            let (Some(date), Some(description)) = columns else {
                continue;
            };
            let amounts = match format.amounts {
                AmountHeaders::Signed {
                    header,
                    positive_direction,
                } => match find(header) {
                    Some(amount) => AmountColumns::Signed {
                        amount,
                        positive_direction,
                    },
                    None => continue,
                },
                AmountHeaders::DebitCredit { debit, credit } => match (find(debit), find(credit)) {
                    (Some(debit), Some(credit)) => AmountColumns::DebitCredit { debit, credit },
                    _ => continue,
                },
            };
            return Some(Layout {
                date,
                date_formats: format.date_formats,
                description,
                amounts,
            });
        }

        None
    }

    fn unrecognised(headers: &csv::StringRecord) -> ParseError {
        ParseError::UnrecognisedColumns {
            headers: headers.iter().map(str::to_owned).collect(),
        }
    }
}

fn normalise_header(header: &str) -> String {
    header
        .trim()
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase()
}

const OLD_MONZO_DATES: &[&str] = &["%Y-%m-%d %H:%M:%S %z"];
const BARCLAYCARD_BUSINESS_DATES: &[&str] = &["%d/%m/%Y"];

fn detect_positional(headers: &csv::StringRecord, record: &csv::StringRecord) -> Option<Layout> {
    let field = |index: usize| record.get(index).unwrap_or("").trim();
    let is_decimal = |index| Decimal::from_str_exact(field(index)).is_ok();
    let is_currency = |index| {
        let value = field(index);
        value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
    };

    // Original Monzo Data Export: id, created, amount, currency,
    // local_amount, local_currency, category, emoji, description,
    // address, notes, receipt. Headers changed over time, so recognise
    // the distinctive row shape and then use the documented positions.
    if headers.len() == 12
        && record.len() == 12
        && parse_date(field(1), OLD_MONZO_DATES).is_some()
        && is_decimal(2)
        && is_currency(3)
        && is_decimal(4)
        && is_currency(5)
        && !field(8).is_empty()
    {
        return Some(Layout {
            date: 1,
            date_formats: OLD_MONZO_DATES,
            description: 8,
            amounts: AmountColumns::Signed {
                amount: 2,
                positive_direction: Direction::Credit,
            },
        });
    }

    // Barclaycard Business exports have existed with 20 and 21
    // columns. The stable leading positions are transaction date,
    // merchant, amount/currency, original amount/currency and posted
    // date. Requiring all of that shape avoids claiming an arbitrary
    // wide CSV merely because its column count matches.
    if matches!(headers.len(), 20 | 21)
        && record.len() == headers.len()
        && parse_date(field(2), BARCLAYCARD_BUSINESS_DATES).is_some()
        && !field(3).is_empty()
        && is_decimal(4)
        && is_currency(5)
        && is_decimal(6)
        && is_currency(7)
        && is_decimal(8)
        && parse_date(field(9), BARCLAYCARD_BUSINESS_DATES).is_some()
    {
        return Some(Layout {
            date: 2,
            date_formats: BARCLAYCARD_BUSINESS_DATES,
            description: 3,
            amounts: AmountColumns::Signed {
                amount: 4,
                positive_direction: Direction::Debit,
            },
        });
    }

    None
}

fn parse_date(value: &str, formats: &[&str]) -> Option<NaiveDate> {
    for format in formats {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date);
        }
        if let Ok(date_time) = DateTime::parse_from_str(value, format) {
            return Some(date_time.date_naive());
        }
        if let Ok(date_time) = NaiveDateTime::parse_from_str(value, format) {
            return Some(date_time.date());
        }
    }
    None
}

fn magnitude(value: Decimal) -> Decimal {
    if value.is_sign_negative() {
        -value
    } else {
        value
    }
}

/// A parsed statement: the rows that made sense, plus plain-language
/// warnings for any that didn't. One bad row never fails the file.
#[derive(Debug)]
pub struct ParsedStatement {
    pub transactions: Vec<Transaction>,
    pub warnings: Vec<String>,
}

pub fn parse_statement_file(path: &Path) -> Result<ParsedStatement, ParseError> {
    let file = std::fs::File::open(path).map_err(ParseError::Io)?;
    parse_statement(file)
}

/// Choose the deterministic reader by file extension. A PDF needs the
/// directory containing the platform libpdfium; builds without the PDF
/// feature fail visibly rather than sending binary bytes to the CSV
/// parser.
pub fn parse_input_file(
    path: &Path,
    pdfium_dir: Option<&Path>,
) -> Result<ParsedStatement, ParseError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "csv" => parse_statement_file(path),
        "pdf" => {
            #[cfg(feature = "pdf")]
            {
                let pdfium_dir =
                    pdfium_dir.ok_or(ParseError::Pdf(crate::pdf::PdfError::LibraryMissing))?;
                let pages = crate::pdf::extract_pages(path, pdfium_dir)?;
                crate::pdf::reconstruct_statement(&pages).map_err(ParseError::Pdf)
            }
            #[cfg(not(feature = "pdf"))]
            {
                let _ = pdfium_dir;
                Err(ParseError::Pdf(crate::pdf::PdfError::LibraryMissing))
            }
        }
        "" => Err(ParseError::UnsupportedFileType(
            "no filename extension".to_owned(),
        )),
        other => Err(ParseError::UnsupportedFileType(format!(".{other}"))),
    }
}

pub fn parse_statement<R: std::io::Read>(reader: R) -> Result<ParsedStatement, ParseError> {
    let mut csv_reader = csv::Reader::from_reader(reader);
    let headers = csv_reader.headers()?.clone();
    let mut records = csv_reader.records();
    let mut buffered_records = Vec::new();
    let layout = match Layout::detect_named(&headers) {
        Some(layout) => layout,
        None => loop {
            let Some(record) = records.next().transpose()? else {
                return Err(Layout::unrecognised(&headers));
            };
            let detected = detect_positional(&headers, &record);
            buffered_records.push(record);
            if let Some(layout) = detected {
                break layout;
            }
        },
    };

    let mut transactions = Vec::new();
    let mut warnings = Vec::new();
    let all_records = buffered_records.into_iter().map(Ok).chain(records);
    for (index, record) in all_records.enumerate() {
        let record = record?;
        let row = index + 2; // 1-based, after the header line

        let field = |column: usize| record.get(column).unwrap_or("").trim();

        let date_value = field(layout.date);
        let date = match parse_date(date_value, layout.date_formats) {
            Some(date) => date,
            None => {
                warnings.push(format!(
                    "row {row}: could not read the date {date_value:?} — row skipped"
                ));
                continue;
            }
        };

        let merchant = field(layout.description);
        if merchant.is_empty() {
            warnings.push(format!("row {row}: merchant is empty — row skipped"));
            continue;
        }

        let (amount_value, positive_direction, sign_changes_direction) = match layout.amounts {
            AmountColumns::Signed {
                amount,
                positive_direction,
            } => (field(amount), positive_direction, true),
            AmountColumns::DebitCredit { debit, credit } => {
                let debit_value = field(debit);
                let credit_value = field(credit);
                match (debit_value.is_empty(), credit_value.is_empty()) {
                    (false, false) => {
                        warnings.push(format!(
                            "row {row}: both Debit and Credit are populated — row skipped"
                        ));
                        continue;
                    }
                    (true, true) => {
                        warnings.push(format!(
                            "row {row}: both Debit and Credit are empty — row skipped"
                        ));
                        continue;
                    }
                    (false, true) => (debit_value, Direction::Debit, false),
                    (true, false) => (credit_value, Direction::Credit, false),
                }
            }
        };

        if amount_value.is_empty() {
            warnings.push(format!("row {row}: amount is empty — row skipped"));
            continue;
        }
        let signed = match Decimal::from_str_exact(amount_value) {
            Ok(amount) => amount,
            Err(_) => {
                warnings.push(format!(
                    "row {row}: could not read the amount {amount_value:?} — row skipped"
                ));
                continue;
            }
        };
        if signed.is_zero() {
            warnings.push(format!("row {row}: amount is zero — row skipped"));
            continue;
        }
        let direction = if sign_changes_direction && signed.is_sign_negative() {
            positive_direction.opposite()
        } else {
            positive_direction
        };
        let amount = magnitude(signed);

        transactions.push(Transaction {
            date,
            raw_merchant: merchant.to_owned(),
            amount,
            direction,
        });
    }
    Ok(ParsedStatement {
        transactions,
        warnings,
    })
}
