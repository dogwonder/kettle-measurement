//! The pipeline itself (#78): compose parsing, cleanup, model steps and
//! recurrence detection into one run of a pack against real files.
//!
//! Boundaries (decided on the issue): the caller owns the sidecar and
//! passes an endpoint; the whole-run cache (#18) wraps *around* this
//! call; persistence (#24), totals (#29), actions (#30) and rendering
//! (#32) consume `RunOutcome` and live with their own issues.

use crate::cleanup::{self, MerchantGroup};
use crate::exec::{
    run_batch, BatchContext, BatchItem, Endpoint, ModelCallError, ModelMetrics, NeedsReview,
    ReviewReason, StepError,
};
use crate::packs::{step_batches, FileSemantics, InputSpec, Manifest, Pack, PipelineStep};
use crate::parse::{parse_input_file, ParseError, Transaction};
use crate::recurrence::{detect_income, detect_recurring, looks_periodic, Period, PriceRise};
use crate::run_dir::RunLog;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The confidence that means "show a person". The pipeline owns the
/// spelling because the pipeline is what emits it — the scorer reads
/// the same constant rather than keeping a second copy, since one fact
/// written down twice is how #251 happened.
pub const LOW_CONFIDENCE: &str = "low";

/// One progress heartbeat for the shell's step display (§7b). The label
/// is plain language — it goes straight to a person.
pub struct Progress<'a> {
    pub step: &'a str,
    pub current: usize,
    pub total: usize,
}

/// What one pipeline step is called on the progress screen, or `None`
/// for a step that emits no progress (the optional prose summary, whose
/// deterministic fallback makes skipping it honest).
///
/// This is the single source the run emits from and the sequence
/// derives from (#244): a label written here and nowhere else cannot
/// drift between what the screen predicts and what the run reports.
/// Keyed on role and builtin — typology-level closed sets — never on a
/// pack id.
///
/// `documents` is how many the run reads: the declared roles when
/// predicting a sequence, the bound files when emitting. The two agree
/// today because the shell binds one file per role (#342); if that
/// changes, an unpredicted plural label appends on the progress screen
/// rather than being dropped.
pub fn step_label(step: &PipelineStep, documents: usize) -> Option<&'static str> {
    match step {
        PipelineStep::Preprocess { implementation }
            if implementation == "builtin:document-text" =>
        {
            Some(if documents <= 1 {
                "Reading your document"
            } else {
                "Reading your documents"
            })
        }
        PipelineStep::Preprocess { .. } => Some("Reading your statement"),
        PipelineStep::Model { schema: None, .. } => None,
        PipelineStep::Model { role, .. } => match role.as_deref() {
            Some("normalise") => Some("Grouping payments by merchant"),
            Some("classify") => Some("Sorting merchants"),
            Some("obligations") => Some("Reading what it asks of you"),
            Some("policy-terms") => Some("Reading what each document says"),
            _ => None,
        },
        PipelineStep::Aggregate { implementation } => match implementation.as_str() {
            "builtin:recurrence-detect" => Some("Checking for price rises"),
            "builtin:timeline-sort" => Some("Working out the deadlines"),
            "builtin:term-diff" => Some("Comparing the two documents"),
            _ => None,
        },
        PipelineStep::Render { .. } => Some("Writing your report"),
    }
}

/// The progress labels a run of this manifest will report, in order —
/// what the shell shows as the expected sequence before any event
/// arrives (#244).
pub fn step_labels(manifest: &Manifest) -> Vec<String> {
    manifest
        .pipeline
        .iter()
        .filter_map(|step| step_label(step, manifest.inputs.len()))
        .map(str::to_owned)
        .collect()
}

/// One dated payment, carried as evidence so every finding can show the
/// transactions that produced it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    pub date: NaiveDate,
    pub amount: Decimal,
}

/// A recurring series the run found, classified and evidenced. Low
/// confidence is a finding too — it surfaces in "check these yourself",
/// it isn't hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub merchant: String,
    pub raw_merchant: String,
    pub kind: String,
    /// Which decision produced `kind` (#272). Recorded here, at the
    /// branch, because it cannot be recovered from the string later.
    pub kind_from: crate::kinds::KindFrom,
    pub category: String,
    pub confidence: String,
    pub period: Period,
    pub current_amount: Decimal,
    pub price_rise: Option<PriceRise>,
    pub evidence: Vec<Evidence>,
}

/// One thing the run couldn't answer for, with a plain-language reason
/// and its payments attached. Not counted anywhere until a person
/// decides.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewItem {
    /// What the person is asked to look at, as they would recognise it:
    /// the raw merchant descriptor in an audit, the passage's own text
    /// in a document typology. Named `raw_merchant` until #394 — an
    /// audit-typology name two other typologies had to explain away.
    /// The alias keeps a parked `pending.json` written before the
    /// rename readable.
    #[serde(alias = "raw_merchant")]
    pub subject: String,
    pub reason: String,
    pub transactions: Vec<Evidence>,
}

/// A merchant the run classified but found no repeating series for —
/// the weekly shop, a one-off. Kept because the report shows regular
/// spending alongside subscriptions, and because dropping classified
/// work silently is how totals stop adding up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spend {
    pub merchant: String,
    pub raw_merchant: String,
    pub kind: String,
    /// Which decision produced `kind` (#272).
    pub kind_from: crate::kinds::KindFrom,
    pub category: String,
    pub confidence: String,
    pub transactions: Vec<Evidence>,
}

/// What was read: how many payments, spanning which dates. The
/// report's cover facts (#46), counted here because only the pipeline
/// knows which rows became transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSeen {
    pub rows: usize,
    /// First and last transaction date; `None` when nothing was read.
    pub period: Option<(NaiveDate, NaiveDate)>,
}

/// One completed run: the envelope every run has, whatever the pack
/// asked, plus the payload of the typology that ran (#51, #238).
///
/// The split is the scaling claim from the plugin architecture doc made
/// structural. What was read, what nobody could answer and what went
/// wrong are true of a subscription audit and of a letter alike, so a
/// caller reads them without knowing which ran. Findings, income and
/// regular spending are the Audit typology's and mean nothing to a
/// letter, so reading them costs a `match` — which is the point. A
/// second typology adds a variant here; a second *pack within* a
/// typology adds nothing at all, and that is what "data-only packs"
/// has to mean to be worth saying.
#[derive(Debug)]
pub struct RunOutcome {
    pub input: InputSeen,
    /// What was read and what each document was supplied *as* (#332),
    /// in the order the run was given them — so the position in this
    /// list is the [`crate::document::Segment::document`] index from
    /// #330. That is what lets a later step ask which document a value
    /// came out of, which is the whole of a renewal diff (#66).
    pub inputs: Vec<BoundInput>,
    pub needs_review: Vec<ReviewItem>,
    pub warnings: Vec<String>,
    /// Diagnostic evidence connecting model candidates to the guards
    /// that accepted or contained them (#425). Never rendered as report
    /// copy and never consulted to make a product decision.
    pub claim_traces: Vec<crate::claim_trace::ClaimTrace>,
    pub payload: Payload,
}

/// One input file, and the role the run supplied it as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundInput {
    /// A role the pack declares in its manifest.
    pub role: String,
    /// File name only, never a full path — a path leaks the person's
    /// home directory into a document they might share (`InputInfo`).
    pub file: String,
}

/// Why a set of files could not be bound to the roles a pack declares.
///
/// Every one of these is refused before a single model call: a run that
/// cannot say which document is which has nothing honest to report, and
/// finding out afterwards costs minutes of a person's time and battery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputBindingError {
    /// A flat list of files was given to a pack declaring more than one
    /// role, so which file is which is unstated.
    ///
    /// Binding by argument order was deliberately not done. It is
    /// invisible at the call site and unverifiable afterwards, and
    /// getting it the wrong way round does not fail — it silently
    /// reverses the comparison. A reversed renewal diff reports a price
    /// cut where there was a rise, which is worse than any error.
    RoleUnstated { declared: Vec<String> },
    /// The pack declares this role and the run supplied nothing for it.
    MissingRole { role: String },
    /// The role got more files than its declared count allows.
    TooMany {
        role: String,
        given: usize,
        /// The declaration in a person's words ("one file", "up to
        /// twelve files") — `Count::in_words`.
        takes: String,
    },
    /// The role got fewer files than its declared count needs (#334
    /// §1). Distinct from `MissingRole`, which is nothing at all: "one
    /// payslip where three were asked for" is a different sentence from
    /// "no payslips", and a person can act on the difference.
    TooFew {
        role: String,
        given: usize,
        takes: String,
    },
    /// A file whose type this role does not accept (#334 §2). Refused
    /// at binding, before any model call: the alternative is finding
    /// out mid-run, per file, from a step that knows nothing about
    /// which role wanted what.
    WrongType {
        role: String,
        file: String,
        accepted: Vec<String>,
    },
    /// A role the pack never declared. The caller is confused about
    /// which pack it is running, and guessing which declared role was
    /// meant would be inventing an answer.
    UndeclaredRole { role: String },
}

impl std::fmt::Display for InputBindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputBindingError::RoleUnstated { declared } => write!(
                f,
                "this pack needs you to say which file is which ({})",
                declared.join(", ")
            ),
            InputBindingError::MissingRole { role } => {
                write!(f, "this pack still needs a file for “{role}”")
            }
            InputBindingError::TooMany { role, given, takes } => {
                write!(f, "this pack takes {takes} for “{role}”, and got {given}")
            }
            InputBindingError::TooFew { role, given, takes } => {
                write!(f, "this pack takes {takes} for “{role}”, and got {given}")
            }
            InputBindingError::WrongType {
                role,
                file,
                accepted,
            } => write!(
                f,
                "“{file}” is not a kind of file this pack can read for “{role}” ({})",
                accepted.join(", ")
            ),
            InputBindingError::UndeclaredRole { role } => {
                write!(f, "this pack has nothing called “{role}”")
            }
        }
    }
}

impl std::error::Error for InputBindingError {}

/// What a run found, in the shape its typology finds things.
///
/// Deliberately not `serde_json::Value`: a payload the compiler cannot
/// check is one the templates and the app have to agree about by
/// convention, and a pack and a runner disagreeing silently is the
/// failure #120 exists to prevent.
#[derive(Debug)]
pub enum Payload {
    /// Structured records in, findings and totals out.
    Audit(AuditOutcome),
    /// A document in, what it obliges someone to do out (#240).
    Extraction(ExtractionOutcome),
    /// Two documents in, what moved between them out (#350, for #66).
    Comparison(ComparisonOutcome),
}

/// The Comparison typology's answers (#350): the named values read out
/// of each document, and the diff across them.
///
/// Both are kept. The diff is what a person reads, and the terms are
/// what it was computed from — a report that showed only the diff would
/// present the pairing as settled, which is exactly the move that made
/// the subscription audit unauditable (#348).
#[derive(Debug, Default)]
pub struct ComparisonOutcome {
    pub terms: Vec<crate::terms::Term>,
    pub diff: Vec<crate::terms::TermDiff>,
    /// Terms read and deliberately not compared (#377). Kept apart from
    /// `needs_review`: that is what the model could not answer, this is
    /// what Rust declined to pair. The report shows both to a person,
    /// under the same heading, saying why for each.
    pub not_compared: Vec<crate::terms::NotCompared>,
}

/// One thing a document asks somebody to do (#240). The model reads
/// dates exactly as they are written and never computes one: a letter
/// saying "within 14 days" produces that phrase and the anchor it
/// counts from, and the arithmetic is Rust's (#241).
// Not `Eq`: a disputed line carries where it sat on the page, and page
// geometry is measured, not counted.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Obligation {
    /// What sort of ask this is — enum-constrained in the pack's schema.
    pub kind: String,
    /// Who is asking, as the document names them.
    pub party: String,
    /// What is being asked, for a person to read.
    pub ask: String,
    /// The deadline as written — a phrase, never a computed date.
    pub deadline: String,
    /// What the deadline counts from, as written ("the date of this
    /// letter", "12 August 2026").
    pub anchor: String,
    /// The sum this ask is for, copied from the passage as written
    /// ("£84.00"), or the sentinel `no amount` (#612). Never parsed and
    /// never operated on here: reading a figure off the page is the
    /// same act as reading a deadline phrase, and anything done with it
    /// afterwards is Rust's. Defaults on deserialisation so recordings
    /// and results written before the field existed still load.
    #[serde(default = "no_amount")]
    pub amount: String,
    /// The passage the model says the sum is printed in, as a batch id
    /// (an index into the run's segments), when it is not this one.
    /// Rust verifies the figure is there and carries the passage as
    /// `priced_by`; it never searches for one itself (CLAUDE.md, *Rust
    /// verifies; it never discovers*).
    #[serde(default)]
    pub amount_from: Option<usize>,
    /// The passage the model says the deadline's date is printed in,
    /// when the deadline points elsewhere on the page. Rust reads one
    /// full date from it and carries the passage as `dated_by`.
    #[serde(default)]
    pub deadline_from: Option<usize>,
    /// The model's confidence about this segment's reading. Low means
    /// "check this yourself", exactly as it does for a classification.
    pub confidence: String,
    /// When it falls due, resolved in Rust by `builtin:timeline-sort`
    /// (#241) — `None` until that step runs, and honestly `None` after
    /// it for a phrase the resolver does not understand.
    ///
    /// It carries the kind of claim the date makes with it (#367): a
    /// date read off the page and a date Rust counted arrive at a person
    /// identically, and they are wrong in different ways. The resolver
    /// is the only step that knows which, so it travels from there
    /// rather than being re-guessed by a template.
    pub due: Option<crate::timeline::Resolved>,
    /// The passages it came from, cited as a person would find them.
    /// More than one when overlapping segments said the same thing and
    /// the duplicates merged.
    pub evidence: Vec<crate::document::Segment>,
    /// The passage the due date was read out of, when the deadline
    /// pointed at one instead of stating it (#544).
    ///
    /// Separate from `evidence`, and the separation is the point. The
    /// passages in `evidence` are ones the model was *asked about* and
    /// answered; this is one Rust went and read afterwards, because the
    /// answer said where to look. Merging the two would make the run
    /// look as though it had asserted an obligation on a due-date row —
    /// which is exactly what the bed measures as an invention there,
    /// and what the replay of the v14 letter run reported when this
    /// was first written as an extra `evidence` entry.
    ///
    /// It is carried rather than dropped because the report asserts a
    /// date the pointing passage does not contain, and #460's first
    /// rule is that a quote must contain the value it evidences. The
    /// row is that quote.
    #[serde(default)]
    pub dated_by: Option<crate::document::Segment>,
    /// The row the sum was read out of, when the ask's own passage
    /// printed none and the letter labelled one — an amount-due, total
    /// or balance row (#612, second half). #544's shape, for the same
    /// reason: `evidence` is what the model was asked about, and a row
    /// added there would read downstream as an obligation asserted on
    /// an amount row. Rust read it; nothing was computed.
    #[serde(default)]
    pub priced_by: Option<crate::document::Segment>,
    /// Lines within those passages the two readings of the photograph
    /// did not agree about (#412 step 6).
    ///
    /// Empty for anything that was not a photograph, and empty for the
    /// great majority of photographed letters: the measured dispute
    /// rate on a good page is 3%, and a dispute is only carried here if
    /// it landed in a passage this obligation was read from. A disputed
    /// letterhead is noise and stops at the page.
    ///
    /// Decided in the runner so the screen does not re-derive it
    /// (#361), and carried rather than reduced to a flag so the review
    /// can show a person both readings — the question "is this date
    /// right?" is answerable in seconds; "was this line uncertain?" is
    /// not.
    #[serde(default)]
    pub disputed: Vec<crate::ocr::Disagreement>,
}

/// The sentinel an obligation carries where its passage names no sum.
pub const NO_AMOUNT: &str = "no amount";

fn no_amount() -> String {
    NO_AMOUNT.to_owned()
}

/// The same default, reachable from the eval's expectation type.
pub fn no_amount_string() -> String {
    no_amount()
}

impl Obligation {
    /// What this obligation *is*, for every comparison the eval harness
    /// makes (#554) — the same identity a bed's expectation carries.
    pub fn identity(&self) -> crate::eval::ObligationIdentity {
        crate::eval::ObligationIdentity::of(
            &self.kind,
            &self.party,
            &self.deadline,
            &self.anchor,
            &self.amount,
            self.due.map(|d| d.date),
        )
    }
}

/// Mark each obligation with the disputes that landed in its own
/// passages (#412 step 6).
///
/// Whole-page disputes are deliberately not propagated. The design this
/// implements is a gate that is rare enough to be read, and the
/// measured letter disputed its registered-office postcode — a line
/// that costs nobody anything and appears on every letter that company
/// sends. Marking a deadline because of it would make the mark
/// meaningless in exactly the way #412 was written to prevent.
///
/// Scoped to one document for the same reason [`crate::timeline::
/// confirm_letter_date`] is: a run may hold several letters (#330), and
/// two letters from one sender repeat each other almost word for word.
/// Matching on text alone would let a bad photograph of the chaser mark
/// a deadline read cleanly off the original.
pub fn mark_disputed(
    obligations: Vec<Obligation>,
    document: usize,
    disagreements: &[crate::ocr::Disagreement],
) -> Vec<Obligation> {
    obligations
        .into_iter()
        .map(|mut obligation| {
            let landed: Vec<_> = disagreements
                .iter()
                .filter(|dispute| {
                    obligation.evidence.iter().any(|passage| {
                        passage.document == document
                            && passage.page == dispute.page
                            && dispute.lands_in(&passage.text)
                    })
                })
                .cloned()
                .collect();
            // Appended, not assigned: an obligation merged from two
            // documents (#330) is marked by each of them in turn, and
            // the second call must not erase the first's finding.
            obligation.disputed.extend(landed);
            obligation
        })
        .collect()
}

/// The Extraction typology's answers (#240): what the document obliges,
/// with every obligation tracing to the segment that says so.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractionOutcome {
    pub obligations: Vec<Obligation>,
    /// Documents whose two readings disagreed about the letter's own
    /// date (#412), by input position.
    ///
    /// On the outcome rather than beside the segments because it is not
    /// a fact about a passage — it is a question for a person, and the
    /// only date every relative deadline in the letter is counted from.
    /// Empty for anything that was not a photograph.
    pub date_disputes: Vec<(usize, crate::timeline::DateDispute)>,
}

/// The Audit typology's answers (#29 turns these into the report).
#[derive(Debug, Default)]
pub struct AuditOutcome {
    pub findings: Vec<Finding>,
    /// Money coming in regularly: wages, a pension, rent received
    /// (#79). Separate from `findings` because it is not spending and
    /// must never be totalled as if it were.
    pub income: Vec<Finding>,
    /// Classified merchants with no recurring series (#29 turns these
    /// into the report's "regular spending"). Only ever merchants the
    /// person paid — see `run_pack`.
    pub other: Vec<Spend>,
}

#[derive(Debug)]
pub enum RunError {
    /// An input file couldn't be read at all — file-level, not row-level.
    Parse(ParseError),
    /// The model was unreachable or the server refused; the run can't
    /// honestly continue (batch-level problems land in needs-review
    /// instead and never surface here).
    Step(StepError),
    /// The person asked to stop. Not an error in any report — just stop.
    Cancelled,
    /// The pack asks for a pipeline piece this version doesn't have.
    UnsupportedStep(String),
    /// The files given don't satisfy the roles the pack declares (#332).
    InputBinding(InputBindingError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Parse(e) => write!(f, "{e}"),
            RunError::Step(e) => write!(f, "{e}"),
            RunError::Cancelled => write!(f, "stopped at your request"),
            RunError::UnsupportedStep(step) => {
                write!(f, "this pack needs a newer version of Kettle ({step})")
            }
            RunError::InputBinding(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// One merchant group: every transaction that belongs to one real-world
/// merchant, however messily it appeared on the statement, plus the
/// model's answers about it as steps complete. The grouping itself
/// lives in `cleanup` so the tests can run the pipeline's own (#261).
struct Group {
    /// Deterministically cleaned representative (what the model is asked
    /// about).
    cleaned: String,
    /// First raw string seen — kept so review items and findings can
    /// point back at the statement.
    raw_first: String,
    txns: Vec<Transaction>,
    /// The model's answers, filled in as steps complete.
    name: Option<String>,
    classification: Option<serde_json::Value>,
}

impl From<MerchantGroup> for Group {
    fn from(merchant: MerchantGroup) -> Self {
        Group {
            cleaned: merchant.cleaned,
            raw_first: merchant.raw_first,
            txns: merchant.txns,
            name: None,
            classification: None,
        }
    }
}

fn evidence(txns: &[Transaction]) -> Vec<Evidence> {
    txns.iter()
        .map(|t| Evidence {
            date: t.date,
            amount: t.amount,
        })
        .collect()
}

/// Why a batch item needs review, said plainly. British English, no
/// pipeline jargon — these reach people (CLAUDE.md).
///
/// `muddled_between` is the one typology-owned noun in this copy
/// (#403): the audit's answers get muddled between merchants, a
/// document step's between passages. One function serving every pack
/// wrote "merchants" into a letter report, where the word refers to
/// nothing on the page — so the caller, which knows what its step
/// works over, supplies the noun. Every other sentence here is
/// typology-neutral.
fn plain_reason(reason: &ReviewReason, muddled_between: &str) -> String {
    match reason {
        ReviewReason::BatchFailedTwice { .. } => {
            "Kettle asked about this twice but couldn't get a clear answer, so it needs your eyes."
                .to_owned()
        }
        ReviewReason::MissingFromResults => {
            "Kettle didn't get an answer for this one, so it needs your eyes.".to_owned()
        }
        ReviewReason::MismatchedEcho { .. } => format!(
            "Kettle's answers got muddled between {muddled_between} here, so it needs your eyes."
        ),
        ReviewReason::LowConfidence => {
            "Kettle wasn't sure about this one — check it yourself.".to_owned()
        }
        ReviewReason::Truncated => {
            "Kettle's answer about this one kept getting cut short, so it needs your eyes."
                .to_owned()
        }
    }
}

/// Execute `pack` against `inputs`, reporting progress and honouring
/// cancellation between pieces of work. Batch-level problems become
/// review items; only unreachable-model, unreadable-file and
/// cancellation stop a run.
/// Where a run's model answers come from (#73).
///
/// A parameter rather than an implication, so a caller cannot ask for
/// the deterministic floor and accidentally measure a model, or the
/// other way round — the two produce numbers that mean different
/// things, and the type is where that difference is made unmissable.
pub enum Answers {
    /// Ask the model serving at this endpoint — a real run.
    FromModel(Endpoint),
    /// No model at all: every model step is answered deterministically,
    /// and the answers are honest about what no model can know (#73).
    /// This is the floor a tier must beat — a model that cannot clearly
    /// better it adds latency, download size and non-determinism for
    /// nothing.
    ///
    /// What each step does in this mode:
    ///
    /// - **normalise** (model role 0): every asked group is answered
    ///   with its own `cleaned` string as the name — pass-through.
    ///   Parsing, grouping and cleaning are real work the pipeline does
    ///   without a model, and whatever score the cleaned strings earn
    ///   against `expected.json` is the deterministic pipeline's honest
    ///   credit.
    /// - **classify** (model role 1+): no answer at all. A pipeline
    ///   with no model has not decided that anything is a subscription,
    ///   so every asked group is routed to needs-review with a plain
    ///   sentence saying a person must sort it. `classification` stays
    ///   `None`, nothing reaches the recurring set, and the classify
    ///   score is 0 of everything expected — measured by running, not
    ///   skipped. Deliberately **not** a neutral placeholder
    ///   classification: any `kind` we invented would either break the
    ///   schema's enum or masquerade as a decision nobody made.
    /// - the **optional prose summary** is already skipped when a step
    ///   has no schema; nothing changes.
    ///
    /// Everything deterministic — preprocess, grouping, aggregation,
    /// income detection — runs exactly as it always does, because this
    /// mode must be the same pipeline minus the model, not a
    /// reimplementation of it. A floor produced by different code is
    /// not a floor.
    WithoutModel,
}

/// Native resources used by deterministic preprocessing. Keeping this
/// explicit prevents the runner from guessing a source-checkout path;
/// the CLI/app can point it at development or installed resources.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunResources<'a> {
    pub pdfium_dir: Option<&'a Path>,
}

impl Answers {
    /// Drain the model measurements accumulated since the previous
    /// boundary. The no-model floor has no model cost by construction.
    pub fn take_model_metrics(&self) -> ModelMetrics {
        match self {
            Self::FromModel(endpoint) => endpoint.take_metrics(),
            Self::WithoutModel => ModelMetrics::default(),
        }
    }
}

pub fn run_pack(
    pack: &Pack,
    inputs: &[PathBuf],
    answers: &Answers,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
) -> Result<RunOutcome, RunError> {
    run_pack_with_resources(
        pack,
        inputs,
        answers,
        RunResources::default(),
        cancel,
        progress,
        log,
    )
}

/// Run a pack whose files are each named with the role they are being
/// supplied as (#332). Roles bind by name, never by position.
pub fn run_pack_bound(
    pack: &Pack,
    inputs: &[(&str, PathBuf)],
    answers: &Answers,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
) -> Result<RunOutcome, RunError> {
    run_pack_bound_with_resources(
        pack,
        inputs,
        answers,
        RunResources::default(),
        cancel,
        progress,
        log,
    )
}

pub fn run_pack_bound_with_resources(
    pack: &Pack,
    inputs: &[(&str, PathBuf)],
    answers: &Answers,
    resources: RunResources<'_>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
) -> Result<RunOutcome, RunError> {
    check_bindings(pack, inputs).map_err(RunError::InputBinding)?;
    let paths: Vec<PathBuf> = inputs.iter().map(|(_, path)| path.clone()).collect();
    let bound = bound_inputs(inputs);
    run_bound(
        pack, &paths, bound, answers, resources, cancel, progress, log,
    )
}

/// Bind a flat list of files to the single role a pack declares.
///
/// A pack declaring one role has exactly one unambiguous binding, which
/// is why every existing caller may keep passing paths. A pack
/// declaring two has none, and is refused rather than bound by the
/// order the files happened to arrive in.
fn bind_to_sole_role<'a>(
    pack: &'a Pack,
    inputs: &[PathBuf],
) -> Result<Vec<(&'a str, PathBuf)>, InputBindingError> {
    match pack.manifest.inputs.as_slice() {
        [only] => Ok(inputs
            .iter()
            .map(|path| (only.role.as_str(), path.clone()))
            .collect()),
        declared => Err(InputBindingError::RoleUnstated {
            declared: declared.iter().map(|input| input.role.clone()).collect(),
        }),
    }
}

/// Every way a set of role-named files can fail to satisfy a manifest.
///
/// Undeclared roles are checked first: a caller naming a role the pack
/// has never heard of is confused about which pack it is running, and
/// every count below it would be answering the wrong question.
fn check_bindings(pack: &Pack, inputs: &[(&str, PathBuf)]) -> Result<(), InputBindingError> {
    for (role, _) in inputs {
        if !pack
            .manifest
            .inputs
            .iter()
            .any(|declared| declared.role == *role)
        {
            return Err(InputBindingError::UndeclaredRole {
                role: (*role).to_owned(),
            });
        }
    }
    for declared in &pack.manifest.inputs {
        let given: Vec<&PathBuf> = inputs
            .iter()
            .filter(|(role, _)| *role == declared.role)
            .map(|(_, path)| path)
            .collect();
        if given.is_empty() {
            return Err(InputBindingError::MissingRole {
                role: declared.role.clone(),
            });
        }
        if let Some(error) = count_error(declared, given.len()) {
            return Err(error);
        }
        for path in given {
            if let Some(error) = type_error(declared, path) {
                return Err(error);
            }
        }
    }
    Ok(())
}

/// Does this role have a number of files it can work with (#334 §1)?
///
/// `None` when the count is satisfied. Otherwise the error that says
/// which way it missed, in the declaration's own words — `TooFew` and
/// `TooMany` are different sentences because they need different things
/// from a person.
///
/// `MissingRole` is handled by the caller and never returned here: zero
/// files is "you have not given me this document yet", which is a
/// different thing from a count being wrong.
fn count_error(declared: &InputSpec, given: usize) -> Option<InputBindingError> {
    if declared.count.permits(given) {
        return None;
    }
    let role = declared.role.clone();
    let takes = declared.count.in_words();
    let short = match declared.count {
        crate::packs::Count::Exactly(wanted) => given < wanted,
        crate::packs::Count::Between { min, .. } => min.is_some_and(|min| given < min),
    };
    Some(if short {
        InputBindingError::TooFew { role, given, takes }
    } else {
        InputBindingError::TooMany { role, given, takes }
    })
}

/// Is this file a type the role accepts (#334 §2)?
///
/// `None` when it is. Otherwise `WrongType`, carrying the accepted list
/// so the message can say what would have worked.
///
/// A file whose type cannot be told from its name is refused: this
/// check exists so a pack declaring `["application/pdf"]` is never
/// silently handed a `.txt`, and "I could not tell" is not evidence
/// that it was a PDF. `document::media_type` is the one place that
/// decides, so a file accepted here is one `read_document` can read.
fn type_error(declared: &InputSpec, path: &Path) -> Option<InputBindingError> {
    let accepted = crate::document::media_type(path).is_some_and(|media| {
        declared
            .accept
            .iter()
            .any(|declared| declared.eq_ignore_ascii_case(media))
    });
    if accepted {
        return None;
    }
    Some(InputBindingError::WrongType {
        role: declared.role.clone(),
        // File name only — never a full path, which would put the
        // person's home directory in a message they might share.
        file: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        accepted: declared.accept.clone(),
    })
}

fn bound_inputs(inputs: &[(&str, PathBuf)]) -> Vec<BoundInput> {
    inputs
        .iter()
        .map(|(role, path)| BoundInput {
            role: (*role).to_owned(),
            // File name only — never a full path (`InputInfo`).
            file: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
        .collect()
}

pub fn run_pack_with_resources(
    pack: &Pack,
    inputs: &[PathBuf],
    answers: &Answers,
    resources: RunResources<'_>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
) -> Result<RunOutcome, RunError> {
    let bound = bind_to_sole_role(pack, inputs).map_err(RunError::InputBinding)?;
    let described = bound_inputs(&bound);
    run_bound(
        pack, inputs, described, answers, resources, cancel, progress, log,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_bound(
    pack: &Pack,
    inputs: &[PathBuf],
    bound: Vec<BoundInput>,
    answers: &Answers,
    resources: RunResources<'_>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
) -> Result<RunOutcome, RunError> {
    let check_cancel = |cancel: &AtomicBool| {
        if cancel.load(Ordering::Relaxed) {
            Err(RunError::Cancelled)
        } else {
            Ok(())
        }
    };

    let mut txns: Vec<Transaction> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut needs_review: Vec<ReviewItem> = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    let mut other: Vec<Spend> = Vec::new();
    let mut income: Vec<Finding> = Vec::new();
    // The letter typology's subjects and answers (#240). Which payload
    // the run produces follows from which preprocess ran, so a pack
    // cannot end up half one typology and half the other.
    let mut segments: Vec<crate::document::Segment> = Vec::new();
    let mut obligations: Vec<Obligation> = Vec::new();
    // Where two readings of a photographed document did not agree about
    // its date (#412), paired with the document they came from.
    let mut date_disputes: Vec<(usize, crate::timeline::DateDispute)> = Vec::new();
    // Disputed lines, by the document they were read from (#412 step
    // 6). Applied to the obligations once the model has produced them.
    let mut disagreements: Vec<(usize, Vec<crate::ocr::Disagreement>)> = Vec::new();
    let mut document_run = false;
    // The comparison typology's subjects and answers (#350). Set by the
    // `policy-terms` step rather than by the preprocess, because both
    // typologies read documents — what differs is the question asked.
    let mut terms: Vec<crate::terms::Term> = Vec::new();
    let mut diff: Vec<crate::terms::TermDiff> = Vec::new();
    let mut not_compared: Vec<crate::terms::NotCompared> = Vec::new();
    let mut comparison_run = false;
    let mut claim_ledger = crate::claim_trace::ClaimLedger::new(pack.manifest.id.clone());

    for step in &pack.manifest.pipeline {
        check_cancel(cancel)?;
        match step {
            PipelineStep::Preprocess { implementation }
                if implementation == "builtin:document-text" =>
            {
                document_run = true;
                // This preprocess serves both the letter typology and
                // the comparison one, and which is running is not
                // settled until the model step names its role — so the
                // label says what is true of both (#373). "Reading your
                // letter" was a comparison run telling somebody it was
                // doing something it was not.
                let label = step_label(step, inputs.len()).expect("document-text has a label");
                let mut logical_document = 0;
                let mut files_read = 0;
                let mut grouped_page_roles: Vec<&str> = Vec::new();
                for (given, input) in bound.iter().zip(inputs) {
                    let declared = pack
                        .manifest
                        .inputs
                        .iter()
                        .find(|declared| declared.role == given.role)
                        .expect("binding was checked before preprocessing");
                    let group: Vec<&Path> = match declared.file_semantics {
                        FileSemantics::Documents => vec![input.as_path()],
                        FileSemantics::Pages => {
                            if grouped_page_roles.contains(&given.role.as_str()) {
                                continue;
                            }
                            grouped_page_roles.push(&given.role);
                            bound
                                .iter()
                                .zip(inputs)
                                .filter(|(candidate, _)| candidate.role == given.role)
                                .map(|(_, path)| path.as_path())
                                .collect()
                        }
                    };
                    for _ in &group {
                        files_read += 1;
                        progress(Progress {
                            step: label,
                            current: files_read,
                            total: inputs.len(),
                        });
                    }
                    let read = crate::document::read_document_parts(
                        &group,
                        logical_document,
                        resources.pdfium_dir,
                    )
                    .map_err(RunError::Parse)?;
                    // Per document, because "the date of this letter" is
                    // a different date in each (#330) — so a dispute
                    // about one letter's date must not be reported
                    // against another's.
                    if let Some(dispute) = read.date_dispute {
                        date_disputes.push((logical_document, dispute));
                    }
                    // Kept with the document they came from: the model
                    // has not run yet, so there are no obligations to
                    // mark until it has (#412 step 6).
                    if !read.disagreements.is_empty() {
                        disagreements.push((logical_document, read.disagreements));
                    }
                    segments.extend(read.segments);
                    logical_document += 1;
                }
            }
            PipelineStep::Preprocess { implementation } => {
                if implementation != "builtin:statement-parse" {
                    return Err(RunError::UnsupportedStep(implementation.clone()));
                }
                for (index, input) in inputs.iter().enumerate() {
                    progress(Progress {
                        step: step_label(step, inputs.len()).expect("statement-parse has a label"),
                        current: index + 1,
                        total: inputs.len(),
                    });
                    let parsed =
                        parse_input_file(input, resources.pdfium_dir).map_err(RunError::Parse)?;
                    txns.extend(parsed.transactions);
                    warnings.extend(parsed.warnings);
                }
                groups = group_transactions(&txns);
            }
            PipelineStep::Model {
                prompt,
                role,
                schema: Some(schema),
                examples,
                ..
            } => {
                // What this step means is what it says it means (#120).
                // `load_pack` has already refused any role not in the
                // supported set, so this cannot be the place a bad pack
                // is discovered — but it is still the place that would
                // have to change if that ever stopped being true.
                let model_role = ModelRole::declared(role.as_deref())
                    .ok_or_else(|| RunError::UnsupportedStep(unnamed_role(role.as_deref())))?;

                // Obligations asks about document segments, not
                // merchant groups — its own path below, sharing the
                // same batch, validation, retry and review machinery.
                let group_role = match model_role {
                    ModelRole::Obligations => {
                        run_obligations_step(
                            pack,
                            step,
                            &segments,
                            answers,
                            cancel,
                            progress,
                            log,
                            &mut claim_ledger,
                            &mut obligations,
                            &mut needs_review,
                        )?;
                        continue;
                    }
                    ModelRole::PolicyTerms => {
                        comparison_run = true;
                        run_terms_step(
                            pack,
                            step,
                            &segments,
                            answers,
                            cancel,
                            progress,
                            log,
                            &mut claim_ledger,
                            &mut terms,
                            &mut needs_review,
                        )?;
                        continue;
                    }
                    ModelRole::Normalise => GroupRole::Normalise,
                    ModelRole::Classify => GroupRole::Classify,
                };

                let label = step_label(step, inputs.len()).expect("group roles have labels");
                let echo_field = match group_role {
                    GroupRole::Normalise => "raw",
                    GroupRole::Classify => "name",
                };

                // Items this step asks about: cleaned representatives
                // for normalise; the normalised names for classify.
                // Groups already in review are nobody's business here.
                let asked: Vec<usize> = (0..groups.len())
                    .filter(|&g| match group_role {
                        GroupRole::Normalise => true,
                        GroupRole::Classify => groups[g].name.is_some(),
                    })
                    .collect();
                let items: Vec<String> = asked
                    .iter()
                    .map(|&g| match group_role {
                        GroupRole::Normalise => groups[g].cleaned.clone(),
                        GroupRole::Classify => groups[g].name.clone().expect("filtered above"),
                    })
                    .collect();

                // Without a model there is no prompt to render, no
                // batch to send and no schema to enforce — the step is
                // answered here and the loop below never runs.
                let Answers::FromModel(endpoint) = answers else {
                    match group_role {
                        // Normalise: the cleaned string is the answer.
                        GroupRole::Normalise => {
                            for (&group, item) in asked.iter().zip(&items) {
                                apply_answer(
                                    &mut groups[group],
                                    group_role,
                                    serde_json::json!({ "name": item }),
                                );
                            }
                        }
                        // Classify: "unknown" is exactly what no model
                        // knows (#253). The deterministic pipeline still
                        // finds every series and derives its kind; only
                        // the label is missing, and forced-low
                        // confidence puts each finding in front of a
                        // person. This is what makes the floor a floor:
                        // the same pipeline, minus the labels a model
                        // would add.
                        GroupRole::Classify => {
                            for &group in &asked {
                                apply_answer(
                                    &mut groups[group],
                                    group_role,
                                    serde_json::json!({
                                        "category": "unknown",
                                        "confidence": "low"
                                    }),
                                );
                            }
                        }
                    }
                    continue;
                };

                let template = read_pack_file(pack, prompt)?;
                let schema_json: serde_json::Value =
                    serde_json::from_str(&read_pack_file(pack, schema)?)
                        .expect("schema validity is checked at pack load");
                let example_text = match examples {
                    Some(examples) => Some(read_pack_file(pack, examples)?),
                    None => None,
                };

                let mut batches = step_batches(step, &items).unwrap_or_else(|| {
                    vec![items
                        .iter()
                        .enumerate()
                        .map(|(id, raw)| BatchItem::new(id, raw.as_str()))
                        .collect()]
                });
                // Batch ids are transient model-call bookkeeping. Keep
                // the stable raw statement merchant beside each one,
                // out of the serialised prompt, so eval exchanges can
                // be joined back to authored item ids after batching,
                // splitting or retrying (#237).
                for batch in &mut batches {
                    for item in batch {
                        if let Some(&group) = asked.get(item.id) {
                            item.source = Some(groups[group].raw_first.clone());
                        }
                    }
                }
                let total = batches.len();
                for (index, batch) in batches.into_iter().enumerate() {
                    check_cancel(cancel)?;
                    progress(Progress {
                        step: label,
                        current: index + 1,
                        total,
                    });
                    let outcome = run_batch(
                        endpoint,
                        &template,
                        example_text.as_deref(),
                        &schema_json,
                        &batch,
                        echo_field,
                        &BatchContext {
                            log,
                            step: label,
                            batch: index + 1,
                            cancel,
                        },
                    )
                    .map_err(|error| match error {
                        // A cancel that landed while the call was in
                        // flight is the person stopping the run, not a
                        // step going wrong.
                        StepError::Call(ModelCallError::Cancelled) => RunError::Cancelled,
                        other => RunError::Step(other),
                    })?;

                    let crate::exec::StepOutcome {
                        answers,
                        needs_review: reviews,
                        mut attempts,
                        rejected,
                    } = outcome;
                    trace_rejected_candidates(&mut claim_ledger, label, index + 1, rejected);
                    for (id, answer) in answers {
                        let source = &groups[asked[id]].raw_first;
                        let trace_id = claim_ledger.push(
                            None,
                            label,
                            index + 1,
                            id,
                            0,
                            group_claim_kind(group_role),
                            source,
                            answer.clone(),
                            vec![
                                crate::claim_trace::check(
                                    crate::claim_trace::Guardrail::Schema,
                                    crate::claim_trace::CheckOutcome::Passed,
                                ),
                                crate::claim_trace::check(
                                    crate::claim_trace::Guardrail::Pairing,
                                    crate::claim_trace::CheckOutcome::Passed,
                                ),
                            ],
                            crate::claim_trace::TerminalDisposition::Accepted,
                        );
                        claim_ledger
                            .attach_attempts(&trace_id, attempts.remove(&id).unwrap_or_default());
                        apply_answer(&mut groups[asked[id]], group_role, answer);
                    }
                    for review in reviews {
                        let id = review.item.id;
                        let source = &groups[asked[id]].raw_first;
                        let terminal = if matches!(review.reason, ReviewReason::MissingFromResults)
                            && review.answer.is_none()
                        {
                            crate::claim_trace::TerminalDisposition::AbsentAfterRetry
                        } else {
                            crate::claim_trace::TerminalDisposition::NeedsReview
                        };
                        let (schema, pairing) = review_check_outcomes(&review.reason);
                        let trace_id = claim_ledger.push(
                            None,
                            label,
                            index + 1,
                            id,
                            0,
                            group_claim_kind(group_role),
                            source,
                            review.answer.clone().unwrap_or(serde_json::Value::Null),
                            vec![
                                crate::claim_trace::check(
                                    crate::claim_trace::Guardrail::Schema,
                                    schema,
                                ),
                                crate::claim_trace::check(
                                    crate::claim_trace::Guardrail::Pairing,
                                    pairing,
                                ),
                                crate::claim_trace::check(
                                    crate::claim_trace::Guardrail::ReviewRouting,
                                    crate::claim_trace::CheckOutcome::Passed,
                                ),
                            ],
                            terminal,
                        );
                        claim_ledger
                            .attach_attempts(&trace_id, attempts.remove(&id).unwrap_or_default());
                        route_review(
                            &mut groups[asked[id]],
                            group_role,
                            review,
                            &mut needs_review,
                        );
                    }
                }
            }
            PipelineStep::Model { schema: None, .. } => {
                // The optional prose summary. The deterministic fallback
                // (#33) makes skipping it honest.
            }
            PipelineStep::Aggregate { implementation }
                if implementation == "builtin:timeline-sort" =>
            {
                // #241: deadlines resolved in Rust against the letter's
                // own date, duplicates merged, soonest first. The model
                // read the phrases; the arithmetic is never its to do.
                progress(Progress {
                    step: step_label(step, inputs.len())
                        .expect("aggregate and render steps have labels"),
                    current: 1,
                    total: 1,
                });
                obligations =
                    crate::timeline::sort_timeline(std::mem::take(&mut obligations), &segments);
                // After the sort, not before: it merges duplicates
                // (#330) and a merged obligation cites the passages of
                // every document it was found in. Marking first would
                // hand the merge two half-marked copies to reconcile;
                // marking last asks each document about the evidence
                // that survived.
                for (document, disputes) in &disagreements {
                    obligations =
                        mark_disputed(std::mem::take(&mut obligations), *document, disputes);
                }
            }
            PipelineStep::Aggregate { implementation } if implementation == "builtin:term-diff" => {
                // #350: pair the two documents' named values on
                // `(term, basis)` and say what moved. Which document is
                // the earlier one is the manifest's declared input
                // order, resolved through the bound roles — never the
                // order the files happened to arrive in, because
                // getting it the wrong way round does not fail, it
                // reports a price cut where there was a rise.
                progress(Progress {
                    step: step_label(step, inputs.len())
                        .expect("aggregate and render steps have labels"),
                    current: 1,
                    total: 1,
                });
                let position = |role: &str| bound.iter().position(|input| input.role == role);
                let declared: Vec<&str> = pack
                    .manifest
                    .inputs
                    .iter()
                    .map(|input| input.role.as_str())
                    .collect();
                // `load_pack` refuses a term-diff pack declaring fewer
                // than two inputs, and `check_bindings` refuses a run
                // missing one — so a role with no file here would be a
                // runner bug, and the honest answer is no diff rather
                // than a diff against document 0 by default.
                if let (Some(&earlier), Some(&later)) = (declared.first(), declared.get(1)) {
                    if let (Some(before), Some(after)) = (position(earlier), position(later)) {
                        let decided = crate::terms::diff_terms(
                            &terms,
                            before,
                            after,
                            &pack.manifest.term_families,
                        );
                        // Deliberately *not* pushed into `needs_review`
                        // (#377). That list means "the model could not
                        // answer for this passage"; this means "Rust
                        // declined to compare what the model read
                        // perfectly well", and they are different facts
                        // about a run.
                        //
                        // Conflating them would also corrupt the bed:
                        // `comparison_items` scores a passage found in
                        // `needs_review` as surfaced rather than read,
                        // so a model that invented a second `premium`
                        // would trigger the refusal and have its own
                        // invention scored as a referral. Inventing
                        // more would improve the score.
                        not_compared = decided.not_compared;
                        diff = decided.rows;
                    }
                }
            }
            PipelineStep::Aggregate { implementation } => {
                if implementation != "builtin:recurrence-detect" {
                    return Err(RunError::UnsupportedStep(implementation.clone()));
                }
                progress(Progress {
                    step: step_label(step, inputs.len())
                        .expect("aggregate and render steps have labels"),
                    current: 1,
                    total: 1,
                });
                for group in &groups {
                    let (Some(name), Some(classification)) = (&group.name, &group.classification)
                    else {
                        continue; // already routed to review
                    };
                    let series = detect_recurring(name, &group.txns);
                    // What this merchant *is* — the model's answer, or
                    // its honest absence. What the payments *do* is not
                    // the model's to say (#253): kind derives below,
                    // from cadence and the pack's category→kind policy.
                    let category = text(classification, "category");
                    let confidence = text(classification, "confidence");
                    let recurring_kind =
                        || crate::kinds::recurring_kind(&pack.manifest.kinds, &category);
                    let debit_days = || {
                        group
                            .txns
                            .iter()
                            .filter(|t| t.direction == crate::parse::Direction::Debit)
                            .map(|t| t.date)
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                    };
                    for earning in detect_income(name, &group.txns) {
                        income.push(Finding {
                            merchant: earning.merchant,
                            raw_merchant: group.raw_first.clone(),
                            kind: "income".to_owned(),
                            kind_from: crate::kinds::KindFrom::Income,
                            category: category.clone(),
                            confidence: confidence.clone(),
                            period: earning.period,
                            current_amount: earning.current_amount,
                            price_rise: earning.price_rise,
                            evidence: evidence(&group.txns),
                        });
                    }

                    // Regular *spending* means money that went out. A
                    // group with no debits at all can never belong
                    // here, whatever else we failed to make of it —
                    // that is what put a salary in the spending column
                    // (#79), and the guard is deliberately about the
                    // transactions rather than about income, so a
                    // direction we haven't thought of fails safe.
                    let paid_out = group
                        .txns
                        .iter()
                        .any(|t| t.direction == crate::parse::Direction::Debit);
                    if series.is_empty() && paid_out {
                        // #271: the confidence on a `kind` has to be about
                        // the decision that produced that kind. Here the
                        // kind comes from cadence — and cadence declined —
                        // while `confidence` is the model's answer about
                        // *category*, a question nobody asked about
                        // recurrence. Carrying it across is how a
                        // subscription the model correctly called
                        // "streaming / high" became regular spending at
                        // high confidence: asserted, never shown, and so
                        // never correctable. 91% of the 7B run's
                        // confident-wrong cell arrived exactly here.
                        //
                        // Only when the payments look periodic, though. A
                        // coffee habit genuinely is not recurring, and
                        // declining it is certain rather than uncertain —
                        // surfacing those too would trade one dishonest
                        // number for a review list nobody can use.
                        let periodic = looks_periodic(&group.txns);
                        let confidence = if periodic {
                            LOW_CONFIDENCE.to_owned()
                        } else {
                            confidence.clone()
                        };
                        other.push(Spend {
                            merchant: name.clone(),
                            raw_merchant: group.raw_first.clone(),
                            kind: crate::kinds::spend_kind(debit_days()).to_owned(),
                            kind_from: if periodic {
                                crate::kinds::KindFrom::CadenceDespitePeriodic
                            } else {
                                crate::kinds::KindFrom::Cadence
                            },
                            category: category.clone(),
                            confidence,
                            transactions: evidence(&group.txns),
                        });
                    }
                    for recurring in series {
                        findings.push(Finding {
                            merchant: recurring.merchant,
                            raw_merchant: group.raw_first.clone(),
                            kind: recurring_kind(),
                            kind_from: crate::kinds::KindFrom::CategoryMap,
                            category: category.clone(),
                            confidence: confidence.clone(),
                            period: recurring.period,
                            current_amount: recurring.current_amount,
                            price_rise: recurring.price_rise,
                            evidence: evidence(&group.txns),
                        });
                    }
                }
            }
            PipelineStep::Render { .. } => {
                // Report rendering is #32; RunOutcome is its input. The
                // heartbeat goes out now regardless: the pipeline really
                // is at this step, and the progress screen's last row
                // would otherwise sit on Waiting for ever.
                progress(Progress {
                    step: step_label(step, inputs.len())
                        .expect("aggregate and render steps have labels"),
                    current: 1,
                    total: 1,
                });
            }
        }
    }

    let dates = || txns.iter().map(|t| t.date);
    Ok(RunOutcome {
        input: InputSeen {
            // What was read, counted in the unit the typology reads:
            // payments for a statement, passages for a document.
            rows: if document_run {
                segments.len()
            } else {
                txns.len()
            },
            period: dates().min().zip(dates().max()),
        },
        inputs: bound,
        needs_review,
        warnings,
        claim_traces: claim_ledger.finish(),
        payload: if comparison_run {
            Payload::Comparison(ComparisonOutcome {
                terms,
                diff,
                not_compared,
            })
        } else if document_run {
            Payload::Extraction(ExtractionOutcome {
                obligations,
                date_disputes,
            })
        } else {
            Payload::Audit(AuditOutcome {
                findings,
                income,
                other,
            })
        },
    })
}

/// The pipeline's merchant grouping — one function, shared with the
/// tests that assert on it (#261).
fn group_transactions(txns: &[Transaction]) -> Vec<Group> {
    cleanup::group_transactions(txns)
        .into_iter()
        .map(Group::from)
        .collect()
}

fn read_pack_file(pack: &Pack, relative: &str) -> Result<String, RunError> {
    std::fs::read_to_string(pack.dir.join(relative)).map_err(|e| RunError::Parse(ParseError::Io(e)))
}

fn text(value: &serde_json::Value, field: &str) -> String {
    value[field].as_str().unwrap_or_default().to_owned()
}

/// What a schema-bearing model step is for (#120). The manifest names
/// it; this is the runner's side of that contract, and the reason the
/// set is closed: each arm is code that reads a specific answer shape
/// into a specific field. A role with no arm here is a pipeline this
/// runner cannot execute, which is why `load_pack` refuses it rather
/// than letting it arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Normalise,
    Classify,
    /// Given a passage, what does it oblige someone to do, and by when
    /// (#240)? Batched over document segments rather than merchant
    /// groups — the letter typology's one model question.
    Obligations,
    /// Which named terms does this passage state, and what does each
    /// say (#350)? The comparison typology's one model question, and
    /// deliberately as closed as the others: a term from a declared
    /// enum, a basis, a value copied verbatim and the quote it came
    /// from. Nothing here compares anything.
    PolicyTerms,
}

impl ModelRole {
    /// The declared role, or `None` if this runner has no arm for it.
    /// Kept beside `packs::MODEL_ROLES`, which is what load-time
    /// validation checks against — `role_names_match_the_runner` holds
    /// the two lists together.
    pub fn declared(role: Option<&str>) -> Option<Self> {
        match role? {
            "normalise" => Some(ModelRole::Normalise),
            "classify" => Some(ModelRole::Classify),
            "obligations" => Some(ModelRole::Obligations),
            "policy-terms" => Some(ModelRole::PolicyTerms),
            _ => None,
        }
    }
}

/// How to name a role in an error when the manifest didn't name one.
fn unnamed_role(role: Option<&str>) -> String {
    role.unwrap_or("a model step with no role").to_owned()
}

/// The roles whose subject is a merchant group. Narrowed from
/// [`ModelRole`] after obligations dispatches to its own path, so the
/// group-shaped helpers below cannot be handed a role they have no
/// arm for — the compiler holds it, not an `unreachable!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupRole {
    Normalise,
    Classify,
}

fn group_claim_kind(role: GroupRole) -> crate::claim_trace::ClaimKind {
    match role {
        GroupRole::Normalise => crate::claim_trace::ClaimKind::Normalisation,
        GroupRole::Classify => crate::claim_trace::ClaimKind::Classification,
    }
}

fn review_check_outcomes(
    reason: &ReviewReason,
) -> (
    crate::claim_trace::CheckOutcome,
    crate::claim_trace::CheckOutcome,
) {
    match reason {
        ReviewReason::BatchFailedTwice { .. } => (
            crate::claim_trace::CheckOutcome::Failed,
            crate::claim_trace::CheckOutcome::NotApplicable,
        ),
        ReviewReason::MissingFromResults | ReviewReason::Truncated => (
            crate::claim_trace::CheckOutcome::Passed,
            crate::claim_trace::CheckOutcome::NotApplicable,
        ),
        ReviewReason::MismatchedEcho { .. } => (
            crate::claim_trace::CheckOutcome::Passed,
            crate::claim_trace::CheckOutcome::Failed,
        ),
        ReviewReason::LowConfidence => (
            crate::claim_trace::CheckOutcome::Passed,
            crate::claim_trace::CheckOutcome::Passed,
        ),
    }
}

fn trace_rejected_candidates(
    ledger: &mut crate::claim_trace::ClaimLedger,
    step: &str,
    batch: usize,
    rejected: Vec<crate::exec::RejectedCandidate>,
) {
    for (index, rejected) in rejected.into_iter().enumerate() {
        ledger.push(
            None,
            step,
            batch,
            usize::MAX,
            index,
            crate::claim_trace::ClaimKind::Decision,
            "",
            rejected.candidate,
            vec![
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Schema,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::ClaimCheck {
                    guardrail: crate::claim_trace::Guardrail::Pairing,
                    outcome: crate::claim_trace::CheckOutcome::Failed,
                    detail: Some(rejected.reason),
                },
            ],
            crate::claim_trace::TerminalDisposition::Rejected,
        );
    }
}

/// A segment routed to a person carries the passage itself as the thing
/// to look at.
fn review_segment(segment: &crate::document::Segment, reason: String) -> ReviewItem {
    ReviewItem {
        subject: segment.text.clone(),
        reason,
        transactions: Vec::new(),
    }
}

/// The obligations step (#240): one closed question per document
/// segment — what does this passage oblige someone to do, and by when?
#[allow(clippy::too_many_arguments)]
fn run_obligations_step(
    pack: &Pack,
    step: &PipelineStep,
    segments: &[crate::document::Segment],
    answers: &Answers,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
    claim_ledger: &mut crate::claim_trace::ClaimLedger,
    obligations: &mut Vec<Obligation>,
    needs_review: &mut Vec<ReviewItem>,
) -> Result<(), RunError> {
    run_segment_step(
        pack,
        step,
        segments,
        answers,
        cancel,
        progress,
        log,
        claim_ledger,
        SegmentStep {
            label: step_label(step, 1).expect("obligations has a label"),
            without_model:
                "Kettle can't read letters without a model's help, so this needs your eyes.",
        },
        needs_review,
        &mut |segment, answer, review, ledger, trace| {
            let (read, routed) = segment_obligations(segment, answer, ledger, trace);
            obligations.extend(read);
            review.extend(routed);
        },
    )
}

/// The policy-terms step (#350): one closed question per document
/// segment — which named terms does this passage state, and what does
/// each say?
#[allow(clippy::too_many_arguments)]
fn run_terms_step(
    pack: &Pack,
    step: &PipelineStep,
    segments: &[crate::document::Segment],
    answers: &Answers,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
    claim_ledger: &mut crate::claim_trace::ClaimLedger,
    terms: &mut Vec<crate::terms::Term>,
    needs_review: &mut Vec<ReviewItem>,
) -> Result<(), RunError> {
    run_segment_step(
        pack,
        step,
        segments,
        answers,
        cancel,
        progress,
        log,
        claim_ledger,
        SegmentStep {
            label: step_label(step, 1).expect("policy-terms has a label"),
            without_model:
                "Kettle can't read documents without a model's help, so this needs your eyes.",
        },
        needs_review,
        &mut |segment, answer, review, ledger, trace| {
            let (read, unread) = segment_terms(
                segment,
                segments,
                answer,
                &pack.manifest.value_kinds,
                ledger,
                trace,
            );
            terms.extend(read);
            review.extend(unread);
        },
    )
}

/// What a segment-shaped model step is called, in the two places a
/// person sees it: the progress screen, and the sentence explaining why
/// a passage reached them when no model ran.
struct SegmentStep {
    label: &'static str,
    without_model: &'static str,
}

/// Where a schema-valid, correctly paired segment answer sat. Nested
/// obligation/term candidates inherit these two passed guards from the
/// parent decision rather than asserting them again by convention.
struct SegmentTrace {
    parent_id: String,
    step: String,
    batch: usize,
    item: usize,
}

type SegmentReader<'a> = dyn FnMut(
        &crate::document::Segment,
        &serde_json::Value,
        &mut Vec<ReviewItem>,
        &mut crate::claim_trace::ClaimLedger,
        &SegmentTrace,
    ) + 'a;

/// The machinery every segment-shaped model step shares: batching over
/// passages, `run_batch` (schema validation, one retry, mismatched-echo
/// detection, truncation splits), `plain_reason`, and the honest
/// no-model floor. What differs between roles is only how a validated
/// answer is read, which is `read`'s job.
///
/// Deliberately one function rather than two similar ones. A second
/// copy is where retry, cancellation and review-routing quietly stop
/// meaning the same thing for the second typology as for the first.
#[allow(clippy::too_many_arguments)]
fn run_segment_step(
    pack: &Pack,
    step: &PipelineStep,
    segments: &[crate::document::Segment],
    answers: &Answers,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    log: &dyn RunLog,
    claim_ledger: &mut crate::claim_trace::ClaimLedger,
    named: SegmentStep,
    needs_review: &mut Vec<ReviewItem>,
    read: &mut SegmentReader<'_>,
) -> Result<(), RunError> {
    let PipelineStep::Model {
        prompt,
        schema: Some(schema),
        examples,
        ..
    } = step
    else {
        return Ok(()); // schema-less steps are the optional prose path
    };

    // Without a model, nothing was read — that is the honest sentence,
    // not a placeholder answer nobody decided (#240). Every segment goes
    // to a person, so nothing in the document is silently dropped.
    let Answers::FromModel(endpoint) = answers else {
        for segment in segments {
            needs_review.push(review_segment(segment, named.without_model.to_owned()));
        }
        return Ok(());
    };

    let template = read_pack_file(pack, prompt)?;
    let schema_json: serde_json::Value = serde_json::from_str(&read_pack_file(pack, schema)?)
        .expect("schema validity is checked at pack load");
    let example_text = match examples {
        Some(examples) => Some(read_pack_file(pack, examples)?),
        None => None,
    };

    let items: Vec<String> = segments.iter().map(|s| s.text.clone()).collect();
    let mut batches = step_batches(step, &items).unwrap_or_else(|| {
        vec![items
            .iter()
            .enumerate()
            .map(|(id, raw)| BatchItem::new(id, raw.as_str()))
            .collect()]
    });
    // The stable identity beside each transient batch id, as for the
    // group roles (#237): a segment's own text is what an exchange can
    // be joined back on.
    for batch in &mut batches {
        for item in batch {
            if let Some(segment) = segments.get(item.id) {
                item.source = Some(segment.text.clone());
            }
        }
    }

    let label = named.label;
    let total = batches.len();
    for (index, batch) in batches.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        progress(Progress {
            step: label,
            current: index + 1,
            total,
        });
        let outcome = run_batch(
            endpoint,
            &template,
            example_text.as_deref(),
            &schema_json,
            &batch,
            "segment",
            &BatchContext {
                log,
                step: label,
                batch: index + 1,
                cancel,
            },
        )
        .map_err(|error| match error {
            StepError::Call(ModelCallError::Cancelled) => RunError::Cancelled,
            other => RunError::Step(other),
        })?;

        let crate::exec::StepOutcome {
            answers,
            needs_review: reviews,
            mut attempts,
            rejected,
        } = outcome;
        trace_rejected_candidates(claim_ledger, label, index + 1, rejected);
        for (id, answer) in answers {
            if let Some(segment) = segments.get(id) {
                let parent_id = claim_ledger.push(
                    None,
                    label,
                    index + 1,
                    id,
                    0,
                    crate::claim_trace::ClaimKind::Decision,
                    &segment.text,
                    answer.clone(),
                    vec![
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Schema,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Pairing,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                    ],
                    crate::claim_trace::TerminalDisposition::Accepted,
                );
                claim_ledger.attach_attempts(&parent_id, attempts.remove(&id).unwrap_or_default());
                read(
                    segment,
                    &answer,
                    needs_review,
                    claim_ledger,
                    &SegmentTrace {
                        parent_id,
                        step: label.to_owned(),
                        batch: index + 1,
                        item: id,
                    },
                );
            }
        }
        for review in reviews {
            let Some(segment) = segments.get(review.item.id) else {
                continue;
            };
            // A low-confidence reading is still a reading: keep it at
            // low confidence so it surfaces in "check these yourself",
            // exactly as a low-confidence classification stays a
            // classification.
            if let (ReviewReason::LowConfidence, Some(answer)) = (&review.reason, &review.answer) {
                let parent_id = claim_ledger.push(
                    None,
                    label,
                    index + 1,
                    review.item.id,
                    0,
                    crate::claim_trace::ClaimKind::Decision,
                    &segment.text,
                    answer.clone(),
                    vec![
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Schema,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Pairing,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::ReviewRouting,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                    ],
                    crate::claim_trace::TerminalDisposition::NeedsReview,
                );
                claim_ledger.attach_attempts(
                    &parent_id,
                    attempts.remove(&review.item.id).unwrap_or_default(),
                );
                read(
                    segment,
                    answer,
                    needs_review,
                    claim_ledger,
                    &SegmentTrace {
                        parent_id,
                        step: label.to_owned(),
                        batch: index + 1,
                        item: review.item.id,
                    },
                );
                continue;
            }
            let (schema, pairing) = review_check_outcomes(&review.reason);
            let terminal = if matches!(review.reason, ReviewReason::MissingFromResults)
                && review.answer.is_none()
            {
                crate::claim_trace::TerminalDisposition::AbsentAfterRetry
            } else {
                crate::claim_trace::TerminalDisposition::NeedsReview
            };
            let trace_id = claim_ledger.push(
                None,
                label,
                index + 1,
                review.item.id,
                0,
                crate::claim_trace::ClaimKind::Decision,
                &segment.text,
                review.answer.clone().unwrap_or(serde_json::Value::Null),
                vec![
                    crate::claim_trace::check(crate::claim_trace::Guardrail::Schema, schema),
                    crate::claim_trace::check(crate::claim_trace::Guardrail::Pairing, pairing),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::ReviewRouting,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                ],
                terminal,
            );
            claim_ledger.attach_attempts(
                &trace_id,
                attempts.remove(&review.item.id).unwrap_or_default(),
            );
            needs_review.push(review_segment(
                segment,
                plain_reason(&review.reason, "passages"),
            ));
        }
    }
    Ok(())
}

/// One segment's validated answer, read into obligations that each
/// carry the passage as evidence. The segment-level confidence lands on
/// every obligation it produced — it is the model's confidence in its
/// reading of this passage.
///
/// One thing routes an obligation to a person instead of into the
/// timeline, and it is a checkable fact rather than a judgement:
///
/// - **a passage that grants and requires nothing** — *"You may also
///   confirm in writing…"* obliges nobody, and the v14 letter run
///   reported it as a `response` action at high confidence (#406). The
///   check is [`crate::modality::grants_without_requiring`], and it
///   routes rather than drops: a permission wrongly surfaced costs a
///   person a glance, where a requirement wrongly dropped is the harm
///   with no headroom.
fn segment_obligations(
    segment: &crate::document::Segment,
    answer: &serde_json::Value,
    claim_ledger: &mut crate::claim_trace::ClaimLedger,
    trace: &SegmentTrace,
) -> (Vec<Obligation>, Vec<ReviewItem>) {
    let confidence = text(answer, "confidence");
    let granted = crate::modality::grants_without_requiring(&segment.text);
    let mut read = Vec::new();
    let mut review = Vec::new();
    for (index, obligation) in answer["obligations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        if granted {
            claim_ledger.push(
                Some(trace.parent_id.clone()),
                &trace.step,
                trace.batch,
                trace.item,
                index + 1,
                crate::claim_trace::ClaimKind::Obligation,
                &segment.text,
                obligation.clone(),
                vec![
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Schema,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Pairing,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::ReviewRouting,
                        crate::claim_trace::CheckOutcome::Failed,
                    ),
                ],
                crate::claim_trace::TerminalDisposition::NeedsReview,
            );
            // One entry per passage, not per claim: what a person is
            // asked to look at is the passage, and two claims read out
            // of one sentence are one thing to check.
            if review.is_empty() {
                review.push(ReviewItem {
                    subject: segment.text.clone(),
                    reason: "This passage offers something rather than asking for \
                             it, so Kettle has not turned it into an action. Read \
                             it and decide."
                        .to_owned(),
                    transactions: Vec::new(),
                });
            }
            continue;
        }
        claim_ledger.push(
            Some(trace.parent_id.clone()),
            &trace.step,
            trace.batch,
            trace.item,
            index + 1,
            crate::claim_trace::ClaimKind::Obligation,
            &segment.text,
            obligation.clone(),
            vec![
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Schema,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Pairing,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::ReviewRouting,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
            ],
            crate::claim_trace::TerminalDisposition::Accepted,
        );
        read.push({
            Obligation {
                kind: text(obligation, "kind"),
                party: text(obligation, "party"),
                ask: text(obligation, "ask"),
                deadline: text(obligation, "deadline"),
                anchor: text(obligation, "anchor"),
                amount: obligation["amount"]
                    .as_str()
                    .map_or_else(no_amount, str::to_owned),
                amount_from: obligation["amount_from"].as_u64().map(|id| id as usize),
                deadline_from: obligation["deadline_from"].as_u64().map(|id| id as usize),
                confidence: confidence.clone(),
                due: None,
                evidence: vec![segment.clone()],
                // Filled by `mark_disputed` once the page's two readings
                // have been compared — the model's answer knows nothing
                // about how the page was read.
                dated_by: None,
                priced_by: None,
                disputed: Vec::new(),
            }
        });
    }
    (read, review)
}

/// One segment's validated answer, read into named terms (#350).
///
/// Two things route a value to a person instead of into the diff, and
/// both are checkable facts rather than judgements:
///
/// - **`other`** — the model's honest place for a term it recognises
///   and the pack does not model. It is a routing answer, not a
///   finding: there is nothing to pair it with.
/// - **a quote that is not in the passage** — the guardrail #258
///   imposes on anything the model says about a source document. A
///   value whose quote Rust cannot find is an invention, and the point
///   of requiring the quote is that invention is checkable. It never
///   becomes a compared finding.
/// - **a value the term cannot hold** — the pack says a cover limit is
///   money, so a policy period read as one is not an answer to the
///   question that was asked (#380). The quote may be perfectly real;
///   what fails is the fit between the words and the term. Nothing
///   downstream could catch this — `Changed { delta: None }` is a
///   legitimate state — so it has to be caught where the value is read.
///
/// The segment-level confidence lands on every term it produced: it is
/// the model's confidence in its reading of this passage.
fn segment_terms(
    segment: &crate::document::Segment,
    siblings: &[crate::document::Segment],
    answer: &serde_json::Value,
    value_kinds: &std::collections::BTreeMap<String, crate::terms::ValueShape>,
    claim_ledger: &mut crate::claim_trace::ClaimLedger,
    trace: &SegmentTrace,
) -> (Vec<crate::terms::Term>, Vec<ReviewItem>) {
    let confidence = text(answer, "confidence");
    let mut read = Vec::new();
    let mut review = Vec::new();
    for (index, value) in answer["terms"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let term = crate::terms::Term {
            term: text(value, "term"),
            basis: text(value, "basis"),
            value: text(value, "value"),
            quote: text(value, "quote"),
            segment: segment.text.clone(),
            document: segment.document,
            confidence: confidence.clone(),
        };
        if term.term == crate::terms::OTHER {
            claim_ledger.push(
                Some(trace.parent_id.clone()),
                &trace.step,
                trace.batch,
                trace.item,
                index + 1,
                crate::claim_trace::ClaimKind::PolicyTerm,
                &segment.text,
                value.clone(),
                vec![
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Schema,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Pairing,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::PackCoverage,
                        crate::claim_trace::CheckOutcome::Failed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::ReviewRouting,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                ],
                crate::claim_trace::TerminalDisposition::NeedsReview,
            );
            review.push(review_segment(
                segment,
                "This says something Kettle's comparison doesn't cover, so it needs your eyes."
                    .to_owned(),
            ));
            continue;
        }
        // Two halves of one guardrail (#460): the quote must be on the
        // page, and it must contain the value it is offered as evidence
        // for — a bare label like `Excess` is verbatim in its own
        // passage and in two others, and a quote that supports three
        // values supports none of them.
        let quote_on_page = quote_is_in(&term.quote, &segment.text);
        let quote_carries_value = quote_is_in(&term.value, &term.quote);
        if !quote_on_page || !quote_carries_value {
            claim_ledger.push(
                Some(trace.parent_id.clone()),
                &trace.step,
                trace.batch,
                trace.item,
                index + 1,
                crate::claim_trace::ClaimKind::PolicyTerm,
                &segment.text,
                value.clone(),
                vec![
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Schema,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Pairing,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::Quote,
                        crate::claim_trace::CheckOutcome::Failed,
                    ),
                    crate::claim_trace::check(
                        crate::claim_trace::Guardrail::ReviewRouting,
                        crate::claim_trace::CheckOutcome::Passed,
                    ),
                ],
                crate::claim_trace::TerminalDisposition::NeedsReview,
            );
            review.push(review_segment(
                segment,
                if quote_on_page {
                    "Kettle couldn't find this value in the words it was based on, so it \
                     needs your eyes."
                        .to_owned()
                } else {
                    "Kettle couldn't find the words this was based on in your document, so it \
                     needs your eyes."
                        .to_owned()
                },
            ));
            continue;
        }
        // Rule 2 of #460: does the quote identify its passage? A quote
        // verbatim in two passages of the same document leaves a person
        // unable to say which one the claim rests on — a property of
        // the document rather than of the claim, so it warns and never
        // refuses. The count is at least one: the quote guardrail above
        // has already found it in this passage.
        let quote_identifies_passage = if siblings
            .iter()
            .filter(|sibling| sibling.document == segment.document)
            .filter(|sibling| quote_is_in(&term.quote, &sibling.text))
            .count()
            > 1
        {
            crate::claim_trace::CheckOutcome::Warned
        } else {
            crate::claim_trace::CheckOutcome::Passed
        };
        // A term with no declared shape cannot arise from a loaded pack
        // — validation refuses one — so the honest arm is to check
        // nothing rather than to invent an expectation here.
        if let Some(shape) = value_kinds.get(&term.term) {
            if !shape.holds(&term.value) {
                claim_ledger.push(
                    Some(trace.parent_id.clone()),
                    &trace.step,
                    trace.batch,
                    trace.item,
                    index + 1,
                    crate::claim_trace::ClaimKind::PolicyTerm,
                    &segment.text,
                    value.clone(),
                    vec![
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Schema,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Pairing,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::Quote,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::QuoteIdentifiesPassage,
                            quote_identifies_passage,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::ValueShape,
                            crate::claim_trace::CheckOutcome::Failed,
                        ),
                        crate::claim_trace::check(
                            crate::claim_trace::Guardrail::ReviewRouting,
                            crate::claim_trace::CheckOutcome::Passed,
                        ),
                    ],
                    crate::claim_trace::TerminalDisposition::NeedsReview,
                );
                review.push(review_segment(
                    segment,
                    format!(
                        "Kettle expected {} here and these words are something else, so it \
                         needs your eyes.",
                        shape.in_words()
                    ),
                ));
                continue;
            }
        }
        claim_ledger.push(
            Some(trace.parent_id.clone()),
            &trace.step,
            trace.batch,
            trace.item,
            index + 1,
            crate::claim_trace::ClaimKind::PolicyTerm,
            &segment.text,
            value.clone(),
            vec![
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Schema,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Pairing,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::Quote,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::QuoteIdentifiesPassage,
                    quote_identifies_passage,
                ),
                crate::claim_trace::check(
                    crate::claim_trace::Guardrail::ValueShape,
                    crate::claim_trace::CheckOutcome::Passed,
                ),
            ],
            crate::claim_trace::TerminalDisposition::Accepted,
        );
        read.push(term);
    }
    (read, review)
}

/// Is this text verbatim inside that one? Both halves of the quote
/// guardrail are this question: the quote inside the passage it claims
/// to come from, and the value inside the quote offered as its evidence
/// (#460).
///
/// Whitespace-insensitive, because a PDF's line breaks are an artefact
/// of the page rather than of the sentence, and nothing else: matching
/// loosely would defeat the purpose of asking for a quote at all.
fn quote_is_in(quote: &str, source: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    !quote.trim().is_empty() && squash(source).contains(&squash(quote))
}

/// Fold one validated answer into its group.
///
/// A classify answer of category "unknown" has its confidence forced
/// to low, whatever the model claimed (#253): "I cannot tell what this
/// is" and "high confidence" cannot both be true, and low is what
/// routes the merchant into "check these yourself".
fn apply_answer(group: &mut Group, group_role: GroupRole, answer: serde_json::Value) {
    match group_role {
        GroupRole::Normalise => group.name = Some(text(&answer, "name")),
        GroupRole::Classify => {
            let mut answer = answer;
            if answer["category"] == "unknown" {
                answer["confidence"] = serde_json::Value::from("low");
            }
            group.classification = Some(answer);
        }
    }
}

/// Route one review item: a low-confidence classification is still a
/// classification (the finding carries "low" into "check these
/// yourself"); everything else parks the merchant for a person.
fn route_review(
    group: &mut Group,
    group_role: GroupRole,
    review: NeedsReview,
    needs_review: &mut Vec<ReviewItem>,
) {
    if let (GroupRole::Classify, ReviewReason::LowConfidence, Some(answer)) =
        (group_role, &review.reason, &review.answer)
    {
        group.classification = Some(answer.clone());
        return;
    }
    needs_review.push(ReviewItem {
        subject: group.raw_first.clone(),
        reason: plain_reason(&review.reason, "merchants"),
        transactions: evidence(&group.txns),
    });
}

#[cfg(test)]
mod tests {
    use super::{plain_reason, ReviewItem};
    use crate::exec::ReviewReason;

    /// #394 renamed `ReviewItem.raw_merchant` to `subject`, and a
    /// parked `pending.json` written before the rename still has the
    /// old spelling in it. The serde alias is what keeps that run
    /// resumable; this is the test that notices it going missing.
    #[test]
    fn a_review_item_parked_before_the_rename_still_reads() {
        let old =
            r#"{"raw_merchant":"AMZN DIGITAL*8H2Q","reason":"needs your eyes","transactions":[]}"#;
        let item: ReviewItem = serde_json::from_str(old).expect("the old spelling still parses");
        assert_eq!(item.subject, "AMZN DIGITAL*8H2Q");

        let new = serde_json::to_string(&item).expect("serialises");
        assert!(
            new.contains("\"subject\""),
            "new files are written with the new name: {new}"
        );
    }

    fn every_reason() -> Vec<ReviewReason> {
        vec![
            ReviewReason::BatchFailedTwice { errors: vec![] },
            ReviewReason::MissingFromResults,
            ReviewReason::MismatchedEcho {
                echoed: "some other passage".to_owned(),
            },
            ReviewReason::LowConfidence,
            ReviewReason::Truncated,
        ]
    }

    /// #403: a letter about a phone contract has no merchants in it, so
    /// review copy that says "muddled between merchants" refers to
    /// nothing on the page. A passage-shaped step's copy must speak the
    /// document's vocabulary, whatever the reason.
    #[test]
    fn a_passage_steps_review_copy_never_speaks_the_audits_vocabulary() {
        for reason in every_reason() {
            let copy = plain_reason(&reason, "passages");
            for noun in ["merchant", "transaction", "statement"] {
                assert!(
                    !copy.contains(noun),
                    "{noun:?} is the audit's vocabulary and has no place in a \
                     letter's review copy: {copy}"
                );
            }
        }
    }

    /// The fix is per-typology copy, not vaguer copy — the audit keeps
    /// its specific noun, because there the noun is what's on the page.
    #[test]
    fn the_audits_muddled_copy_still_names_merchants() {
        let copy = plain_reason(
            &ReviewReason::MismatchedEcho {
                echoed: "APRICOT MUSIC".to_owned(),
            },
            "merchants",
        );
        assert!(
            copy.contains("muddled between merchants"),
            "the audit's reason keeps its noun: {copy}"
        );
    }
}
