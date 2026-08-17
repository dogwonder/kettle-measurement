//! #426: a passing recording synthesised from the bed itself.
//!
//! The mutation harness needs a recording whose replay passes, so that
//! a mutant's failure means the mutation was caught and nothing else.
//! A committed recording would go stale on any prompt edit — the
//! request-digest key makes staleness a hard failure, which is right
//! for replay and poison for a harness input — so the oracle generates
//! one at runtime instead: it runs the pack's own fixtures against a
//! local answering loop that reads each batch question and answers it
//! exactly as `expected.json` says a correct model would.
//!
//! The oracle is not a model and must never look like one: it answers
//! from the bed's authored ground truth, deterministically, in
//! milliseconds, with no network beyond a loopback socket this process
//! owns for the duration of one call.

use super::fixture::{fixtures_in, Fixture, FixtureEvaluator, TermExpectation};
use super::replay::Recording;
use crate::eval::ExpectedTerm;
use crate::packs::Pack;
use crate::run::Answers;
use std::io::{Read, Write};
use std::net::TcpListener;

/// Build a recording of correct answers for every development fixture
/// of this pack, by running the real pipeline against the oracle's
/// answering loop and keeping the exchanges.
pub fn recording(pack: &Pack) -> Result<Recording, String> {
    let fixtures = fixtures_in(pack)?;
    let answers = AnsweringLoop::start(&fixtures)?;

    // Unique per call, not merely per process: parallel test threads
    // building oracles for the same pack id must not share a directory
    // one of them is about to delete.
    static CALL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALL.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let runs = std::env::temp_dir().join(format!(
        "kettle-oracle-{}-{call}-{}",
        std::process::id(),
        pack.manifest.id
    ));
    let _ = std::fs::remove_dir_all(&runs);
    std::fs::create_dir_all(&runs).map_err(|e| format!("oracle runs dir: {e}"))?;

    let evaluator = FixtureEvaluator {
        answers: Answers::FromModel(crate::exec::Endpoint::local(answers.port)),
        model: None,
        machine: super::MachineInfo {
            cpu: "oracle".to_owned(),
            ram_gb: 0,
            os: "oracle".to_owned(),
        },
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: Some(runs.clone()),
        resume_dir: None,
    };
    let outcome = evaluator.evaluate(pack);
    drop(answers);
    outcome?;

    let recording = Recording::from_run_dirs(&runs);
    let _ = std::fs::remove_dir_all(&runs);
    recording
}

/// The ground truth one segment's question is answered from.
#[derive(Debug, Clone)]
enum Truth {
    Obligations(Vec<crate::eval::ExpectedObligation>),
    Terms(Vec<crate::eval::ExpectedTerm>),
}

/// A loopback HTTP loop answering completion requests from the bed's
/// expectations. Dropped when the oracle run finishes; the drop wakes
/// the accept loop so the thread exits with it.
struct AnsweringLoop {
    port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AnsweringLoop {
    fn start(fixtures: &[Fixture]) -> Result<Self, String> {
        // Truths are scoped per fixture, never pooled: the letter bed
        // repeats phrasings across letters by construction, and a
        // passage looked up globally would answer with the first
        // letter's party for every letter sharing the sentence. Each
        // request is one fixture's batch, so the fixture whose
        // expected segments best match the batch answers it.
        let mut truths: Vec<Vec<(String, Truth)>> = Vec::new();
        for fixture in fixtures {
            let mut scoped: Vec<(String, Truth)> = Vec::new();
            for want in &fixture.expected.obligations {
                scoped.push((
                    want.segment.clone(),
                    Truth::Obligations(want.expect.clone().into_iter().collect()),
                ));
            }
            for want in &fixture.expected.policy_terms {
                let truth = if want.review && want.expect.is_none() {
                    Truth::Terms(vec![referral_answer(fixture, want)?])
                } else {
                    Truth::Terms(want.expect.clone().into_iter().collect())
                };
                scoped.push((want.segment.clone(), truth));
            }
            if !scoped.is_empty() {
                truths.push(scoped);
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("oracle bind: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("oracle addr: {e}"))?
            .port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                let Some(prompt) = read_request_prompt(&mut stream) else {
                    continue;
                };
                let body = answer(&prompt, &truths);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Ok(Self { port, stop })
    }
}

impl Drop for AnsweringLoop {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // Wake the accept loop so it observes the flag and exits.
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// Read one HTTP request and return the prompt inside its JSON body.
fn read_request_prompt(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    let body_from = loop {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 && raw.is_empty() {
            return None;
        }
        raw.extend_from_slice(&buffer[..n]);
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if n == 0 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&raw[..body_from]).to_string();
    let content_length: usize = head
        .lines()
        .find_map(|line| {
            line.to_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    while raw.len() < body_from + content_length {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buffer[..n]);
    }
    let body: serde_json::Value =
        serde_json::from_slice(&raw[body_from..]).unwrap_or(serde_json::Value::Null);
    body["messages"][0]["content"].as_str().map(str::to_owned)
}

/// One batch question, answered from the truths. The batch items are
/// found by parsing the JSON array the prompt embeds — the same array
/// the executor rendered in — and each is answered by joining its
/// segment to the authored expectation for that passage.
fn answer(prompt: &str, fixtures: &[Vec<(String, Truth)>]) -> String {
    let items = batch_items(prompt);
    // The fixture this batch belongs to: the one whose authored
    // passages cover the most batch items. Ties break to the first,
    // deterministically.
    let truths: &[(String, Truth)] = fixtures
        .iter()
        .max_by_key(|scoped| {
            items
                .iter()
                .filter(|(_, segment)| {
                    scoped
                        .iter()
                        .any(|(passage, _)| same_squashed(passage, segment))
                })
                .count()
        })
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    // A segment the bed says nothing about still needs an answer in
    // the batch's own vocabulary, or the whole batch fails schema
    // validation over one silent passage.
    let terms_flavoured = items.iter().any(|(_, segment)| {
        matches!(
            truths
                .iter()
                .find(|(passage, _)| same_squashed(passage, segment)),
            Some((_, Truth::Terms(_)))
        )
    });
    let results: Vec<serde_json::Value> = items
        .iter()
        .map(|(id, segment)| {
            let truth = truths
                .iter()
                .find(|(passage, _)| same_squashed(passage, segment))
                .map(|(_, truth)| truth);
            match truth {
                Some(Truth::Obligations(expected)) => serde_json::json!({
                    "id": id,
                    "segment": segment,
                    "confidence": "high",
                    "obligations": expected
                        .iter()
                        .map(|expect| {
                            serde_json::json!({
                                "kind": expect.kind,
                                "party": expect.party,
                                "ask": "As the letter asks",
                                "deadline": expect.deadline,
                                "anchor": expect.anchor
                            })
                        })
                        .collect::<Vec<_>>()
                }),
                Some(Truth::Terms(expected)) => serde_json::json!({
                    "id": id,
                    "segment": segment,
                    "confidence": "high",
                    "terms": expected
                        .iter()
                        .map(|expect| {
                            serde_json::json!({
                                "term": expect.term,
                                "basis": expect.basis,
                                "value": expect.value,
                                "quote": expect.quote
                            })
                        })
                        .collect::<Vec<_>>()
                }),
                None if terms_flavoured => serde_json::json!({
                    "id": id,
                    "segment": segment,
                    "confidence": "high",
                    "terms": []
                }),
                None => serde_json::json!({
                    "id": id,
                    "segment": segment,
                    "confidence": "high",
                    "obligations": []
                }),
            }
        })
        .collect();
    serde_json::json!({
        "choices": [{ "message": {
            "role": "assistant",
            "content": serde_json::json!({ "results": results }).to_string()
        } }]
    })
    .to_string()
}

/// The batch array embedded in a rendered prompt: the first JSON array
/// in the text whose members are objects carrying `id` and `raw` — the
/// shape `exec::BatchItem` serialises into the prompt.
fn batch_items(prompt: &str) -> Vec<(u64, String)> {
    for (start, _) in prompt.match_indices('[') {
        let mut iter =
            serde_json::Deserializer::from_str(&prompt[start..]).into_iter::<serde_json::Value>();
        if let Some(Ok(serde_json::Value::Array(items))) = iter.next() {
            let parsed: Vec<(u64, String)> = items
                .iter()
                .filter_map(|item| {
                    Some((
                        item.get("id")?.as_u64()?,
                        item.get("raw")?.as_str()?.to_owned(),
                    ))
                })
                .collect();
            if !parsed.is_empty() && parsed.len() == items.len() {
                return parsed;
            }
        }
    }
    Vec::new()
}

/// Answer one authored referral expectation (#461): `review: true`
/// with no term named, because the passage's whole difficulty is that
/// no label is determinate. The honest reading is `other` — the escape
/// hatch every terms pack models (#445) — which the pack-coverage
/// guardrail routes to a person, exactly the referral the bed authors
/// as the correct outcome. A basis is borrowed from any authored
/// expectation in the same fixture, since it is guaranteed to be in
/// the pack's schema and a referred reading is never compared on it.
fn referral_answer(fixture: &Fixture, want: &TermExpectation) -> Result<ExpectedTerm, String> {
    let basis = fixture
        .expected
        .policy_terms
        .iter()
        .find_map(|other| other.expect.as_ref().map(|expect| expect.basis.clone()))
        .ok_or_else(|| {
            format!(
                "the referral expectation {} has no sibling term expectation to \
                 borrow a basis from",
                want.id
            )
        })?;
    Ok(ExpectedTerm {
        term: "other".to_owned(),
        basis,
        value: money_token(&want.segment).unwrap_or_else(|| want.segment.clone()),
        quote: want.segment.clone(),
    })
}

/// The first money-looking token in a passage — the value the bare
/// line states. Verbatim from the segment, so the quote (the segment
/// itself) always contains it, which #460's rule one requires.
fn money_token(segment: &str) -> Option<String> {
    let start = segment.find('£')?;
    let token: String = segment[start..]
        .chars()
        .take_while(|c| *c == '£' || c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let token = token.trim_end_matches(['.', ',']).to_owned();
    (token.len() > '£'.len_utf8()).then_some(token)
}

fn same_squashed(left: &str, right: &str) -> bool {
    let squash = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    squash(left) == squash(right)
}
