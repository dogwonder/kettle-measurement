//! Answers an earlier run recorded, replayed (#288).
//!
//! A scorer change can be checked against the answers already recorded,
//! without asking a model again. Replay preserves those observations;
//! it does not assume that a new live run would produce identical bytes.
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
//! New recordings retain the complete generation payload. Legacy archives
//! retain only prompt and policy: their schema compatibility is unknown,
//! and the report explicitly identifies that limitation.

use crate::exec::GenerationRequest;
use std::collections::{BTreeMap, BTreeSet};
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
    exact_prompts: BTreeSet<String>,
}

impl Recording {
    /// Every exchange under a run-directory root, however deep.
    ///
    /// `evals/runs/run1/<pack>-<model>-<fixture>/raw/NNNN-<step>.request.txt`,
    /// `.generation.json` and `.response.json`. Older `.request.json`
    /// prompts remain readable. A request without a response
    /// (the interruption that stopped the run mid-write) is skipped
    /// rather than half-loaded.
    pub fn from_run_dirs(root: &Path) -> Result<Self, String> {
        let mut recording = Self::default();
        let mut models: BTreeMap<String, crate::eval::ModelInfo> = BTreeMap::new();
        collect(root, &mut recording, &mut models, None, None)?;
        if recording.is_empty() {
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
            let named: Vec<&str> = models.values().map(|model| model.file.as_str()).collect();
            return Err(format!(
                "the recording under {} holds answers from more than one model ({}) — a \
                 replay serves them all and could only label the report with one. Point \
                 --replay at a directory holding a single model's runs.",
                root.display(),
                named.join(", "),
            ));
        }
        recording.model = models.into_values().next();
        Ok(recording)
    }

    pub fn has_exact(&self, prompt: &str, schema: &serde_json::Value) -> bool {
        self.answers.contains_key(&digest(prompt, schema))
    }

    pub fn compatibility(&self) -> ReplayCompatibility {
        let legacy_prompt_only_requests = self
            .answers
            .keys()
            .filter(|key| key.starts_with("blake3:request-prompt:"))
            .count();
        ReplayCompatibility {
            exact_requests: self.len() - legacy_prompt_only_requests,
            legacy_prompt_only_requests,
        }
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

    /// Match complete identity first, then legacy prompt/policy only when
    /// no exact recording owns this prompt. The legacy path cannot establish
    /// schema compatibility; callers expose that via `compatibility()`.
    pub fn answer_for(&self, prompt: &str, schema: &serde_json::Value) -> Option<String> {
        self.answers
            .get(&digest(prompt, schema))
            .or_else(|| {
                (!self.exact_prompts.contains(&digest_prompt_only(prompt)))
                    .then(|| self.answers.get(&digest_prompt_only(prompt)))
                    .flatten()
            })
            .cloned()
    }

    /// Add one exchange. Used by the loader and by tests.
    pub fn insert(&mut self, prompt: &str, schema: &serde_json::Value, body: impl Into<String>) {
        self.exact_prompts.insert(digest_prompt_only(prompt));
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

/// Disk and in-memory recordings use exactly the same generation identity.
fn digest(prompt: &str, schema: &serde_json::Value) -> String {
    GenerationRequest::current(prompt, schema).digest()
}

/// Legacy archives have only prompt and request policy. This key cannot
/// establish schema compatibility; new recordings always use full identity.
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
    recording: &mut Recording,
    models: &mut BTreeMap<String, crate::eval::ModelInfo>,
    inherited_policy: Option<&crate::exec::RequestPolicy>,
    inherited_version: Option<u32>,
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
    let manifest_path = dir.join("run.json");
    let manifest: Option<serde_json::Value> = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => Some(serde_json::from_str(&text).map_err(|e| {
            format!(
                "{}: invalid recording manifest: {e}",
                manifest_path.display()
            )
        })?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", manifest_path.display())),
    };
    let version = match manifest
        .as_ref()
        .and_then(|m| m.get("generation_request_version"))
    {
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| "invalid generation request version".to_owned())?,
        ),
        None => inherited_version,
    };
    if version.is_some_and(|v| v != GenerationRequest::VERSION) {
        return Err(format!(
            "unsupported generation request version {version:?}"
        ));
    }
    let request_policy = match manifest.as_ref().and_then(|m| m.get("request")) {
        Some(value) if !value.is_null() => {
            serde_json::from_value::<crate::exec::RequestPolicy>(value.clone())
                .map_err(|e| format!("invalid recorded request policy: {e}"))?
        }
        _ => inherited_policy
            .cloned()
            .unwrap_or_else(crate::exec::RequestPolicy::legacy),
    };
    if let Some(value) = manifest
        .as_ref()
        .and_then(|m| m.get("model"))
        .filter(|v| !v.is_null())
    {
        let model: crate::eval::ModelInfo = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid recorded model identity: {e}"))?;
        models.insert(
            serde_json::to_string(&model).expect("model serialises"),
            model,
        );
    }

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, recording, models, Some(&request_policy), version)?;
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
        let identity_path = path.with_file_name(format!("{stem}.generation.json"));
        let key = match std::fs::read_to_string(&identity_path) {
            Ok(text) => {
                let generation: GenerationRequest = serde_json::from_str(&text)
                    .map_err(|e| format!("{}: invalid generation identity: {e}", identity_path.display()))?;
                if generation.version != GenerationRequest::VERSION || generation.prompt() != Some(request_text.as_str())
                    || generation.payload.pointer("/response_format/json_schema/schema").is_none() {
                    return Err(format!("{}: incompatible or inconsistent generation identity", identity_path.display()));
                }
                recording.exact_prompts.insert(digest_prompt_only(&request_text));
                generation.digest()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && version.is_none() => {
                digest_policy_prompt(&request_policy, &request_text)
            }
            Err(e) => return Err(format!("{}: missing or unreadable generation identity: {e}; cannot use prompt-only matching", identity_path.display())),
        };
        if let Some(previous) = recording.answers.get(&key) {
            if previous != &body {
                return Err(format!(
                    "the recording under {} holds different answers for the same request — \
                     it mixes runs whose request identity cannot distinguish them. Point \
                     --replay at one coherent run set.",
                    dir.display()
                ));
            }
        } else {
            recording.answers.insert(key, body);
        }
    }
    Ok(())
}

/// A legacy match cannot establish schema compatibility, even if its answer
/// validates today. Kept with replay results rather than called exact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReplayCompatibility {
    pub exact_requests: usize,
    pub legacy_prompt_only_requests: usize,
}
