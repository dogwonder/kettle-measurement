//! Answers an earlier run recorded, replayed (#288).
//!
//! At temperature 0, with a fixed prompt, schema and model, an answer
//! is deterministic — the property the whole eval discipline already
//! rests on, and the one prompt caching exploits a level further down.
//! So a *scorer* change never needs the model: it needs the answers,
//! and Kettle already writes every exchange to a run directory (brief
//! §11), today only as a diagnostic record.
//!
//! That turns verifying a scoring change from a 115-minute measurement
//! (#242) into seconds, which is what makes "read the answers before
//! theorising" (`evals/README.md`) affordable rather than a commitment.
//!
//! # Why the request is the key
//!
//! A recording keyed by position — batch 3 of fixture 12 — would
//! happily serve the old prompt's answers to a new prompt's questions,
//! and report it as a clean run. Keyed on the request itself, a changed
//! prompt, schema or example set produces a request nothing matches,
//! and the run stops with a sentence saying so. The safety property is
//! structural rather than remembered.
//!
//! A prompt or request-policy change invalidates a recording. A scorer
//! change does not, because it cannot alter what the model would have
//! said — which is precisely the case this exists to serve.

use std::collections::BTreeMap;
use std::path::Path;

/// The completion bodies an earlier run received, by request.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Recording {
    /// request digest -> the raw completion body as it arrived.
    answers: BTreeMap<String, String>,
    /// Whose answers these are (#303). `None` for a recording of the
    /// deterministic floor, or of a run written before run directories
    /// carried the model.
    model: Option<crate::eval::ModelInfo>,
}

impl Recording {
    /// Every exchange under a run-directory root, however deep.
    ///
    /// `evals/runs/run1/<pack>-<model>-<fixture>/raw/NNNN-<step>.request.json`
    /// and its `.response.json` sibling. A request without a response
    /// (the interruption that stopped the run mid-write) is skipped
    /// rather than half-loaded.
    pub fn from_run_dirs(root: &Path) -> Result<Self, String> {
        let mut answers = BTreeMap::new();
        let mut models: BTreeMap<String, crate::eval::ModelInfo> = BTreeMap::new();
        collect(root, &mut answers, &mut models, None)?;
        if answers.is_empty() {
            return Err(format!(
                "no recorded answers under {} — a replay needs a run that kept its \
                 exchanges, which every eval writes to evals/runs/run<N>/.",
                root.display()
            ));
        }
        // Two models under one root means the answers came from both,
        // and any single label on the resulting report would be a false
        // claim about the evidence — the defect #303 is about, one layer
        // down. Refused rather than resolved: only the person who made
        // these directories knows which one they meant.
        if models.len() > 1 {
            let named: Vec<&str> = models.keys().map(String::as_str).collect();
            return Err(format!(
                "the recording under {} holds answers from more than one model ({}) — a \
                 replay serves them all and could only label the report with one. Point \
                 --replay at a directory holding a single model's runs.",
                root.display(),
                named.join(", "),
            ));
        }
        Ok(Recording {
            answers,
            model: models.into_values().next(),
        })
    }

    /// Whose answers this recording serves, if the runs recorded it.
    ///
    /// `baseline::compare` joins reports on the model, so a replayed
    /// report that could not name one could never be compared against a
    /// live measurement — which is the whole of #303. `None` is honest
    /// for the floor, and for a recording made before run directories
    /// carried this.
    pub fn model(&self) -> Option<&crate::eval::ModelInfo> {
        self.model.as_ref()
    }

    /// How many distinct questions this recording can answer. Reported
    /// so a replay can say what it is standing on.
    pub fn len(&self) -> usize {
        self.answers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.answers.is_empty()
    }

    /// The recorded completion body for this exact request, if there is
    /// one. `None` is the honest answer for a question the recording
    /// never heard, and the caller must refuse rather than improvise.
    pub fn answer_for(&self, prompt: &str, schema: &serde_json::Value) -> Option<String> {
        self.answers
            .get(&digest(prompt, schema))
            .or_else(|| self.answers.get(&digest_prompt_only(prompt)))
            .cloned()
    }

    /// Add one exchange. Used by the loader and by tests.
    pub fn insert(&mut self, prompt: &str, schema: &serde_json::Value, body: impl Into<String>) {
        self.answers.insert(digest(prompt, schema), body.into());
    }

    /// Every recorded answer, in deterministic digest order. The
    /// mutation harness (#426) walks these to enumerate its sites.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&String, &String)> {
        self.answers.iter()
    }

    /// Replace one recorded answer body in this clone. The mutation
    /// harness alters responses, never requests: the question asked is
    /// part of the mutant's identity.
    pub(crate) fn replace_answer(&mut self, digest: &str, body: String) {
        self.answers.insert(digest.to_owned(), body);
    }
}

/// A question's exact in-memory identity: the chat-completions request
/// Kettle sends, including the rendered prompt and schema.
///
/// Length-prefixed so a prompt ending where a schema begins cannot
/// collide with a different split of the same bytes.
fn digest(prompt: &str, schema: &serde_json::Value) -> String {
    let request = crate::exec::RequestPolicy::current().request(prompt, schema);
    let request = serde_json::to_vec(&request).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(request.len() as u64).to_le_bytes());
    hasher.update(&request);
    format!("blake3:{}", hasher.finalize().to_hex())
}

/// The key for an entry loaded from a run directory, where the prompt
/// is recorded per exchange and the stable request policy is recorded
/// once in `run.json`. The schema remains unavailable on legacy and
/// current run directories; in-memory recordings use [`digest`] and
/// keep its exact identity.
pub(crate) fn digest_prompt_only(prompt: &str) -> String {
    digest_policy_prompt(&crate::exec::RequestPolicy::current(), prompt)
}

fn digest_policy_prompt(policy: &crate::exec::RequestPolicy, prompt: &str) -> String {
    let policy = serde_json::to_vec(policy).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    for part in [policy.as_slice(), prompt.as_bytes()] {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("blake3:request-prompt:{}", hasher.finalize().to_hex())
}

fn collect(
    dir: &Path,
    answers: &mut BTreeMap<String, String>,
    models: &mut BTreeMap<String, crate::eval::ModelInfo>,
    inherited_policy: Option<&crate::exec::RequestPolicy>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A directory that isn't there yet is not an error here; the
        // empty-recording check above gives the useful sentence.
        Err(_) => return Ok(()),
    };
    // Read the run-level request identity before its raw exchanges.
    // Directory iteration order is unspecified, so learning this while
    // walking the entries could key an exchange before its manifest.
    let manifest = std::fs::read_to_string(dir.join("run.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let request_policy = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("request"))
        .and_then(|request| {
            serde_json::from_value::<crate::exec::RequestPolicy>(request.clone()).ok()
        })
        .or_else(|| inherited_policy.cloned())
        // Archived recordings predate #328. Treat them as the request
        // shape current when the compatibility rule landed: this keeps
        // the archive usable, while any future role/bound/format change
        // gets a different key and refuses them.
        .unwrap_or_else(crate::exec::RequestPolicy::current);
    if let Some(model) = manifest.as_ref().and_then(|manifest| {
        serde_json::from_value::<crate::eval::ModelInfo>(manifest.get("model")?.clone()).ok()
    }) {
        models.insert(model.file.clone(), model);
    }

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, answers, models, Some(&request_policy))?;
            continue;
        }
        if path.file_name().is_some_and(|name| name == "run.json") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Both namings, deliberately and permanently (#478). The
        // request file holds the rendered prompt as plain text, so
        // `.txt` is what the writer emits; `.json` is what every run
        // archived before 16 August 2026 is called, and those
        // recordings are the whole reason the archive exists — they
        // let a score be re-asked under new scoring without re-running
        // the GPU. Reading only the new name would strand them.
        let Some(stem) = name
            .strip_suffix(".request.txt")
            .or_else(|| name.strip_suffix(".request.json"))
        else {
            continue;
        };
        let response = path.with_file_name(format!("{stem}.response.json"));
        let (Ok(request_text), Ok(body)) = (
            std::fs::read_to_string(&path),
            std::fs::read_to_string(&response),
        ) else {
            continue; // a request whose answer never landed
        };
        // A run directory records the rendered prompt, not the HTTP
        // payload, so the prompt is the key. The schema is not on
        // disk: `schema_free` entries match whatever schema the
        // replaying run supplies, which is sound because a schema
        // change without a prompt change is not a thing any pack has
        // done — but it is a gap, and it is written down rather than
        // hidden.
        let key = digest_policy_prompt(&request_policy, &request_text);
        if let Some(previous) = answers.get(&key) {
            if previous != &body {
                return Err(format!(
                    "the recording under {} holds different answers for the same request — \
                     it mixes runs whose request identity cannot distinguish them. Point \
                     --replay at one coherent run set.",
                    dir.display()
                ));
            }
        } else {
            answers.insert(key, body);
        }
    }
    Ok(())
}
