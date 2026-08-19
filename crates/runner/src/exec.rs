//! The generic model-step executor: render a prompt over a batch of
//! items, ask the model under a schema constraint, rejoin the answers.
//!
//! Rust owns the batch ids. The model echoes them back, so a dropped or
//! reordered result costs one item to needs-review rather than silently
//! misattributing every answer in the batch.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One item put to the model, carrying the id Rust rejoins on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchItem {
    pub id: usize,
    pub raw: String,
    /// The stable source this transient batch item came from. It never
    /// reaches the prompt (`serde(skip)`); eval logging uses it to join
    /// a batched exchange back to the scored fixture item it affected.
    #[serde(skip)]
    pub source: Option<String>,
}

impl BatchItem {
    pub fn new(id: usize, raw: impl Into<String>) -> Self {
        BatchItem {
            id,
            raw: raw.into(),
            source: None,
        }
    }
}

/// Split a step's items into batches of the size the pack asks for,
/// numbering them across the whole step rather than per batch — a result
/// carrying id 41 is then unambiguous whichever batch produced it.
///
/// A size of zero would be a broken manifest; clamp rather than divide
/// by it, so a bad pack can't stall a run.
pub fn batch_items(items: &[String], size: usize) -> Vec<Vec<BatchItem>> {
    let size = size.max(1);
    items
        .chunks(size)
        .enumerate()
        .map(|(batch, chunk)| {
            chunk
                .iter()
                .enumerate()
                .map(|(offset, raw)| BatchItem::new(batch * size + offset, raw.as_str()))
                .collect()
        })
        .collect()
}

/// Prompt rendering failures. A pack whose template doesn't parse is
/// caught at load time (`check_prompt_syntax`, via the pack loader);
/// render-time failures here mean a step's context didn't satisfy the
/// template, which only a run can prove.
#[derive(Debug)]
pub enum PromptError {
    Render(tera::Error),
    Serialise(serde_json::Error),
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::Render(e) => write!(f, "could not build the prompt: {e}"),
            PromptError::Serialise(e) => write!(f, "could not prepare the batch: {e}"),
        }
    }
}

impl std::error::Error for PromptError {}

/// Check a prompt template parses, without rendering it. This is the
/// load-time check (#16): syntax errors are provable before a run, but
/// variable availability depends on each step's context (batch steps
/// get `batch_json`/`examples`; the summary step gets
/// `aggregate_json`), so it can only be proven at render time.
pub fn check_prompt_syntax(template: &str) -> Result<(), PromptError> {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("prompt", template)
        .map_err(PromptError::Render)?;
    Ok(())
}

/// Render a step's prompt template with its batch, and the step's
/// few-shot examples if it declares any. Examples go in verbatim: the
/// pack owns that copy, the executor only places it.
///
/// Autoescaping is off — the output is a prompt, not HTML, and escaping
/// would corrupt the JSON the model is being shown.
pub fn render_prompt(
    template: &str,
    batch: &[BatchItem],
    examples: Option<&str>,
) -> Result<String, PromptError> {
    let batch_json = serde_json::to_string_pretty(batch).map_err(PromptError::Serialise)?;

    let mut context = tera::Context::new();
    context.insert("batch_json", &batch_json);
    // Always defined, so a template referencing examples renders cleanly
    // whether or not the step supplies any.
    context.insert("examples", examples.unwrap_or("").trim());

    tera::Tera::one_off(template, &context, false).map_err(PromptError::Render)
}

/// The most tokens one answer may generate (#232).
///
/// llama-server's default is unlimited, which is a policy nobody chose:
/// under it, Gemma 4 generated 1,700–2,300 hidden reasoning tokens
/// before each compact JSON answer, and nothing recorded says so. An
/// explicit bound turns runaway generation into a *bounded, visible*
/// event — the server answers `finish_reason: "length"` and the call
/// takes the truncation path that already exists (split the batch, and
/// a single item that still truncates goes to a person), never a quiet
/// acceptance of a cut-off answer.
///
/// The value is generous by an order of magnitude, deliberately. With
/// reasoning off ([`crate::sidecar::Reasoning`]) an answer is compact
/// grammar-constrained JSON — the largest legitimate batch answer
/// observed is a few hundred tokens — so no honest answer approaches
/// the bound, and a generation that does is not an answer worth
/// keeping. Half of [`crate::sidecar::DEFAULT_CONTEXT`], so prompt and
/// answer fit the default context together.
///
/// One constant, used twice: the request carries it, and the recorded
/// runtime policy ([`crate::eval::RuntimePolicy`]) claims it. They
/// cannot drift apart — two copies of a runtime fact is how #251
/// happened.
pub const MAX_ANSWER_TOKENS: u32 = 4096;

/// The stable part of the chat-completions request Kettle sends.
///
/// A rendered prompt is only one field inside this contract. The same
/// bytes in a system turn and a user turn are different questions to a
/// chat template (#328), just as a different output bound or response
/// format is a different measurement. Run directories record this
/// policy once beside the model; replay combines it with each recorded
/// prompt, so a future request-shape change refuses old answers rather
/// than silently treating them as evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RequestPolicy {
    pub(crate) model: String,
    pub(crate) temperature: u32,
    pub(crate) message_role: String,
    pub(crate) max_tokens: u32,
    pub(crate) response_format: String,
}

impl RequestPolicy {
    /// The policy executed by [`call_constrained`]. Kept as one value
    /// so the HTTP request and the run manifest cannot describe two
    /// different contracts.
    pub(crate) fn current() -> Self {
        RequestPolicy {
            model: "local".to_owned(),
            temperature: 0,
            message_role: "user".to_owned(),
            max_tokens: MAX_ANSWER_TOKENS,
            response_format: "json_schema".to_owned(),
        }
    }

    pub(crate) fn request(&self, prompt: &str, schema: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "temperature": self.temperature,
            "messages": [{ "role": self.message_role, "content": prompt }],
            "max_tokens": self.max_tokens,
            "response_format": {
                "type": self.response_format,
                "json_schema": { "schema": schema }
            }
        })
    }
}

/// Why a call was asked again (#150). Different failures with different
/// fixes — a truncation wants a smaller batch, a schema failure wants a
/// better prompt, a rejoin failure wants a firmer echo — so the counts
/// must stay separate for the report to say which happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryReason {
    /// The answer ran out of room and the batch was split.
    Truncation,
    /// The answer failed the schema and was re-asked with the errors.
    Schema,
    /// Answers dropped or mispaired items and those were re-asked.
    Rejoin,
}

/// Retries counted by reason, drained with the rest of the metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetryCounts {
    pub truncation: u32,
    pub schema: u32,
    pub rejoin: u32,
}

impl RetryCounts {
    /// The one number for consumers that don't care why.
    pub fn total(&self) -> u32 {
        self.truncation + self.schema + self.rejoin
    }

    fn record(&mut self, reason: RetryReason) {
        match reason {
            RetryReason::Truncation => self.truncation += 1,
            RetryReason::Schema => self.schema += 1,
            RetryReason::Rejoin => self.rejoin += 1,
        }
    }
}

/// Where the model answers: a llama-server sidecar on localhost.
#[derive(Debug, Default)]
struct ModelTotals {
    calls: u32,
    timed_calls: u32,
    model_ms: f64,
    predicted_ms: f64,
    predicted_tokens: f64,
    retries: RetryCounts,
}

/// The model work completed since an endpoint's metrics were last
/// drained. A missing timings block makes the timing fields zero rather
/// than turning a partial measurement into an apparently complete one;
/// retries remain countable independently.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModelMetrics {
    pub model_ms: u64,
    pub tokens_per_second: f32,
    pub retries: RetryCounts,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    base_url: String,
    metrics: Arc<Mutex<ModelTotals>>,
    /// Answers recorded by an earlier run (#288). When present no HTTP
    /// call is made at all — and none may be: a replay that quietly
    /// fell back to a live model would be a measurement nobody could
    /// tell from a fresh one.
    recording: Option<Arc<crate::eval::replay::Recording>>,
}

impl Endpoint {
    pub fn local(port: u16) -> Self {
        Endpoint {
            base_url: format!("http://127.0.0.1:{port}"),
            metrics: Arc::new(Mutex::new(ModelTotals::default())),
            recording: None,
        }
    }

    /// An endpoint that answers only from what an earlier run recorded.
    ///
    /// At temperature 0, with a fixed prompt, schema and model, the
    /// answer is deterministic — the property the whole eval discipline
    /// already rests on — so replaying it is sound. What makes it
    /// *safe* is that the lookup is keyed on the request itself: a
    /// changed prompt or schema produces a different request, finds no
    /// recorded answer, and fails loudly rather than scoring the new
    /// prompt against the old prompt's answers.
    pub fn replaying(recording: crate::eval::replay::Recording) -> Self {
        Endpoint {
            base_url: String::new(),
            metrics: Arc::new(Mutex::new(ModelTotals::default())),
            recording: Some(Arc::new(recording)),
        }
    }

    fn totals(&self) -> std::sync::MutexGuard<'_, ModelTotals> {
        self.metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn record_completion(&self, envelope: &serde_json::Value) {
        let mut totals = self.totals();
        totals.calls += 1;

        let timings = &envelope["timings"];
        let finite_non_negative = |field: &str| {
            timings[field]
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
        };
        let values = (
            finite_non_negative("prompt_ms"),
            finite_non_negative("predicted_ms"),
            finite_non_negative("predicted_per_second"),
        );
        let (Some(prompt_ms), Some(predicted_ms), Some(predicted_per_second)) = values else {
            return;
        };

        totals.timed_calls += 1;
        totals.model_ms += prompt_ms + predicted_ms;
        totals.predicted_ms += predicted_ms;
        totals.predicted_tokens += predicted_per_second * predicted_ms / 1_000.0;
    }

    /// Count one retry against its reason (#150). Public so the retry
    /// machinery and its tests both talk to the same counters.
    pub fn record_retry(&self, reason: RetryReason) {
        self.totals().retries.record(reason);
    }

    /// Drain the measurements accumulated by every clone of this
    /// endpoint. Fixture evaluation calls this at run boundaries so one
    /// statement's cost cannot leak into the next one's report.
    pub fn take_metrics(&self) -> ModelMetrics {
        let totals = std::mem::take(&mut *self.totals());
        let timings_complete = totals.calls > 0 && totals.calls == totals.timed_calls;
        if !timings_complete {
            return ModelMetrics {
                retries: totals.retries,
                ..ModelMetrics::default()
            };
        }

        let tokens_per_second = if totals.predicted_ms > 0.0 {
            (totals.predicted_tokens / (totals.predicted_ms / 1_000.0)) as f32
        } else {
            0.0
        };
        ModelMetrics {
            model_ms: totals.model_ms.round() as u64,
            tokens_per_second,
            retries: totals.retries,
        }
    }
}

/// The four ways a model call goes wrong. They need different handling
/// (#23, #150), so they must never be conflated (brief §11):
/// - `Transport`: the server couldn't be reached or broke protocol —
///   nothing to do with the model's answer.
/// - `Refused`: the server answered and said no (e.g. it couldn't turn
///   the schema into a grammar). Retrying the same request is pointless.
/// - `Invalid`: the model answered and its answer fails the schema. The
///   one case where a retry with the errors appended makes sense.
/// - `Truncated`: the answer ran out of room (`finish_reason` "length").
///   At temperature 0 an error-annotated retry truncates identically,
///   so the only fix is a smaller batch — never the `Invalid` path.
///
/// `Cancelled` is the fifth outcome and not a failure at all: the
/// person asked to stop while the call was in flight, so the wait was
/// abandoned (#46). It must never be retried and never reported as an
/// error — `run_pack` turns it back into `RunError::Cancelled`.
#[derive(Debug)]
pub enum ModelCallError {
    Transport(String),
    Refused {
        status: u16,
        message: String,
    },
    Invalid {
        errors: Vec<String>,
        content: String,
    },
    Truncated,
    Cancelled,
}

impl std::fmt::Display for ModelCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelCallError::Transport(e) => write!(f, "could not reach the model: {e}"),
            ModelCallError::Refused { status, message } => {
                write!(
                    f,
                    "the model server refused the request ({status}): {message}"
                )
            }
            ModelCallError::Invalid { errors, .. } => {
                write!(
                    f,
                    "the answer did not match what was asked for: {}",
                    errors.join("; ")
                )
            }
            ModelCallError::Truncated => {
                write!(f, "the answer ran out of room before it finished")
            }
            ModelCallError::Cancelled => write!(f, "stopped at your request"),
        }
    }
}

impl std::error::Error for ModelCallError {}

/// How often the waiting thread looks at the cancel flag while a call
/// is in flight — the ceiling on how stale a person's Cancel can go
/// unnoticed mid-call.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// The most one HTTP exchange may take before it counts as transport
/// failure. Generous — a single batch on baseline hardware is well
/// under a minute — because its real job is bounding two hangs: a
/// wedged sidecar with nobody around to cancel, and the abandoned
/// helper thread a cancellation leaves behind.
const CALL_DEADLINE: Duration = Duration::from_secs(600);

/// One constrained call: POST the rendered prompt with the step schema
/// as a `json_schema` response format (never hand-written GBNF, brief
/// §2), temperature 0, then re-validate the answer with `jsonschema` as
/// belt-and-braces before anyone downstream trusts it.
///
/// The blocking exchange runs on its own thread so this one can keep
/// watching `cancel`: a person's Cancel must stop the run even while a
/// call is in flight against a stalled server (#46). On cancellation
/// the helper thread is abandoned, not joined — `CALL_DEADLINE` bounds
/// how long it can outlive the run.
pub fn call_constrained(
    endpoint: &Endpoint,
    prompt: &str,
    schema: &serde_json::Value,
    cancel: &AtomicBool,
) -> Result<serde_json::Value, ModelCallError> {
    // A user turn, not a system one: a chat template may insist on a
    // user query and refuse the request outright without it (Qwen3.5
    // does, with HTTP 400 and no tokens generated). The rendered prompt
    // is instruction and data together, so there is nothing to split.
    //
    // The explicit output bound (#232) turns exhaustion into
    // `finish_reason: "length"` and the truncation path below. Both
    // choices come from the recorded policy, so executed and replayed
    // request identity cannot drift.
    let request = RequestPolicy::current().request(prompt, schema);

    let transport = |e: &dyn std::fmt::Display| ModelCallError::Transport(e.to_string());

    let url = format!("{}/v1/chat/completions", endpoint.base_url);
    let payload = request.to_string();
    let (send_exchange, exchange) = mpsc::channel();
    std::thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(CALL_DEADLINE))
            .build()
            .into();
        let result = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(payload)
            .and_then(|mut response| {
                let status = response.status().as_u16();
                response
                    .body_mut()
                    .read_to_string()
                    .map(|body| (status, body))
            });
        // Nobody listening means the call was cancelled meanwhile.
        send_exchange.send(result).ok();
    });

    // A replay never reaches the network. The request is the key, so
    // this both serves the recorded answer and refuses a prompt the
    // recording knows nothing about. Everything below — the truncation
    // check, the schema re-validation — runs on a replayed answer
    // exactly as on a live one: a replay must be scored by the same
    // rules or it is not the same measurement.
    if let Some(recording) = &endpoint.recording {
        let body = recording.answer_for(prompt, schema).ok_or_else(|| {
            ModelCallError::Transport(
                "this replay has no recorded answer for one of the questions asked. \
                 The prompt, schema or examples have changed since the recording, so \
                 those answers are not evidence about these questions — record again."
                    .to_owned(),
            )
        })?;
        // A run directory records the model's parsed answer, not the
        // server's envelope, so it is validated directly — by the same
        // schema rules a live answer meets.
        return validate_answer(schema, &body, &transport);
    }

    let (status, body) = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ModelCallError::Cancelled);
        }
        match exchange.recv_timeout(CANCEL_POLL) {
            Ok(result) => break result.map_err(|e| transport(&e))?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(transport(&"the model call never finished"));
            }
        }
    };

    if status != 200 {
        return Err(ModelCallError::Refused {
            status,
            message: body,
        });
    }

    finish_completion(endpoint, schema, &body, &transport)
}

/// Re-validate an answer against the step's schema. Shared so a
/// replayed answer meets exactly the bar a live one does.
fn validate_answer(
    schema: &serde_json::Value,
    content: &str,
    transport: &dyn Fn(&dyn std::fmt::Display) -> ModelCallError,
) -> Result<serde_json::Value, ModelCallError> {
    let answer: serde_json::Value =
        serde_json::from_str(content).map_err(|e| ModelCallError::Invalid {
            errors: vec![format!("not valid JSON: {e}")],
            content: content.to_owned(),
        })?;
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| transport(&format!("unusable step schema: {e}")))?;
    let errors: Vec<String> = validator
        .iter_errors(&answer)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(answer)
    } else {
        Err(ModelCallError::Invalid {
            errors,
            content: content.to_owned(),
        })
    }
}

/// Everything after a completion body arrives, however it arrived.
///
/// Shared by the live path and the replay so the two cannot diverge:
/// a replayed answer is truncation-checked and re-validated against the
/// schema exactly as a fresh one is.
fn finish_completion(
    endpoint: &Endpoint,
    schema: &serde_json::Value,
    body: &str,
    transport: &dyn Fn(&dyn std::fmt::Display) -> ModelCallError,
) -> Result<serde_json::Value, ModelCallError> {
    // The completion envelope is the server's, not the model's — a shape
    // we don't recognise is a transport problem, never the model's fault.
    let envelope: serde_json::Value = serde_json::from_str(body).map_err(|e| transport(&e))?;
    endpoint.record_completion(&envelope);
    // A finish_reason of "length" means generation hit the prediction
    // limit and the answer is cut off. Checked before parsing: the
    // remnant is usually broken JSON, and reporting it as Invalid would
    // send it down a retry path that cannot help (#150).
    if envelope["choices"][0]["finish_reason"] == "length" {
        return Err(ModelCallError::Truncated);
    }
    let content = envelope["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| transport(&"no message content in the completion"))?
        .to_owned();

    // From here down it IS the model's answer.
    let answer: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| ModelCallError::Invalid {
            errors: vec![format!("not valid JSON: {e}")],
            content: content.clone(),
        })?;

    let validator = jsonschema::validator_for(schema)
        .map_err(|e| transport(&format!("unusable step schema: {e}")))?;
    let errors: Vec<String> = validator
        .iter_errors(&answer)
        .map(|e| e.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(ModelCallError::Invalid { errors, content });
    }

    Ok(answer)
}

/// The JSON Schema keywords we know llama-server turns into a grammar
/// correctly. Deliberately conservative: outside this subset the
/// converter may silently drop a constraint — generation then runs
/// unconstrained while full-draft re-validation still passes, which is
/// invisible until an eval catches it. Extend only after verifying the
/// keyword against the real llama-server.
const GRAMMAR_SAFE_KEYWORDS: &[&str] = &["type", "properties", "required", "items", "enum"];

/// Check every keyword in `schema` (recursively) is grammar-safe,
/// naming the first offender and where it sits. Pack loading should
/// refuse a pack that fails this — loudly at load, not silently at run.
pub fn assert_grammar_safe(schema: &serde_json::Value) -> Result<(), String> {
    fn walk(node: &serde_json::Value, at: &str) -> Result<(), String> {
        let Some(object) = node.as_object() else {
            return Ok(()); // `required` lists, `enum` values, `type` strings
        };
        for (keyword, value) in object {
            if !GRAMMAR_SAFE_KEYWORDS.contains(&keyword.as_str()) {
                return Err(format!(
                    "schema keyword \"{keyword}\" at {at} is outside the grammar-safe subset \
                     ({}) — the model would not actually be constrained by it",
                    GRAMMAR_SAFE_KEYWORDS.join(", ")
                ));
            }
            match keyword.as_str() {
                "properties" => {
                    for (name, property) in value.as_object().into_iter().flatten() {
                        walk(property, &format!("{at}/{name}"))?;
                    }
                }
                "items" => walk(value, &format!("{at}/items"))?,
                _ => {}
            }
        }
        Ok(())
    }
    walk(schema, "root")
}

/// What one batch produced: validated answers keyed by batch id, plus
/// the items a person needs to look at. Both can be non-empty — a batch
/// partially rejoining is normal, not an error.
#[derive(Debug)]
pub struct StepOutcome {
    pub answers: std::collections::BTreeMap<usize, serde_json::Value>,
    pub needs_review: Vec<NeedsReview>,
    /// Per source item, every candidate seen before the one terminal
    /// result. These are diagnostic evidence for claim tracing (#425),
    /// not additional answers to apply.
    pub attempts: std::collections::BTreeMap<usize, Vec<crate::claim_trace::ClaimAttempt>>,
    /// Schema-valid candidates that could not name an item in this
    /// batch. They must not silently disappear merely because they
    /// cannot affect the product output.
    pub rejected: Vec<RejectedCandidate>,
}

#[derive(Debug)]
pub struct RejectedCandidate {
    pub candidate: serde_json::Value,
    pub reason: String,
}

/// One item routed to the "check these yourself" bucket, with enough
/// context for the report to say why.
#[derive(Debug)]
pub struct NeedsReview {
    pub item: BatchItem,
    pub reason: ReviewReason,
    /// The model's answer, where there was one worth showing — a
    /// low-confidence guess is shown for checking, never silently used.
    pub answer: Option<serde_json::Value>,
}

#[derive(Debug)]
pub enum ReviewReason {
    /// The batch failed schema validation twice (original + one retry).
    BatchFailedTwice { errors: Vec<String> },
    /// The model's results left this id out.
    MissingFromResults,
    /// The id came back but the echoed field didn't match the item —
    /// a confabulated pairing (brief §3).
    MismatchedEcho { echoed: String },
    /// The model marked its own answer low-confidence.
    LowConfidence,
    /// The answer for this item alone still ran out of room — the batch
    /// split bottomed out at one item (#150).
    Truncated,
}

/// A step failure that stops the run — as opposed to batch problems,
/// which land in needs-review and never crash a run (#23).
#[derive(Debug)]
pub enum StepError {
    /// The template didn't render with this batch's context.
    Prompt(PromptError),
    /// Transport or refusal. Never `Invalid` — invalid answers are
    /// retried once, then absorbed into needs-review.
    Call(ModelCallError),
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepError::Prompt(e) => write!(f, "{e}"),
            StepError::Call(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StepError {}

/// Where one batch sits in the run, so its raw exchange can be written
/// to the run directory under a name that says what was happening
/// (#24). Carrying this rather than three loose arguments keeps
/// `run_batch`'s signature readable.
pub struct BatchContext<'a> {
    pub log: &'a dyn crate::run_dir::RunLog,
    /// The progress label, e.g. "Sorting merchants".
    pub step: &'a str,
    /// 1-based, in the order the batches ran.
    pub batch: usize,
    /// The run's stop flag, watched even while a call is in flight
    /// (#46) — a batch belongs to a run a person can cancel.
    pub cancel: &'a AtomicBool,
}

/// The flag `BatchContext::none` waits on: nobody ever sets it.
static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

impl BatchContext<'_> {
    /// A context that records nothing and cannot be cancelled — tests,
    /// and the eval harness.
    pub fn none() -> BatchContext<'static> {
        BatchContext {
            log: &crate::run_dir::NoLog,
            step: "",
            batch: 0,
            cancel: &NEVER_CANCELLED,
        }
    }

    /// Log one exchange and hand the result straight back. Both the
    /// first attempt and the retry go through here, so a retried batch
    /// leaves two numbered pairs in `raw/` and the run reads as it ran.
    fn record<E: std::fmt::Display>(
        &self,
        items: &[BatchItem],
        prompt: &str,
        result: Result<serde_json::Value, E>,
    ) -> Result<serde_json::Value, E> {
        let response = match &result {
            Ok(answer) => answer.to_string(),
            Err(e) => e.to_string(),
        };
        self.log
            .exchange(self.step, self.batch, items, prompt, &response);
        result
    }
}

/// Appended to the re-ask of items a previous answer dropped or
/// mispaired (#150) — after the template, so it renders verbatim below
/// the pack's own instructions.
const REJOIN_NOTE: &str = "\n\nA previous answer dropped some of these items or paired them \
     with the wrong input. Answer each item exactly once, echoing its input exactly as given.";

/// Run one batch through a model step: render, ask under the schema
/// constraint, rejoin on id, route problems to needs-review.
///
/// An invalid answer is retried exactly once with the validation errors
/// appended; a second failure sends the whole batch to needs-review.
/// `echo_field` names the result field that must echo the item's input
/// ("raw" for normalise, "name" for classify) — id match with a
/// mismatched echo is a confabulated pairing and goes to review.
///
/// Items the rejoin couldn't pair — dropped, duplicated or mispaired —
/// get one targeted re-ask as a sub-batch of their own (#150), fired
/// from here and never from `ask_batch`, so it happens at most once per
/// original batch however the sub-batch itself misbehaves. Low
/// confidence is not a pairing failure and is never re-asked: that is
/// the model being honest, not wrong.
pub fn run_batch(
    endpoint: &Endpoint,
    template: &str,
    examples: Option<&str>,
    schema: &serde_json::Value,
    batch: &[BatchItem],
    echo_field: &str,
    context: &BatchContext,
) -> Result<StepOutcome, StepError> {
    let mut outcome = ask_batch(
        endpoint, template, examples, schema, batch, echo_field, context,
    )?;

    let failed_pairing = |reason: &ReviewReason| {
        matches!(
            reason,
            ReviewReason::MissingFromResults | ReviewReason::MismatchedEcho { .. }
        )
    };
    let sub_batch: Vec<BatchItem> = outcome
        .needs_review
        .iter()
        .filter(|review| failed_pairing(&review.reason))
        .map(|review| review.item.clone())
        .collect();
    if sub_batch.is_empty() {
        return Ok(outcome);
    }

    // The re-ask goes through the same machinery — schema retry,
    // truncation split — under the same template plus a note saying
    // what went wrong last time.
    endpoint.record_retry(RetryReason::Rejoin);
    let noted = format!("{template}{REJOIN_NOTE}");
    let reasked = ask_batch(
        endpoint, &noted, examples, schema, &sub_batch, echo_field, context,
    )?;

    // A clean second answer resolves the item; a low-confidence one is
    // shown for checking, as anywhere else. Anything still unpaired
    // keeps its original review entry — the first failure's detail
    // (e.g. what was mis-echoed) is what the report can usefully say.
    let mut low_confidence: std::collections::BTreeMap<usize, NeedsReview> = reasked
        .needs_review
        .into_iter()
        .filter(|review| matches!(review.reason, ReviewReason::LowConfidence))
        .map(|review| (review.item.id, review))
        .collect();
    outcome.needs_review = outcome
        .needs_review
        .into_iter()
        .filter_map(|review| {
            if !failed_pairing(&review.reason) {
                return Some(review);
            }
            if reasked.answers.contains_key(&review.item.id) {
                return None;
            }
            Some(low_confidence.remove(&review.item.id).unwrap_or(review))
        })
        .collect();
    outcome.answers.extend(reasked.answers);
    for (id, attempts) in reasked.attempts {
        outcome.attempts.entry(id).or_default().extend(attempts);
    }
    outcome.rejected.extend(reasked.rejected);
    Ok(outcome)
}

/// Ask one batch: render, call, absorb an invalid answer with one
/// errors-appended retry, split a truncated answer (#150), rejoin.
fn ask_batch(
    endpoint: &Endpoint,
    template: &str,
    examples: Option<&str>,
    schema: &serde_json::Value,
    batch: &[BatchItem],
    echo_field: &str,
    context: &BatchContext,
) -> Result<StepOutcome, StepError> {
    let prompt = render_prompt(template, batch, examples).map_err(StepError::Prompt)?;

    let split = || {
        split_truncated(
            endpoint, template, examples, schema, batch, echo_field, context,
        )
    };

    let mut invalid_first = None;
    let answer = match context.record(
        batch,
        &prompt,
        call_constrained(endpoint, &prompt, schema, context.cancel),
    ) {
        Ok(answer) => answer,
        Err(ModelCallError::Truncated) => return split(),
        Err(ModelCallError::Invalid { errors, content }) => {
            invalid_first = Some(candidate_value(&content));
            // Retry once, quoting the answer alongside what was wrong
            // with it — the errors carry JSON paths into that answer,
            // which the model cannot follow without seeing it.
            endpoint.record_retry(RetryReason::Schema);
            let retry_prompt = format!(
                "{prompt}\n\nYour previous answer was:\n{content}\n\n\
                 It did not match what was asked for:\n- {}\n\n\
                 Answer again, correcting these problems. Return JSON only, matching the schema.",
                errors.join("\n- ")
            );
            match context.record(
                batch,
                &retry_prompt,
                call_constrained(endpoint, &retry_prompt, schema, context.cancel),
            ) {
                Ok(answer) => answer,
                // The longer retry prompt can be what tips the answer
                // over the limit — a split still beats giving up.
                Err(ModelCallError::Truncated) => return split(),
                Err(ModelCallError::Invalid { errors, content }) => {
                    // Twice is enough: the whole batch goes to review.
                    return Ok(StepOutcome {
                        answers: Default::default(),
                        needs_review: batch
                            .iter()
                            .map(|item| NeedsReview {
                                item: item.clone(),
                                reason: ReviewReason::BatchFailedTwice {
                                    errors: errors.clone(),
                                },
                                answer: Some(candidate_value(&content)),
                            })
                            .collect(),
                        attempts: batch
                            .iter()
                            .map(|item| {
                                (
                                    item.id,
                                    vec![
                                        attempt(
                                            invalid_first
                                                .clone()
                                                .unwrap_or(serde_json::Value::Null),
                                            crate::claim_trace::CheckOutcome::Failed,
                                            crate::claim_trace::CheckOutcome::NotApplicable,
                                        ),
                                        attempt(
                                            candidate_value(&content),
                                            crate::claim_trace::CheckOutcome::Failed,
                                            crate::claim_trace::CheckOutcome::NotApplicable,
                                        ),
                                    ],
                                )
                            })
                            .collect(),
                        rejected: Vec::new(),
                    });
                }
                Err(other) => return Err(StepError::Call(other)),
            }
        }
        Err(other) => return Err(StepError::Call(other)),
    };

    let mut outcome = rejoin(batch, &answer, echo_field);
    if let Some(candidate) = invalid_first {
        for item in batch {
            outcome.attempts.entry(item.id).or_default().insert(
                0,
                attempt(
                    candidate.clone(),
                    crate::claim_trace::CheckOutcome::Failed,
                    crate::claim_trace::CheckOutcome::NotApplicable,
                ),
            );
        }
    }
    Ok(outcome)
}

fn candidate_value(content: &str) -> serde_json::Value {
    serde_json::from_str(content).unwrap_or_else(|_| serde_json::Value::String(content.to_owned()))
}

fn attempt(
    candidate: serde_json::Value,
    schema: crate::claim_trace::CheckOutcome,
    pairing: crate::claim_trace::CheckOutcome,
) -> crate::claim_trace::ClaimAttempt {
    crate::claim_trace::ClaimAttempt {
        ordinal: 0,
        candidate,
        schema,
        pairing,
    }
}

/// A truncated batch, halved: at temperature 0 the same ask truncates
/// the same way, so the only lever is fewer items per answer. Each half
/// goes back through `ask_batch` — splitting again if it must — and a
/// single item that still truncates goes to review: there is nowhere
/// smaller to go, and a person must see it. Cancellation propagates out
/// of either half's call as an error, never absorbed here.
fn split_truncated(
    endpoint: &Endpoint,
    template: &str,
    examples: Option<&str>,
    schema: &serde_json::Value,
    batch: &[BatchItem],
    echo_field: &str,
    context: &BatchContext,
) -> Result<StepOutcome, StepError> {
    match batch {
        // Can't happen from `batch_items`, but an empty slice must not
        // split into two empty slices forever.
        [] => Ok(StepOutcome {
            answers: Default::default(),
            needs_review: Vec::new(),
            attempts: Default::default(),
            rejected: Vec::new(),
        }),
        [item] => Ok(StepOutcome {
            answers: Default::default(),
            needs_review: vec![NeedsReview {
                item: item.clone(),
                reason: ReviewReason::Truncated,
                answer: None,
            }],
            attempts: Default::default(),
            rejected: Vec::new(),
        }),
        _ => {
            endpoint.record_retry(RetryReason::Truncation);
            let (left, right) = batch.split_at(batch.len() / 2);
            let mut outcome = ask_batch(
                endpoint, template, examples, schema, left, echo_field, context,
            )?;
            let rest = ask_batch(
                endpoint, template, examples, schema, right, echo_field, context,
            )?;
            outcome.answers.extend(rest.answers);
            outcome.needs_review.extend(rest.needs_review);
            for (id, attempts) in rest.attempts {
                outcome.attempts.entry(id).or_default().extend(attempts);
            }
            outcome.rejected.extend(rest.rejected);
            Ok(outcome)
        }
    }
}

/// Does this echo identify this item, and only this item, in the batch?
///
/// #312: the echo pairs on **prefix uniqueness** rather than on being
/// verbatim. The #283 spike measured the letter pack at 94.6%
/// generation with the verbatim `segment` echo about a quarter of it, so
/// asking for the first few words instead is worth ~20% of a bed run —
/// and it applies to every pack.
///
/// It costs the check nothing, because the echo was never a source of
/// data. The runner already holds every item's text (`run.rs` sets
/// `item.source` from the segment before the call); the echo exists to
/// confirm that the answer filed under id N is about item N. A prefix
/// unique within its batch is a bijection onto that batch's items, which
/// is all equality ever bought.
///
/// Three ways to fail, all landing in review exactly as a mismatch does
/// today:
///
/// - it is not a prefix of this item at all;
/// - it is empty, so it identifies nothing — a missing echo field reads
///   as `""`, and the model not answering is not evidence it read the
///   right item;
/// - it is also a prefix of a *differently worded* item in the batch, so
///   it cannot say which one was read.
///
/// Uniqueness is decided per batch, in Rust, rather than by hardcoding a
/// prefix length. A fixed "three words" would be tuned to today's bed
/// and would silently weaken on a document full of repetitive
/// boilerplate; this degrades into review instead, which is the honest
/// failure. It matters because where two passages differ the
/// discriminating detail is usually at the *end* — "within 21 days"
/// against "within 42 days" — exactly where a leading prefix is blindest.
///
/// Byte-identical items are not ambiguous. No echo of any length
/// distinguishes them, including today's full one, and none needs to:
/// swapping their answers produces the same answer. The spike found 88%
/// of the letter bed's segments are non-unique across letters, so this
/// case is ordinary rather than exotic.
fn pairs(echoed: &str, item: &BatchItem, batch: &[BatchItem]) -> bool {
    if echoed.is_empty() || !item.raw.starts_with(echoed) {
        return false;
    }
    // An exact echo is conclusive on its own, and does not have to be
    // unique. `TESCO` and `TESCO STORES` can share a batch: a verbatim
    // echo of the first is a prefix of the second, and a uniqueness rule
    // alone would send a correctly paired answer to review. This is a
    // relaxation of the echo check, and a relaxation that newly rejects
    // something is not one.
    //
    // The distinction is real rather than grandfathered. A short prefix
    // needs uniqueness because it is weak evidence about which item was
    // read; an exact match is not a prefix of the item, it *is* the
    // item.
    if echoed == item.raw {
        return true;
    }
    !batch
        .iter()
        .any(|other| other.raw.starts_with(echoed) && other.raw != item.raw)
}

/// Match results back to batch items by id (brief §3): missing,
/// duplicate or mismatched-echo items go to review, unknown ids are
/// retained as rejected candidates for diagnostics, and low-confidence
/// answers are shown for checking rather than used.
fn rejoin(batch: &[BatchItem], answer: &serde_json::Value, echo_field: &str) -> StepOutcome {
    let empty = Vec::new();
    let results = answer["results"].as_array().unwrap_or(&empty);

    let known: std::collections::BTreeSet<usize> = batch.iter().map(|item| item.id).collect();
    let mut by_id: std::collections::BTreeMap<usize, Vec<&serde_json::Value>> =
        std::collections::BTreeMap::new();
    let mut rejected = Vec::new();
    for result in results {
        if let Some(id) = result["id"].as_u64() {
            let id = id as usize;
            if known.contains(&id) {
                by_id.entry(id).or_default().push(result);
            } else {
                rejected.push(RejectedCandidate {
                    candidate: result.clone(),
                    reason: format!("result id {id} is not in this batch"),
                });
            }
        } else {
            rejected.push(RejectedCandidate {
                candidate: result.clone(),
                reason: "result has no numeric id".to_owned(),
            });
        }
    }

    let mut outcome = StepOutcome {
        answers: Default::default(),
        needs_review: Vec::new(),
        attempts: Default::default(),
        rejected,
    };
    for item in batch {
        let mut review = |reason, answer| {
            outcome.needs_review.push(NeedsReview {
                item: item.clone(),
                reason,
                answer,
            });
        };
        match by_id.get(&item.id) {
            None => review(ReviewReason::MissingFromResults, None),
            Some(results) if results.len() > 1 => {
                // Two answers claiming one id: neither can be trusted.
                outcome.attempts.insert(
                    item.id,
                    results
                        .iter()
                        .map(|result| {
                            attempt(
                                (*result).clone(),
                                crate::claim_trace::CheckOutcome::Passed,
                                crate::claim_trace::CheckOutcome::Failed,
                            )
                        })
                        .collect(),
                );
                review(
                    ReviewReason::MissingFromResults,
                    Some(serde_json::Value::Array(
                        results.iter().map(|result| (*result).clone()).collect(),
                    )),
                )
            }
            Some(results) => {
                let result = results[0];
                let echoed = result[echo_field].as_str().unwrap_or_default();
                if !pairs(echoed, item, batch) {
                    outcome.attempts.insert(
                        item.id,
                        vec![attempt(
                            result.clone(),
                            crate::claim_trace::CheckOutcome::Passed,
                            crate::claim_trace::CheckOutcome::Failed,
                        )],
                    );
                    review(
                        ReviewReason::MismatchedEcho {
                            echoed: echoed.to_owned(),
                        },
                        Some(result.clone()),
                    );
                } else if result["confidence"] == "low" {
                    outcome.attempts.insert(
                        item.id,
                        vec![attempt(
                            result.clone(),
                            crate::claim_trace::CheckOutcome::Passed,
                            crate::claim_trace::CheckOutcome::Passed,
                        )],
                    );
                    review(ReviewReason::LowConfidence, Some((*result).clone()));
                } else {
                    outcome.attempts.insert(
                        item.id,
                        vec![attempt(
                            result.clone(),
                            crate::claim_trace::CheckOutcome::Passed,
                            crate::claim_trace::CheckOutcome::Passed,
                        )],
                    );
                    outcome.answers.insert(item.id, (*result).clone());
                }
            }
        }
    }
    outcome
}
