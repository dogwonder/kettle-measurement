//! Pack loading: parse and validate a task pack directory (#16), and
//! refuse any pack asking for capabilities beyond `["read"]` (#17).
//!
//! A broken pack must be loud at load time, never mid-run: referenced
//! paths stay inside the pack (#77), referenced files exist, schema
//! files are valid JSON Schema inside the grammar-safe subset, prompts
//! parse, batch sizes are at least one.

use crate::eval::evidence::{EvidenceDeclaration, EvidenceDimension};
use crate::eval::{ClassificationStratum, EvalCost, EvalMetric, Gate};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The manifest compatibility version this runner implements.
///
/// A pack compares its `min_runner_version` with the crate that actually
/// loads and executes it, rather than with whichever CLI or shell happens
/// to be carrying the crate.
pub const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A loaded, validated pack: the manifest plus the directory it came
/// from, so later steps resolve `prompts/…` etc. against the right root.
#[derive(Debug)]
pub struct Pack {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// `pack.json`, worked example in brief §3.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    /// One plain-language line: what this pack promises a person, for
    /// the app's task card. Optional so older packs still load, but a
    /// pack without one has nothing to say for itself on the grid.
    #[serde(default)]
    pub description: String,
    pub version: String,
    /// The oldest runner that understands this pack's manifest and
    /// execution contract (#94).
    pub min_runner_version: String,
    pub inputs: Vec<InputSpec>,
    pub capabilities: Vec<String>,
    pub model: ModelConfig,
    /// What a run of this pack says for itself on the app's screens
    /// (#244): how long it takes, what it will do, and the words on the
    /// Run button. `Option` only so the absence can be refused with a
    /// message that says what to write, rather than a bare serde error;
    /// `load_pack` guarantees `Some`, which is what [`Manifest::copy`]
    /// encodes.
    #[serde(default)]
    copy: Option<CopyBlock>,
    pub pipeline: Vec<PipelineStep>,
    /// Per-step quality thresholds. Scores, not money — f32 is fine
    /// here. Review rate is a reported cost and has no threshold.
    #[serde(default)]
    pub eval: BTreeMap<String, f32>,
    /// How this pack's verdict is computed from its fixtures (#301).
    ///
    /// Deliberately **not** `#[serde(default)]`, unlike every other
    /// eval field here. The others default to empty, which gates
    /// nothing and is safe; a defaulted *verdict rule* gates everything
    /// and is how `letter-to-actions` came to be judged by a rule
    /// nobody chose. A pack declaring bars must say how they are read.
    #[serde(default)]
    pub eval_gate: Option<Gate>,
    /// Per-item decision shapes this pack asks the runner to score.
    /// Empty remains valid for packs with no per-item eval content.
    #[serde(default)]
    pub eval_metrics: BTreeSet<EvalMetric>,
    /// Classification strata and their provenance-bearing harm
    /// ceilings. Empty for packs without classification gates.
    #[serde(default)]
    pub eval_strata: BTreeMap<String, ClassificationStratum>,
    /// Report-only costs and why this pack records them. These are
    /// deliberately separate from `eval` so they cannot become verdict
    /// thresholds by accident.
    #[serde(default)]
    pub eval_costs: BTreeMap<EvalCost, EvalCostDeclaration>,
    /// The evidence dimensions this pack can truthfully score (#430),
    /// each with the reason it believes so. A dimension not declared
    /// here is not scored — a report says what was and was not
    /// measured rather than passing the unmeasured vacuously.
    #[serde(default)]
    pub eval_evidence: BTreeMap<EvidenceDimension, EvidenceDeclaration>,
    /// Stable scored-item ids that have been retired and may never be
    /// assigned again (#237). The live ids are authored beside their
    /// fixture expectations; this is the pack-wide tombstone registry
    /// that prevents an old baseline joining to a different item.
    #[serde(default)]
    pub eval_items: EvalItems,
    pub outputs: Vec<String>,
    /// What a *recurring* series of each category is (#253):
    /// category → "subscription" | "utility" | "regular_spend".
    ///
    /// Pack data on purpose. The runner derives a recurring merchant's
    /// kind from cadence plus this map, because "recurring housing
    /// money is a bill, recurring streaming money is a subscription"
    /// is pack policy — hard-coding it in the runner would be
    /// pack-specific runner code (#51). Required, and required
    /// complete, whenever the pipeline has a classify-role step;
    /// validated against that step's category enum below.
    #[serde(default)]
    pub kinds: BTreeMap<String, String>,
    /// What kind of value each modelled term can hold (#380):
    /// term → "money" | "percentage" | "duration" | "text", or a list
    /// where a term may honestly be written either way.
    ///
    /// Pack data for the same reason `kinds` is. "A cover limit is
    /// money" is pack policy, and a runner that knew it would be
    /// pack-specific runner code (#51). Required, and required
    /// complete, whenever the pipeline has a policy-terms step;
    /// validated against that step's term enum below.
    #[serde(default)]
    pub value_kinds: BTreeMap<String, crate::terms::ValueShape>,
    /// Which modelled terms name the same *kind* of value (#461):
    /// family name → the terms in it.
    ///
    /// Pack data for `value_kinds`' reason exactly. "A compulsory
    /// excess and a total excess are both excesses" is pack policy, and
    /// a runner that knew it would be pack-specific runner code.
    ///
    /// Read only to *refuse* a comparison when the two documents label
    /// the same kind of value differently — never to pair one term with
    /// another. Optional: a pack whose terms have no siblings to be
    /// confused with declares nothing and loses nothing.
    #[serde(default)]
    pub term_families: crate::terms::TermFamilies,
    /// Reserved Tier 3 shape from the plugin architecture (#94).
    ///
    /// This runner parses and validates it so the format cannot drift,
    /// but refuses every non-empty use until the WASM sandbox exists.
    #[serde(default)]
    pub wasm_steps: Vec<WasmStep>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalItems {
    #[serde(default)]
    pub retired: Vec<String>,
    /// The audition set (#539): fixture *file names* (unlike `retired`,
    /// which holds item ids) making up the committed go/no-go bed a
    /// candidate model runs before earning a full bed run.
    ///
    /// Declared here rather than in each fixture's `expected.json`
    /// because those bytes feed the recorded bed digests (#320) and
    /// every resume key — tagging a development fixture in-file would
    /// change the development digest and read as "the bed changed" when
    /// no question or expectation moved. Development fixtures only; a
    /// name from the sealed exam set is refused at selection.
    #[serde(default)]
    pub audition: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalCostDeclaration {
    pub reason: String,
    pub date: NaiveDate,
}

#[derive(Debug)]
pub struct InputSpec {
    pub role: String,
    /// What a person is shown when asked for this document. Required,
    /// because `role` is a binding key and not copy: a screen that has
    /// to invent the words for `previous` puts product language in the
    /// shell, out of reach of the pack author who knows what the
    /// document is called. British English, sentence case.
    pub label: String,
    pub accept: Vec<String>,
    /// How many files this role takes (#334 §1). Normalised at load
    /// from either `count` or the older `multiple`, so every reader
    /// asks one question of one field.
    pub count: Count,
}

/// How many files a role takes.
///
/// `multiple: bool` could say "one" or "any number" and nothing else.
/// #66 wants exactly one document per side, #67 wants "at least two"
/// payslips, and a statement pack wants "up to twelve" so that four
/// hundred dropped files are refused rather than read. None of those
/// are boolean states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Count {
    /// `"count": 2` — this many, no more and no fewer.
    Exactly(usize),
    /// `"count": {"min": 2, "max": 12}` — either end may be open.
    Between {
        min: Option<usize>,
        max: Option<usize>,
    },
}

impl Count {
    /// Is `given` a number of files this role accepts?
    ///
    /// The only reader of the declaration, so a wrong answer here is a
    /// run that reads the wrong documents or refuses the right ones.
    pub fn permits(&self, given: usize) -> bool {
        match self {
            Count::Exactly(wanted) => given == *wanted,
            Count::Between { min, max } => {
                min.is_none_or(|min| given >= min) && max.is_none_or(|max| given <= max)
            }
        }
    }

    /// The declaration in a person's words, for the refusal message:
    /// "one file", "two files", "at least two files", "up to twelve
    /// files", "between two and twelve files". British English, and
    /// singular where the number is one — a screen saying "1 files" is
    /// the shell talking to itself.
    pub fn in_words(&self) -> String {
        match self {
            Count::Exactly(wanted) => files(*wanted),
            Count::Between {
                min: Some(min),
                max: None,
            } => format!("at least {}", files(*min)),
            Count::Between {
                min: None,
                max: Some(max),
            } => format!("up to {}", files(*max)),
            Count::Between {
                min: Some(min),
                max: Some(max),
            } => format!("between {} and {}", number(*min), files(*max)),
            Count::Between {
                min: None,
                max: None,
            } => "any number of files".to_owned(),
        }
    }
}

/// "one file", "two files" — the noun agreeing with the number, since
/// a screen saying "1 files" is the shell talking to itself.
fn files(n: usize) -> String {
    format!("{} {}", number(n), if n == 1 { "file" } else { "files" })
}

/// Small numbers as words, as prose writes them. Above twelve the
/// digits read better than the words do, and a pack asking for
/// thirteen of anything is already unusual enough to be worth seeing
/// as a number.
fn number(n: usize) -> String {
    const WORDS: [&str; 13] = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS
        .get(n)
        .map(|word| (*word).to_owned())
        .unwrap_or_else(|| n.to_string())
}

/// The manifest spelling of [`Count`], before normalisation.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CountSpec {
    Exactly(usize),
    Between(CountRange),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountRange {
    #[serde(default)]
    min: Option<usize>,
    #[serde(default)]
    max: Option<usize>,
}

impl From<CountSpec> for Count {
    fn from(spec: CountSpec) -> Self {
        match spec {
            CountSpec::Exactly(n) => Count::Exactly(n),
            CountSpec::Between(range) => Count::Between {
                min: range.min,
                max: range.max,
            },
        }
    }
}

/// `multiple` and `count` are two ways to say the same thing, so a
/// manifest may use either and never both: two fields that can disagree
/// is one assertion nobody checks. `multiple: false` and an absent
/// declaration both mean exactly one file, which is what they meant
/// before this existed.
impl<'de> Deserialize<'de> for InputSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            role: String,
            label: String,
            accept: Vec<String>,
            #[serde(default)]
            multiple: Option<bool>,
            #[serde(default)]
            count: Option<CountSpec>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let count = match (raw.multiple, raw.count) {
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(format!(
                    "input “{}” declares both `multiple` and `count`; they say the same \
                     thing and can disagree, so a pack states one of them",
                    raw.role
                )))
            }
            (_, Some(spec)) => spec.into(),
            (Some(true), None) => Count::Between {
                min: Some(1),
                max: None,
            },
            (Some(false) | None, None) => Count::Exactly(1),
        };

        Ok(InputSpec {
            role: raw.role,
            label: raw.label,
            accept: raw.accept,
            count,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub min_tier: String,
    pub recommended_tier: String,
    pub context: u32,
    pub temperature: f32,
}

impl Manifest {
    /// The pack's copy block. `load_pack` refused any manifest without
    /// one, so a loaded pack always answers.
    pub fn copy(&self) -> &CopyBlock {
        self.copy
            .as_ref()
            .expect("load_pack refuses a pack without a copy block")
    }
}

/// What a run of this pack says for itself on the app's screens (#244).
///
/// Authored because it is prose: what a step means to a person is not
/// recoverable from a step's name, and a time estimate is a measurement
/// somebody made. Required because the missing declaration is the
/// silent case — the same argument as `value_kinds` (#380). An optional
/// copy block means the shell keeps a fallback branch alive for ever,
/// and "no pack-id branch decides what a screen says" is only true
/// once that fallback retires.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CopyBlock {
    pub time: CopyTime,
    /// "What this run will do", numbered, in the pack's own words.
    pub will: Vec<WillEntry>,
    /// The words on the Run button ("Run this audit").
    pub run_verb: String,
}

/// How long a run takes, honestly. A real figure only where a
/// measurement exists; otherwise the sentence says plainly that the
/// pack has not been timed yet — #213's lesson applied before it can
/// repeat.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CopyTime {
    pub kind: TimeKind,
    /// Tag-sized, beside the time class: "with statement size".
    pub estimate: String,
    /// The same honesty as a full sentence — never derived from
    /// `estimate` by string surgery.
    pub on_this_computer: String,
}

/// The time classes the task grid sorts by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeKind {
    Quick,
    KettleWorthy,
    Overnight,
    Varies,
}

/// One numbered line of "what this run will do".
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WillEntry {
    pub doing: String,
    pub detail: String,
    /// The progress-step labels this line covers, if it cares to say.
    /// Each named label must exist (decision 2 on #244) — prose stays
    /// free to group six steps into three sentences a person cares
    /// about, but a reference that resolves to nothing is a bug, not a
    /// style opinion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

/// One pipeline entry, tagged by its `step` field. Paths are relative to
/// the pack directory. The summary step shows why `schema` and `batch`
/// are optional on model steps: prose output has neither.
#[derive(Debug, Deserialize)]
#[serde(tag = "step", rename_all = "lowercase", deny_unknown_fields)]
pub enum PipelineStep {
    Preprocess {
        #[serde(rename = "impl")]
        implementation: String,
    },
    Model {
        prompt: String,
        /// What this step *means* (#120). Required on schema-bearing
        /// steps, because those are the ones whose answers the runner
        /// reads into named fields — and reading them by position is the
        /// bug this exists to kill. Prose steps (no schema) have no
        /// answers to read, so they need no role.
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        schema: Option<String>,
        #[serde(default)]
        batch: Option<u32>,
        #[serde(default)]
        examples: Option<String>,
        #[serde(default)]
        optional: bool,
    },
    Aggregate {
        #[serde(rename = "impl")]
        implementation: String,
    },
    Render {
        template: String,
    },
}

/// A future sandboxed deterministic step, reserved now so adding the
/// executor later is an additive manifest change (#94).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmStep {
    pub step: String,
    pub module: String,
    pub sha256: String,
    pub limits: WasmLimits,
}

/// Per-invocation bounds the future WASM runtime must enforce.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmLimits {
    pub fuel: u64,
    pub memory_mb: u64,
}

/// Why a pack refused to load. Every message is plain language — these
/// surface to people, not logs.
#[derive(Debug)]
pub enum PackError {
    Io(std::io::Error),
    /// `pack.json` didn't parse into a `Manifest`.
    Manifest(serde_json::Error),
    /// `min_runner_version` is present but is not a semantic version.
    InvalidRunnerVersion {
        version: String,
        reason: String,
    },
    /// A scoring threshold has no usable value or provenance.
    InvalidEvalDeclaration {
        stratum: String,
        reason: String,
    },
    /// The pack declares bars but not how they are read (#301).
    MissingEvalGate {
        steps: Vec<String>,
    },
    /// The pack asks for a runner newer than this one.
    NeedsNewerRunner {
        required: String,
        current: String,
    },
    /// Pack ids are globally unique reverse-DNS names, not local slugs.
    InvalidId {
        id: String,
    },
    /// A reserved WASM declaration is malformed even before execution is
    /// considered.
    InvalidWasmStep {
        step: String,
        reason: String,
    },
    /// The reserved shape is understood, but its executor does not exist.
    UnsupportedWasm {
        current: String,
    },
    /// The pack says nothing for itself on the app's screens (#244).
    MissingCopy,
    /// A `will` entry names a progress step that does not exist (#244).
    UnknownWillStep {
        named: String,
        available: Vec<String>,
    },
    /// The manifest references a file that isn't in the pack.
    MissingFile {
        path: PathBuf,
    },
    /// The manifest references a path that leaves the pack directory —
    /// absolute, or climbing out with `..`. Holds the path as written,
    /// not the resolved one: what's wrong is what the manifest says.
    EscapingPath {
        path: PathBuf,
    },
    /// A schema file exists but isn't usable JSON Schema.
    InvalidSchema {
        path: PathBuf,
        reason: String,
    },
    /// A model step's prompt template doesn't render.
    BrokenPrompt {
        path: PathBuf,
        reason: String,
    },
    /// A model step declares `batch: 0`. `exec::batch_items` clamps as a
    /// backstop, but a manifest saying 0 is a broken pack.
    ZeroBatch {
        prompt: String,
    },
    /// The pack asks for more than `["read"]`. Lists only the
    /// capabilities beyond read — the ones being refused.
    Refused {
        capabilities: Vec<String>,
    },
    /// The `kinds` map cannot answer the question the runner will ask
    /// it (#253): missing, incomplete against the classify categories,
    /// or naming a kind nothing downstream understands.
    InvalidKinds {
        reason: String,
    },
    /// The `value_kinds` map cannot answer the question the runner will
    /// ask it (#380): missing, incomplete against the term enum, or
    /// naming a kind of value nothing can check.
    InvalidValueKinds {
        reason: String,
    },
    /// A `builtin:` step this runner cannot execute (#120). The set is
    /// closed and known at load time, so there is no reason to discover
    /// it mid-run, after someone has already chosen their files.
    UnsupportedStep {
        step: String,
    },
    /// The pack diffs two documents but does not take two (#350). Holds
    /// how many it takes, because one is the case that would otherwise
    /// run: every term reported as newly added, in a report that reads
    /// like a finding rather than like the mistake it is.
    CannotCompare {
        roles: usize,
    },
    /// A compared document may be several files, so which one the
    /// comparison is against is unstated (#350). The same failure as
    /// [`InputBindingError::RoleUnstated`] one step along: it does not
    /// fail, it silently compares against whichever arrived first.
    AmbiguousComparison {
        role: String,
    },
    /// A role declares a number of files it can never be given (#334
    /// §1): a maximum below its minimum, or a count of nothing. Both
    /// are refused at load rather than at bind time, because a pack
    /// that cannot be satisfied should not reach a person's file
    /// picker at all.
    UnsatisfiableCount {
        role: String,
        reason: String,
    },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::Io(e) => write!(f, "could not open the pack: {e}"),
            PackError::Manifest(e) => write!(f, "could not read pack.json: {e}"),
            PackError::InvalidRunnerVersion { version, reason } => write!(
                f,
                "the pack's min_runner_version ({version}) is not a version Kettle understands: \
                 {reason}"
            ),
            PackError::InvalidEvalDeclaration { stratum, reason } => {
                write!(f, "the eval stratum {stratum:?} is not valid: {reason}")
            }
            PackError::MissingCopy => write!(
                f,
                "the pack doesn't say what it will do — add a \"copy\" block to pack.json, \
                 with \"time\" (how long a run takes, honestly: a measured figure or a plain \
                 \"not timed yet\"), \"will\" (what a run will do, in the pack's own words) \
                 and \"run_verb\" (the words on the Run button)"
            ),
            PackError::UnknownWillStep { named, available } => write!(
                f,
                "the pack's copy says a run will cover a step called {named:?}, but no step \
                 in this pipeline has that label — its steps are: {}",
                available.join(", ")
            ),
            PackError::MissingEvalGate { steps } => write!(
                f,
                "the pack sets bars for {} but no eval_gate saying how they are read — \
                 declare \"per_fixture\" (every fixture must clear its bar; right where a \
                 fixture carries many decisions) or \"pooled\" (the rate across every \
                 decision, read by its Wilson lower bound)",
                steps.join(", ")
            ),
            PackError::NeedsNewerRunner { required, current } => write!(
                f,
                "this pack needs a newer Kettle — it requires runner {required}, \
                 but this is runner {current}"
            ),
            PackError::InvalidId { id } => write!(
                f,
                "the pack id {id:?} is not namespaced — use a reverse-DNS id such as \
                 app.kttl.subscription-audit"
            ),
            PackError::InvalidWasmStep { step, reason } => {
                write!(f, "the reserved WASM step {step:?} is not valid: {reason}")
            }
            PackError::UnsupportedWasm { current } => write!(
                f,
                "this pack needs a newer Kettle — it contains WASM steps, which runner \
                 {current} does not execute"
            ),
            PackError::MissingFile { path } => {
                write!(f, "the pack refers to a missing file: {}", path.display())
            }
            PackError::EscapingPath { path } => write!(
                f,
                "the pack refers to a file outside itself ({}), \
                 so Kettle won't run it — packs may only use their own files",
                path.display()
            ),
            PackError::InvalidSchema { path, reason } => {
                write!(f, "not a usable schema: {} ({reason})", path.display())
            }
            PackError::BrokenPrompt { path, reason } => {
                write!(f, "the prompt {} doesn't work: {reason}", path.display())
            }
            PackError::ZeroBatch { prompt } => {
                write!(f, "the step using {prompt} asks for a batch size of 0")
            }
            PackError::InvalidKinds { reason } => {
                write!(f, "this pack's kinds list doesn't work: {reason}")
            }
            PackError::InvalidValueKinds { reason } => {
                write!(
                    f,
                    "this pack's list of what each term holds doesn't work: {reason}"
                )
            }
            PackError::UnsupportedStep { step } => write!(
                f,
                "this pack needs a step Kettle doesn't have ({step}), \
                 so it may need a newer version of Kettle"
            ),
            PackError::CannotCompare { roles } => write!(
                f,
                "this pack compares two documents but only asks you for {roles}, \
                 so there is nothing to compare it against"
            ),
            PackError::UnsatisfiableCount { role, reason } => write!(
                f,
                "the pack asks for a number of files it can never be given for “{role}”: {reason}"
            ),
            PackError::AmbiguousComparison { role } => write!(
                f,
                "this pack compares two documents but lets you give it several \
                 for “{role}”, so it can't say which one it compared"
            ),
            PackError::Refused { capabilities } => {
                write!(
                    f,
                    "this pack asks to do more than read your files ({}), so Kettle won't run it",
                    capabilities.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for PackError {}

/// The builtins this runner can execute (#120). Closed on purpose: a
/// pack naming anything else is refused at load, so a pipeline the
/// runner cannot run never reaches the point of asking for files.
/// Growing this list is how a new capability ships — deliberately a code
/// change, reviewed, rather than a manifest asserting its way in.
pub const PREPROCESS_BUILTINS: &[&str] = &["builtin:statement-parse", "builtin:document-text"];
pub const AGGREGATE_BUILTINS: &[&str] = &[
    "builtin:recurrence-detect",
    "builtin:timeline-sort",
    // #350: named values from two documents, paired on `(term, basis)`
    // and compared in `Decimal`. Named for what it does rather than for
    // insurance — #67's payslips and a tariff comparison diff the same
    // way — because a builtin per pack is a closed set in name only.
    "builtin:term-diff",
];

/// The roles a schema-bearing model step may declare (#120). Same
/// closed-set reasoning as the builtins: the runner reads each role's
/// answers into named fields it knows, so a role it does not know is a
/// pipeline it cannot execute — refused at load rather than guessed at.
pub const MODEL_ROLES: &[&str] = &["normalise", "classify", "obligations", "policy-terms"];

/// Reverse-DNS pack id: at least an owner pair plus a pack name, with
/// lowercase DNS-style labels throughout.
fn is_namespaced_id(id: &str) -> bool {
    let labels: Vec<&str> = id.split('.').collect();
    labels.len() >= 3 && labels.into_iter().all(is_id_label)
}

/// One label in a pack id or a reserved step name.
fn is_id_label(label: &str) -> bool {
    let mut chars = label.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !label.ends_with('-')
}

/// Validate the future WASM declarations fully enough to freeze their
/// manifest contract, without pretending this runner can execute them.
fn validate_wasm_steps(steps: &[WasmStep]) -> Result<(), PackError> {
    let mut names = BTreeSet::new();
    for wasm in steps {
        let invalid = |reason: &str| PackError::InvalidWasmStep {
            step: wasm.step.clone(),
            reason: reason.to_owned(),
        };

        if !is_id_label(&wasm.step) {
            return Err(invalid(
                "step must be a lowercase name using letters, digits and hyphens",
            ));
        }
        if !names.insert(&wasm.step) {
            return Err(invalid("step names must be unique"));
        }
        if !is_contained(&wasm.module) {
            return Err(PackError::EscapingPath {
                path: PathBuf::from(&wasm.module),
            });
        }
        if Path::new(&wasm.module)
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("wasm")
        {
            return Err(invalid("module must name a .wasm file inside the pack"));
        }
        if wasm.sha256.len() != 64 || !wasm.sha256.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return Err(invalid("sha256 must be exactly 64 hexadecimal digits"));
        }
        if wasm.limits.fuel == 0 {
            return Err(invalid("limits.fuel must be greater than zero"));
        }
        if wasm.limits.memory_mb == 0 {
            return Err(invalid("limits.memory_mb must be greater than zero"));
        }
    }
    Ok(())
}

/// Every file path a step references, relative to the pack directory.
fn referenced_files(step: &PipelineStep) -> Vec<&str> {
    match step {
        PipelineStep::Preprocess { .. } | PipelineStep::Aggregate { .. } => Vec::new(),
        PipelineStep::Model {
            prompt,
            schema,
            examples,
            ..
        } => std::iter::once(prompt.as_str())
            .chain(schema.as_deref())
            .chain(examples.as_deref())
            .collect(),
        PipelineStep::Render { template } => vec![template],
    }
}

/// Does this manifest path stay inside the pack directory?
///
/// Only ordinary path segments are allowed: an absolute path makes
/// `Path::join` discard the pack directory entirely, and `..` climbs out
/// of it. Judged on the path as written — symlinks pointing outward are
/// a separate question, and one for whoever installed the pack.
fn is_contained(relative: &str) -> bool {
    use std::path::Component;
    !relative.is_empty()
        && Path::new(relative)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Load and validate the pack in `dir`.
///
/// Validation order (so the most useful error surfaces first):
/// 1. read + parse `pack.json` → `Io` / `Manifest`
/// 2. capabilities beyond `["read"]` → `Refused` (#17 — before anything
///    else: a refused pack's other problems are nobody's business)
/// 3. pack id and runner compatibility → `InvalidId` /
///    `InvalidRunnerVersion` / `NeedsNewerRunner`
/// 4. validate the reserved WASM shape, then refuse its use
/// 5. every referenced path stays inside the pack (#77 — the trust
///    boundary, so before the files are looked for) → `EscapingPath`
/// 6. every referenced file exists (prompts, schemas, examples,
///    template) → `MissingFile`
/// 7. each schema file parses, builds a validator, and stays inside the
///    grammar-safe subset → `InvalidSchema`
/// 8. every model prompt parses (syntax only) → `BrokenPrompt`
/// 9. every `builtin:` step is one this runner can execute (#120) →
///    `UnsupportedStep`
/// 10. model steps with `batch: Some(0)` → `ZeroBatch`
pub fn load_pack(dir: &Path) -> Result<Pack, PackError> {
    // 1. read + parse pack.json
    let manifest_text = std::fs::read_to_string(dir.join("pack.json")).map_err(PackError::Io)?;
    let manifest: Manifest = serde_json::from_str(&manifest_text).map_err(PackError::Manifest)?;

    // 2. capabilities beyond ["read"] — refuse before looking any further.
    let beyond_read: Vec<String> = manifest
        .capabilities
        .iter()
        .filter(|capability| capability.as_str() != "read")
        .cloned()
        .collect();
    if !beyond_read.is_empty() {
        return Err(PackError::Refused {
            capabilities: beyond_read,
        });
    }

    // 3. Identity and the coarse runner compatibility gate. Execution
    // semantics remain independently validated below (#120).
    if !is_namespaced_id(&manifest.id) {
        return Err(PackError::InvalidId {
            id: manifest.id.clone(),
        });
    }
    let required = semver::Version::parse(&manifest.min_runner_version).map_err(|error| {
        PackError::InvalidRunnerVersion {
            version: manifest.min_runner_version.clone(),
            reason: error.to_string(),
        }
    })?;
    let current =
        semver::Version::parse(RUNNER_VERSION).expect("the runner crate version is valid semver");
    if required > current {
        return Err(PackError::NeedsNewerRunner {
            required: required.to_string(),
            current: current.to_string(),
        });
    }

    // Bars without a rule for reading them is the #301 defect: the
    // harness picks one, nobody reviews it, and a pack can end up gated
    // on "no errors at all, anywhere" without anyone having chosen that.
    if !manifest.eval.is_empty() && manifest.eval_gate.is_none() {
        return Err(PackError::MissingEvalGate {
            steps: manifest.eval.keys().cloned().collect(),
        });
    }

    for (stratum, declaration) in &manifest.eval_strata {
        if !is_id_label(stratum) {
            return Err(PackError::InvalidEvalDeclaration {
                stratum: stratum.clone(),
                reason: "its name must be lowercase kebab-case".to_owned(),
            });
        }
        if declaration.description.trim().is_empty() {
            return Err(PackError::InvalidEvalDeclaration {
                stratum: stratum.clone(),
                reason: "its description is empty".to_owned(),
            });
        }
        for (class, ceiling) in &declaration.classes {
            if !ceiling.max_wilson_95.is_finite() || !(0.0..=1.0).contains(&ceiling.max_wilson_95) {
                return Err(PackError::InvalidEvalDeclaration {
                    stratum: stratum.clone(),
                    reason: format!("the {class:?} max_wilson_95 must be between 0 and 1"),
                });
            }
            if ceiling.reason.trim().is_empty() {
                return Err(PackError::InvalidEvalDeclaration {
                    stratum: stratum.clone(),
                    reason: format!("the {class:?} threshold has no reason"),
                });
            }
        }
    }

    // An evidence dimension declared without its argument (#430): the
    // manifest records why a pack believes it can score a dimension,
    // not just that it wants to.
    for (dimension, declaration) in &manifest.eval_evidence {
        if declaration.reason.trim().is_empty() {
            return Err(PackError::InvalidEvalDeclaration {
                stratum: format!("{dimension:?}").to_lowercase(),
                reason: "its reason is empty".to_owned(),
            });
        }
    }

    // 4. Reserve and validate the documented shape, but make no claim
    // that parsing a WASM declaration means this runner can execute it.
    validate_wasm_steps(&manifest.wasm_steps)?;
    if !manifest.wasm_steps.is_empty() {
        return Err(PackError::UnsupportedWasm {
            current: current.to_string(),
        });
    }

    // 5. every referenced path stays inside the pack — the trust
    //    boundary, so it is checked before the files are even looked for.
    for step in &manifest.pipeline {
        for relative in referenced_files(step) {
            if !is_contained(relative) {
                return Err(PackError::EscapingPath {
                    path: PathBuf::from(relative),
                });
            }
        }
    }

    // 6. every referenced file exists.
    let mut schema_paths: Vec<PathBuf> = Vec::new();
    for step in &manifest.pipeline {
        for relative in referenced_files(step) {
            let path = dir.join(relative);
            if !path.is_file() {
                return Err(PackError::MissingFile { path });
            }
        }
        if let PipelineStep::Model {
            schema: Some(schema),
            ..
        } = step
        {
            schema_paths.push(dir.join(schema));
        }
    }

    // 7. each schema file parses and builds a validator.
    for path in schema_paths {
        let text = std::fs::read_to_string(&path).map_err(PackError::Io)?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| PackError::InvalidSchema {
                path: path.clone(),
                reason: e.to_string(),
            })?;
        jsonschema::validator_for(&value).map_err(|e| PackError::InvalidSchema {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        // Usable JSON Schema is not enough: it must also stay inside the
        // subset llama-server provably turns into a grammar, or the
        // constraint silently doesn't happen at generation time (§4a).
        crate::exec::assert_grammar_safe(&value).map_err(|reason| PackError::InvalidSchema {
            path: path.clone(),
            reason,
        })?;
    }

    // 8. every model prompt parses. Syntax only: which variables a
    // prompt may use depends on its step's context, so that half is
    // only provable at render time.
    for step in &manifest.pipeline {
        if let PipelineStep::Model { prompt, .. } = step {
            let path = dir.join(prompt);
            let text = std::fs::read_to_string(&path).map_err(PackError::Io)?;
            crate::exec::check_prompt_syntax(&text).map_err(|e| PackError::BrokenPrompt {
                path: path.clone(),
                reason: e.to_string(),
            })?;
        }
    }

    // 9. every step names semantics this runner can actually execute:
    //    a builtin it has, or a model role it reads. Both sets are
    //    closed, so both are answerable here rather than mid-run.
    for step in &manifest.pipeline {
        let (named, known) = match step {
            PipelineStep::Preprocess { implementation } => {
                (implementation.clone(), PREPROCESS_BUILTINS)
            }
            PipelineStep::Aggregate { implementation } => {
                (implementation.clone(), AGGREGATE_BUILTINS)
            }
            // A schema-bearing step's answers are read into named
            // fields. No role means nothing says which, and silence is
            // exactly the case that used to be resolved by counting.
            PipelineStep::Model {
                schema: Some(_),
                role,
                prompt,
                ..
            } => (
                role.clone()
                    .unwrap_or_else(|| format!("a step with no role ({prompt})")),
                MODEL_ROLES,
            ),
            _ => continue,
        };
        if !known.contains(&named.as_str()) {
            return Err(PackError::UnsupportedStep { step: named });
        }
    }

    // 10. model steps asking for a batch size of 0.
    for step in &manifest.pipeline {
        if let PipelineStep::Model {
            prompt,
            batch: Some(0),
            ..
        } = step
        {
            return Err(PackError::ZeroBatch {
                prompt: prompt.clone(),
            });
        }
    }

    // 10b. every pack says what it will do, in its own words (#244).
    //      Refused with a message that says what to write, and any
    //      `will` reference to a progress step must resolve — prose may
    //      group steps, but a name that resolves to nothing is a bug.
    validate_copy(&manifest)?;

    // 11. a classify-role step needs the kinds map, complete both ways
    //     against its category enum (#253). The runner derives every
    //     recurring merchant's kind through this map, so a category it
    //     cannot answer for is a pipeline it cannot finish — found
    //     here, not mid-run.
    validate_kinds(dir, &manifest)?;

    //     …and a policy-terms step needs `value_kinds`, complete both
    //     ways against its term enum (#380). A term with no declared
    //     kind is a value nothing checks, which is how a policy period
    //     rendered as a cover limit on the first real document.
    validate_value_kinds(dir, &manifest)?;

    // 12. a diff needs two documents to compare (#350). A pack declaring
    //     `builtin:term-diff` over a single input would report every
    //     term it read as newly added — a renewal report whose every
    //     row is a finding, and none of them true. The manifest already
    //     says how many documents the pack takes.
    if manifest
        .pipeline
        .iter()
        .any(|step| matches!(step, PipelineStep::Aggregate { implementation } if implementation == "builtin:term-diff"))
    {
        if manifest.inputs.len() < 2 {
            return Err(PackError::CannotCompare {
                roles: manifest.inputs.len(),
            });
        }
        // The two compared documents must each be exactly one file. A
        // role taking several leaves "which one did it compare?"
        // unanswerable — and unanswered questions about which document
        // is which are how a diff reports a price cut for a rise.
        for input in manifest.inputs.iter().take(2) {
            if input.count != Count::Exactly(1) {
                return Err(PackError::AmbiguousComparison {
                    role: input.role.clone(),
                });
            }
        }
    }

    // 13. a declared count must be satisfiable (#334 §1). A role that
    //     can never be filled is a pack that cannot run, and finding
    //     that out at bind time costs a person their file picker.
    validate_counts(&manifest)?;

    Ok(Pack {
        dir: dir.to_path_buf(),
        manifest,
    })
}

/// Refuse a count no set of files can satisfy (#334 §1).
///
/// Two cases, and the reason each is a load-time error rather than a
/// bind-time one: a `max` below the `min` describes an empty set, and a
/// count of nothing describes a role that exists and is never supplied.
/// Both make the pack unrunnable, and a person finds that out after
/// choosing their documents unless it is caught here.
///
/// `reason` is the plain sentence a pack author reads. It says what the
/// manifest declared, not what the code expected.
fn validate_counts(manifest: &Manifest) -> Result<(), PackError> {
    for input in &manifest.inputs {
        let unsatisfiable = |reason: String| {
            Err(PackError::UnsatisfiableCount {
                role: input.role.clone(),
                reason,
            })
        };
        match input.count {
            Count::Exactly(0) => {
                return unsatisfiable(
                    "it asks for no files, which is a role that exists and is never given one"
                        .to_owned(),
                )
            }
            Count::Between {
                min: Some(min),
                max: Some(max),
            } if min > max => {
                return unsatisfiable(format!(
                    "it asks for at least {min} and at most {max}, and no number is both"
                ))
            }
            Count::Between { max: Some(0), .. } => {
                return unsatisfiable(
                    "it allows at most no files, which is a role that exists and is never \
                     given one"
                        .to_owned(),
                )
            }
            _ => {}
        }
    }
    Ok(())
}

/// The kinds a recurring series can be. `one_off` is deliberately not
/// here — a series recurs — and `income` is the credit path's word,
/// never a map entry.
const RECURRING_KINDS: &[&str] = &["subscription", "utility", "regular_spend"];

/// Hold `kinds` complete against the classify step's category enum.
/// The copy block is present, and every `will` reference names a real
/// progress step (#244).
fn validate_copy(manifest: &Manifest) -> Result<(), PackError> {
    let Some(copy) = &manifest.copy else {
        return Err(PackError::MissingCopy);
    };
    let labels = crate::run::step_labels(manifest);
    for entry in &copy.will {
        for named in &entry.steps {
            if !labels.contains(named) {
                return Err(PackError::UnknownWillStep {
                    named: named.clone(),
                    available: labels,
                });
            }
        }
    }
    Ok(())
}

fn validate_kinds(dir: &Path, manifest: &Manifest) -> Result<(), PackError> {
    let invalid = |reason: String| PackError::InvalidKinds { reason };

    let classify_schema = manifest.pipeline.iter().find_map(|step| match step {
        PipelineStep::Model {
            role: Some(role),
            schema: Some(schema),
            ..
        } if role == "classify" => Some(schema.clone()),
        _ => None,
    });
    let Some(schema) = classify_schema else {
        // No classify step, no derivation, no map needed — a stray map
        // is tolerated rather than refused, since it asks for nothing.
        return Ok(());
    };

    if manifest.kinds.is_empty() {
        return Err(invalid(
            "a pack that sorts merchants must say what a recurring payment of each              category is — add a kinds map (category to subscription, utility or              regular_spend)"
                .to_owned(),
        ));
    }

    // The categories the model can actually answer, from the schema the
    // step itself declares — the map is validated against the question
    // as asked, not against a list kept somewhere else.
    let text = std::fs::read_to_string(dir.join(&schema)).map_err(PackError::Io)?;
    let value: serde_json::Value = serde_json::from_str(&text).expect("validated above");
    let categories: Vec<String> = value["properties"]["results"]["items"]["properties"]["category"]
        ["enum"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if categories.is_empty() {
        return Err(invalid(format!(
            "the classify answers file ({schema}) doesn't list its categories, so the              kinds map has nothing to be checked against"
        )));
    }

    for category in &categories {
        if !manifest.kinds.contains_key(category) {
            return Err(invalid(format!(
                "the category {category:?} has no kinds entry — every category needs                  one, or a recurring payment of it would take a kind nobody decided"
            )));
        }
    }
    for (category, kind) in &manifest.kinds {
        if !categories.iter().any(|known| known == category) {
            return Err(invalid(format!(
                "the kinds entry {category:?} names a category the pack never uses"
            )));
        }
        if !RECURRING_KINDS.contains(&kind.as_str()) {
            return Err(invalid(format!(
                "the kinds entry for {category:?} says {kind:?}, but a recurring                  payment can only be a subscription, a utility or regular spending"
            )));
        }
    }
    Ok(())
}

/// Hold `value_kinds` complete against the policy-terms step's term
/// enum (#380).
///
/// The same shape as [`validate_kinds`] and for the same reason: the
/// runner checks every read value against the kind its term declares,
/// so a term with no entry is a value that passes unchecked — and it
/// fails *open*, looking guarded while checking nothing. Found here
/// rather than mid-run, like every other unexecutable pipeline (#120).
fn validate_value_kinds(dir: &Path, manifest: &Manifest) -> Result<(), PackError> {
    let invalid = |reason: String| PackError::InvalidValueKinds { reason };

    let terms_schema = manifest.pipeline.iter().find_map(|step| match step {
        PipelineStep::Model {
            role: Some(role),
            schema: Some(schema),
            ..
        } if role == "policy-terms" => Some(schema.clone()),
        _ => None,
    });
    let Some(schema) = terms_schema else {
        // No terms step, no values to check, no map needed.
        return Ok(());
    };

    if manifest.value_kinds.is_empty() {
        return Err(invalid(
            "a pack that reads named values must say what kind of value each term holds \
             — add a value_kinds map (term to money, percentage, duration or text)"
                .to_owned(),
        ));
    }

    // The terms the model can actually answer, from the schema the step
    // itself declares — the map is validated against the question as
    // asked, not against a list kept somewhere else.
    let text = std::fs::read_to_string(dir.join(&schema)).map_err(PackError::Io)?;
    let value: serde_json::Value = serde_json::from_str(&text).expect("validated above");
    let terms: Vec<String> = value["properties"]["results"]["items"]["properties"]["terms"]
        ["items"]["properties"]["term"]["enum"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if terms.is_empty() {
        return Err(invalid(format!(
            "the terms answers file ({schema}) doesn't list its terms, so the value_kinds \
             map has nothing to be checked against"
        )));
    }

    for term in &terms {
        // `other` is a routing answer, not a value: it never pairs and
        // never reaches the diff, so there is nothing about it to check.
        if term == crate::terms::OTHER {
            continue;
        }
        if !manifest.value_kinds.contains_key(term) {
            return Err(invalid(format!(
                "the term {term:?} has no value_kinds entry — every term needs one, or a \
                 value of the wrong kind would be compared as though it were right"
            )));
        }
    }
    for (term, shape) in &manifest.value_kinds {
        if term == crate::terms::OTHER {
            return Err(invalid(format!(
                "the value_kinds entry {term:?} names the routing answer, which carries no \
                 value to check"
            )));
        }
        if !terms.iter().any(|known| known == term) {
            return Err(invalid(format!(
                "the value_kinds entry {term:?} names a term the pack never uses"
            )));
        }
        if shape.is_empty() {
            return Err(invalid(format!(
                "the value_kinds entry for {term:?} lists no kinds, which is a term whose \
                 every value would go to a person"
            )));
        }
    }
    Ok(())
}

/// Split items for a batched model step at the size its manifest
/// declares (#21). `None` for non-model steps and for unbatched model
/// steps — the summary step makes one call over the aggregate, not a
/// batch sweep. Load-time validation already refused `batch: 0`, so
/// `batch_items`' clamp stays dead code.
pub fn step_batches(
    step: &PipelineStep,
    items: &[String],
) -> Option<Vec<Vec<crate::exec::BatchItem>>> {
    match step {
        PipelineStep::Model {
            batch: Some(size), ..
        } => Some(crate::exec::batch_items(items, *size as usize)),
        _ => None,
    }
}
