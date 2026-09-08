//! Explicit, one-attempt challenge execution. Declarations are not proof of
//! independence; freezing/reuse review remains the author's responsibility.
use super::corpus::Corpus;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Attempt {
    record: Value,
    replay: bool,
}

fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|s| !s.trim().is_empty())
}

fn digest(text: &str) -> String {
    let hex: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

/// Validate the same facts/spans/asks as diagnostics, retaining challenge identity.
pub fn validate(text: &str) -> Result<Corpus, String> {
    let corpus = Corpus::parse_challenge(text)?;
    let author = &corpus.provenance["authoring"];
    let sources = author["source_families"].as_array();
    if corpus.provenance["independent"] != true
        || !nonempty(&author["author"])
        || author["relationship"] != "separate-author"
        || sources.is_none_or(|s| s.is_empty())
    {
        return Err("challenge requires a separate author's external-family declaration".into());
    }
    let mut families = std::collections::BTreeSet::new();
    for source in sources.unwrap() {
        if !nonempty(&source["id"])
            || !nonempty(&source["locator"])
            || source["relationship"] != "external-source"
            || source["id"]
                .as_str()
                .unwrap()
                .starts_with("kettle-examples")
            || !families.insert(source["id"].as_str().unwrap())
        {
            return Err("challenge requires distinct external source families".into());
        }
    }
    Ok(corpus)
}

fn read(text: &str, path: &Path) -> Result<Value, String> {
    let corpus = validate(text)?;
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if record["schema"] != "kettle/challenge-lifecycle@1"
        || record["corpus_digest"] != digest(text)
        || record["selection"] != serde_json::from_str::<Value>(text).unwrap()["selection"]
        || record["authoring"] != corpus.provenance["authoring"]
        || record["cases"] != json!(corpus.cases.len())
    {
        return Err("challenge content, selection or authoring changed after freezing".into());
    }
    let events = record["events"]
        .as_array()
        .ok_or("invalid challenge lifecycle")?;
    if !(1..=2).contains(&events.len())
        || events[0]["event"] != "frozen"
        || events.iter().any(|e| !nonempty(&e["at"]))
        || (events.len() == 2
            && (events[1]["event"] != "exposed"
                || !nonempty(&events[1]["reason"])
                || !nonempty(&events[1]["evidence"])))
    {
        return Err("invalid challenge lifecycle".into());
    }
    Ok(record)
}

fn pending(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".pending");
    name.into()
}

impl Attempt {
    /// Consume before starting the sidecar or sending any request. Failed runs
    /// also consume the selection. Exclusive pending files coordinate with the
    /// Python exposure command; an interrupted write fails closed.
    pub fn start(text: &str, path: &Path, output: &Path, source: &str) -> Result<Self, String> {
        let initial = read(text, path)?;
        if initial["events"].as_array().unwrap().len() != 1 {
            return Err(
                "challenge is already regression material; use exact replay or a fresh selection"
                    .into(),
            );
        }
        let temporary = pending(path);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("challenge lifecycle is locked or unavailable: {e}"))?;
        let mut record = match read(text, path) {
            Ok(record) if record == initial => record,
            _ => {
                drop(file);
                let _ = std::fs::remove_file(&temporary);
                return Err("challenge lifecycle changed while reserving execution".into());
            }
        };
        let output = std::path::absolute(output).map_err(|e| e.to_string())?;
        record["events"].as_array_mut().unwrap().push(json!({
            "event": "exposed", "at": chrono::Utc::now().to_rfc3339(),
            "reason": "challenge execution reserved; failures also consume this selection",
            "evidence": output.to_string_lossy(), "answer_source": source
        }));
        file.write_all(serde_json::to_string_pretty(&record).unwrap().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(temporary, path).map_err(|e| e.to_string())?;
        Ok(Self {
            record,
            replay: false,
        })
    }

    /// Replay retains the consumed lifecycle; it cannot make a fresh claim.
    pub fn replay(text: &str, path: &Path, previous: &Value) -> Result<Self, String> {
        if pending(path).exists() {
            return Err(
                "challenge lifecycle has a pending write; investigate before replay".into(),
            );
        }
        let record = read(text, path)?;
        if record != *previous || record["events"].as_array().unwrap().len() != 2 {
            return Err("replay challenge lifecycle differs from the recorded attempt".into());
        }
        Ok(Self {
            record,
            replay: true,
        })
    }

    pub(crate) fn corpus(&self, text: &str, replay: bool) -> Result<Corpus, String> {
        if self.replay != replay {
            return Err("challenge attempt and answer source disagree about replay".into());
        }
        if self.record["corpus_digest"] != digest(text) {
            return Err("challenge attempt belongs to different corpus bytes".into());
        }
        validate(text)
    }

    pub(crate) fn record(self) -> Value {
        self.record
    }
}
