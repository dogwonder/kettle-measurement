//! CONTRACT FILE — don't change these shapes.
//!
//! The two documents a run writes to disk: `kettle/run-report@0`
//! (results.json) and `kettle/proposed-actions@0` (actions.json). They
//! cross a process boundary — the Tauri shell reads them and the report
//! template renders them — so they are mirrored, field for field, in
//! `app/src/lib/types.ts` and exemplified by `fixtures/run-01/`.
//!
//! Changing a field name here breaks the app silently. If a shape must
//! change, change it here, in `types.ts`, and regenerate the fixtures in
//! the same commit.
//!
//! **Money is a string, never a JSON number.** JSON numbers become
//! floats in JavaScript; amounts stay exact by staying text. `Decimal`
//! serialises to a string by default, which is exactly what we want —
//! do not enable `rust_decimal`'s `serde-float` feature.

use crate::recurrence::Period;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// `"monthly"`, `"quarterly"` … — the wire spelling of a cadence. The
/// in-memory `Period` stays an enum; this is only how it travels.
impl Period {
    pub fn as_wire(self) -> &'static str {
        match self {
            Period::Weekly => "weekly",
            Period::Monthly => "monthly",
            Period::Quarterly => "quarterly",
            Period::Annual => "yearly",
        }
    }

    pub fn from_wire(value: &str) -> Option<Period> {
        match value {
            "weekly" => Some(Period::Weekly),
            "monthly" => Some(Period::Monthly),
            "quarterly" => Some(Period::Quarterly),
            "yearly" => Some(Period::Annual),
            _ => None,
        }
    }
}

impl Serialize for Period {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for Period {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Period, D::Error> {
        let raw = String::deserialize(d)?;
        Period::from_wire(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown period: {raw}")))
    }
}

// ---------------------------------------------------------------------------
// kettle/run-report@0

pub const RUN_REPORT_SCHEMA: &str = "kettle/run-report@0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunReport {
    /// Always `RUN_REPORT_SCHEMA`.
    pub schema: String,
    pub run: RunInfo,
    pub summary: RunSummary,
    pub recurring: Vec<RecurringFinding>,
    pub regular_spend: Vec<RegularSpend>,
    pub income: Vec<Income>,
    pub needs_review: Vec<NeedsReviewItem>,
    pub check_yourself: Vec<CheckYourself>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInfo {
    pub id: String,
    pub pack: PackInfo,
    pub input: InputInfo,
    pub model: ModelInfo,
    /// RFC 3339, UTC, e.g. "2026-07-19T14:02:11Z".
    pub started: String,
    pub finished: String,
    /// ISO 4217, e.g. "GBP". Kettle does not convert between currencies.
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInfo {
    pub id: String,
    pub version: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputInfo {
    /// File name only — never a full path. Paths leak the person's home
    /// directory into a document they might share.
    pub file: String,
    pub rows: usize,
    pub period: DateRange,
    /// `"blake3:<hex>"` — the same hash the results cache is keyed on.
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The tier's human name, e.g. "Steady" — not a parameter count.
    pub tier: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub recurring_count: usize,
    pub annualised_total: Decimal,
    pub monthly_equivalent: Decimal,
    pub price_rises: usize,
    pub needs_review_count: usize,
    /// One plain sentence about what the totals do and don't include.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringFinding {
    pub merchant: String,
    /// How it appeared on the statement, so a person can find the row.
    pub raw_merchant: String,
    /// Open set owned by the pack's classify schema: "subscription",
    /// "utility", …
    pub kind: String,
    /// Open set owned by the pack's classify schema: "streaming", …
    pub category: String,
    pub period: Period,
    pub amount_current: Decimal,
    pub annualised: Decimal,
    pub confidence: Confidence,
    pub price_rise: Option<PriceRiseOut>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    /// Map the model's confidence string. Unrecognised strings are
    /// `Low`, never a panic — the model's vocabulary isn't ours to
    /// trust blindly. One implementation for every builder (#394): two
    /// copies had grown, and only one of them accepted "High".
    pub fn parse(raw: &str) -> Confidence {
        match raw.to_lowercase().as_str() {
            "high" => Confidence::High,
            "medium" => Confidence::Medium,
            _ => Confidence::Low,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceRiseOut {
    pub from: Decimal,
    pub to: Decimal,
    /// The month it landed, "YYYY-MM".
    pub month: String,
    /// What the rise costs over a year at this cadence.
    pub extra_per_year: Decimal,
}

/// Why a finding exists, in a form a person can check for themselves
/// (design philosophy: evidence per finding, #26).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// One plain sentence, e.g. "12 payments, one every month on or near
    /// the 5th".
    pub reason: String,
    /// Absent when there aren't two payments to measure between — a
    /// single yearly charge has no interval, and saying so is honest.
    pub interval_days: Option<IntervalDays>,
    pub transactions: Vec<TransactionOut>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntervalDays {
    pub median: i64,
    /// Max minus min, in days — how ragged the cadence is.
    pub spread: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionOut {
    pub date: NaiveDate,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegularSpend {
    pub merchant: String,
    pub raw_merchant: String,
    pub kind: String,
    pub category: String,
    pub visits: usize,
    pub total: Decimal,
    pub typical_visit: Decimal,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Income {
    pub merchant: String,
    pub raw_merchant: String,
    pub period: Period,
    pub amount: Decimal,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsReviewItem {
    pub raw_merchant: String,
    /// One plain sentence; this reaches a person, not a log.
    pub reason: String,
    pub transactions: Vec<TransactionOut>,
    /// The reassurance line: what not knowing costs them.
    pub note: String,
}

/// An honest checklist entry — something Kettle found but can't settle
/// on its own. Not an error state (#34).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckYourself {
    /// The merchant or thing this is about.
    pub about: String,
    /// Why it's worth your eyes, in plain British English.
    pub why: String,
}

// ---------------------------------------------------------------------------
// kettle/proposed-actions@0

pub const PROPOSED_ACTIONS_SCHEMA: &str = "kettle/proposed-actions@0";

/// The honesty note that travels with every actions document. Nothing
/// Kettle proposes ever happens by itself (brief §1, capabilities are
/// read-only).
pub const ACTIONS_NOTE: &str =
    "Nothing happens automatically. Approving an action only prepares it \
                                for you — as a calendar file (.ics) you add yourself, or text you \
                                can copy.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedActions {
    /// Always `PROPOSED_ACTIONS_SCHEMA`.
    pub schema: String,
    pub run_id: String,
    /// Always `ACTIONS_NOTE`.
    pub note: String,
    pub actions: Vec<ProposedAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    ReviewPriceRise,
    ReviewSubscription,
    CheckRenewal,
    CalendarReminder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedAction {
    /// "act-01", "act-02", … in emission order.
    pub id: String,
    pub kind: ActionKind,
    /// One line, the card's heading.
    pub title: String,
    /// Two or three sentences: what it means and what they might do.
    pub detail: String,
    /// Shape varies by kind; the screens read named fields defensively.
    pub evidence: std::collections::BTreeMap<String, String>,
    /// A photographed line used for this action's deadline did not
    /// read the same way twice. Empty for every ordinary action.
    ///
    /// Kept on the action rather than recovered from its evidence so
    /// the review screen shows the runner's decision, not a second
    /// approximation of it (#420).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disputed: Vec<DisputedOut>,
    pub export: ActionExport,
    /// The runner only ever proposes. Approved/dismissed is screen state.
    pub status: String,
}

/// Always `"proposed"` on anything the runner emits.
pub const STATUS_PROPOSED: &str = "proposed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionExport {
    /// Absent when the action has no day to sit on: a letter's undated
    /// ask gets a card and a line of text, never an event dated by
    /// Kettle (#399). RFC 5545 needs a DTSTART and the only date to
    /// hand would be today's, which the letter never wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ics: Option<IcsExport>,
    /// The same action as a line of text, for copying.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcsExport {
    pub summary: String,
    pub date: NaiveDate,
}

// ── The Extraction typology's report (#243) ─────────────────────────

/// A letter report is its own document, not a `RunReport` with the
/// money fields left empty. The two typologies answer different
/// questions, and a shared shape would mean every reader guessing
/// which half of it was meaningful — the confusion #238's typed
/// payload exists to prevent.
pub const LETTER_REPORT_SCHEMA: &str = "kettle/letter-report@0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterReport {
    /// Always `LETTER_REPORT_SCHEMA`.
    pub schema: String,
    pub run: LetterRunInfo,
    pub summary: LetterSummary,
    /// Soonest first, undated last — the order [`crate::timeline`]
    /// put them in, which is the order a person can act on.
    pub obligations: Vec<ObligationOut>,
    pub needs_review: Vec<NeedsReviewPassage>,
}

/// What was read, and by what. No `currency` and no model tier
/// pretending to be one: a letter report's facts are its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterRunInfo {
    pub id: String,
    pub pack: String,
    pub pack_version: String,
    /// The document read, by name.
    pub file: String,
    /// How many passages it came to.
    pub passages: usize,
    /// RFC 3339, UTC.
    pub started: String,
    pub finished: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterSummary {
    pub obligations_count: usize,
    /// How many carry a date somebody could diarise.
    pub dated_count: usize,
    /// How many are real asks whose deadline could not be resolved.
    /// Counted separately because they are the ones a person must
    /// judge, and hiding them in a total would be the lie.
    pub undated_count: usize,
    pub needs_review_count: usize,
    /// One or two plain sentences, generated from the counts alone.
    pub note: String,
}

/// One thing the letter asks, as the report shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationOut {
    pub kind: String,
    pub party: String,
    pub ask: String,
    /// The letter's own words for when.
    pub deadline: String,
    /// Resolved in Rust (#241), or `None` when the words could not be
    /// resolved. Never today's date standing in for an answer.
    ///
    /// Built only from the runner's own [`crate::timeline::Resolved`]
    /// rather than a date and a flag assembled separately, so the
    /// report cannot carry a kind that disagrees with the date it
    /// describes (#367). The template shows what the resolver decided;
    /// it never decides for itself.
    pub due: Option<DueOut>,
    pub confidence: Confidence,
    pub evidence: Vec<PassageOut>,
    /// The passage the due date was read out of, where the deadline
    /// pointed at one rather than stating it (#544).
    ///
    /// Shown, because the report otherwise asserts a date that appears
    /// nowhere in the passage quoted for it — every part of the claim
    /// backed except the part a person acts on, which is what #460's
    /// first rule refuses. Kept out of `evidence` because it is not a
    /// passage the model was asked about: it is one Rust went and read
    /// because the answer said where to look.
    #[serde(default)]
    pub dated_by: Option<PassageOut>,
    /// Where the two readings of a photographed page did not agree
    /// about the passage this was read from (#412 step 6).
    ///
    /// Empty for a text document, and empty for the great majority of
    /// photographs: the measured rate on a good page is 3%. That
    /// rarity is the feature — a mark that appeared on every letter
    /// would be read by nobody, which is the failure #412 was written
    /// to avoid.
    #[serde(default)]
    pub disputed: Vec<DisputedOut>,
}

/// One line two readings of a photograph made differently, as the
/// report shows it (#412 step 6).
///
/// Where it sat on the page is deliberately not carried. Position is
/// how the two readings were matched to each other; it is not
/// something to show anybody, and a report that held page coordinates
/// would invite a template to start laying out a photograph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputedOut {
    /// What the reading Kettle used made of it — the words the rest of
    /// the report is built on.
    pub read: String,
    /// What the second reading made of the same line. Empty if it saw
    /// nothing there at all, which is itself a disagreement: nothing
    /// confirmed the line.
    pub also_read: String,
    /// The complete first line a person sees. Authored here so the
    /// report and Action Review cannot describe the same disagreement
    /// differently.
    #[serde(default)]
    pub message: String,
    /// What the person should do before relying on the deadline.
    #[serde(default)]
    pub instruction: String,
}

impl From<&crate::ocr::Disagreement> for DisputedOut {
    fn from(disagreement: &crate::ocr::Disagreement) -> Self {
        let (message, instruction) = if disagreement.also_read.is_empty() {
            (
                "Reading the page a second time did not find this line at all.".to_owned(),
                "Nothing confirmed it, so check it against the letter before you rely on the date."
                    .to_owned(),
            )
        } else {
            (
                format!(
                    "Read a second time, this came out as “{}”",
                    disagreement.also_read
                ),
                "Kettle used the first reading. Check this against the letter before you rely on the date."
                    .to_owned(),
            )
        };

        DisputedOut {
            read: disagreement.read.clone(),
            also_read: disagreement.also_read.clone(),
            message,
            instruction,
        }
    }
}

/// A resolved deadline, as the report shows it (#405).
///
/// The date is already in words — [`crate::fmt::date`], the same
/// implementation the action card reads through — so one run states one
/// deadline one way. A template given the wire form would have to
/// format it itself, and a second formatter is how "1 September 2026"
/// and "2026-09-01" ended up describing the same day in the same run.
/// Constructed only via [`From<crate::timeline::Resolved>`], which is
/// what keeps the kind and the date travelling together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueOut {
    /// "17 March 2026" — never the serialised `NaiveDate`.
    pub date: String,
    pub kind: crate::claim::Kind,
}

impl From<crate::timeline::Resolved> for DueOut {
    fn from(resolved: crate::timeline::Resolved) -> Self {
        DueOut {
            date: crate::fmt::date(resolved.date),
            kind: resolved.kind,
        }
    }
}

/// A passage, cited as a person would find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassageOut {
    pub page: usize,
    pub text: String,
    /// The rows this passage was set in, where it came from a table
    /// (#406). Empty for prose, and a template renders the blockquote
    /// it always has when it is.
    ///
    /// The report is the only consumer. A blockquote tells a reader
    /// these are the document's own words in the document's own order;
    /// for a table assembled row-wise, half of that was true, and this
    /// is the half that was missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Vec<CellOut>>,
}

/// One cell of a quoted table, and whether it belongs to a column of
/// figures.
///
/// `numeric` is derived here rather than asked of the template, for the
/// reason the claim marks are (#366): a template may render a property
/// but may never decide one. "Every cell after the first" is a guess
/// about the data made in the copy layer, and it right-aligns a column
/// of words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellOut {
    pub text: String,
    /// Set only where the whole column is figures, so a column of money
    /// can be compared down its last digit — govuk's rule, applied on
    /// evidence rather than on position.
    pub numeric: bool,
}

/// A passage Kettle could not answer for, carried through with its
/// plain-language reason and counted nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsReviewPassage {
    pub text: String,
    pub reason: String,
    pub note: String,
}

/// One mapping for the two document typologies (#394): the letter and
/// comparison builders had the same six lines each, both apologising
/// that their subject wasn't a merchant — which was the field's old
/// name talking, not a real difference between them.
impl From<&crate::run::ReviewItem> for NeedsReviewPassage {
    fn from(item: &crate::run::ReviewItem) -> Self {
        NeedsReviewPassage {
            text: item.subject.clone(),
            reason: item.reason.clone(),
            note: crate::aggregate::NOT_COUNTED_NOTE.to_owned(),
        }
    }
}

// ── The Comparison typology's report (#66) ──────────────────────────

/// The third typology's document, for the reason the second one is its
/// own: a comparison answers "what moved between these two", which is
/// neither "what did this cost" nor "what does this ask of me". A
/// shared shape with half its fields empty would leave every reader
/// guessing which half meant anything (#238).
pub const COMPARISON_REPORT_SCHEMA: &str = "kettle/comparison-report@0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Always `COMPARISON_REPORT_SCHEMA`.
    pub schema: String,
    pub run: ComparisonRunInfo,
    pub summary: ComparisonSummary,
    /// What moved first, then what appeared, then what went, then what
    /// stayed — decided once here so a screen and a report cannot each
    /// pick their own order and disagree.
    pub changes: Vec<TermChangeOut>,
    pub needs_review: Vec<NeedsReviewPassage>,
}

/// What was read, and which document was which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonRunInfo {
    pub id: String,
    pub pack: String,
    pub pack_version: String,
    /// In the pack's declared order, earlier document first. A
    /// comparison that does not say which side is this year's can be
    /// read exactly backwards, which is this pack's worst harm (its
    /// README): it does not fail, it reverses.
    pub documents: Vec<ComparedDocument>,
    /// How many passages both documents came to.
    pub passages: usize,
    /// RFC 3339, UTC.
    pub started: String,
    pub finished: String,
}

/// One of the documents compared, named the three ways it can be:
/// the pack's role, the pack's words for it, and the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparedDocument {
    pub role: String,
    pub label: String,
    /// File name only — never a full path (`InputInfo`).
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonSummary {
    /// Every `(term, basis)` either document named.
    pub terms_count: usize,
    pub changed_count: usize,
    pub unchanged_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub needs_review_count: usize,
    /// One or two plain sentences, generated from the counts alone.
    pub note: String,
}

/// One named value's fate, as the report shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermChangeOut {
    /// The pack's enum value, for anything reading this document.
    pub term: String,
    /// The same term in a person's words. Derived from the enum rather
    /// than declared beside it (#367): a declared label can disagree
    /// with the value it names, and nothing would catch it.
    pub label: String,
    pub basis: String,
    /// "a year", "per claim" — the basis as it reads in a sentence.
    pub basis_label: String,
    pub state: ChangeState,
    /// The earlier document's value, verbatim as written. Absent where
    /// only the later document names the term.
    pub from: Option<String>,
    /// The later document's value, verbatim. Absent where only the
    /// earlier one names it.
    pub to: Option<String>,
    /// How big the move was, already formatted, and absent where the
    /// values are phrases — the template reformats nothing, and an
    /// invented number would be worse than no number (`terms`).
    pub delta: Option<String>,
    /// Which way it went. Absent for the same reason `delta` is.
    pub direction: Option<Direction>,
    /// What this row asserts, and therefore how it can be wrong (#366).
    pub kind: crate::claim::Kind,
    /// The passages behind the row, so it can be checked locally —
    /// each paired to the value it stands behind and to the document it
    /// was read from. One side per compared document, in document
    /// order (#379).
    pub sides: Vec<TermSideOut>,
}

/// One compared document's account of a term: what it said, and the
/// passage that says it (#379).
///
/// Pairing happens here rather than in a template because position is
/// not a pairing. Two passages in document order look paired until a
/// document is silent — an added term has one passage and two sides,
/// and a reader who mis-assigns it reads this year's words as last
/// year's. So the runner answers it once (#361) and the template
/// arranges what it is given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermSideOut {
    /// The pack's own label for the document — "Last year's policy",
    /// never the role key and never a file name.
    pub label: String,
    /// What this document said the value was, verbatim. `None` where
    /// this document does not state the term at all: a fact the report
    /// shows ("Not stated"), not an absence to drop.
    pub value: Option<String>,
    /// The passage this document's value was read from. Absent
    /// alongside an absent value, for the same reason.
    pub quote: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeState {
    Changed,
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
}
