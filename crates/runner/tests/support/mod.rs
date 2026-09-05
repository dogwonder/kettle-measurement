//! Shared test support: a canned-response model server. No dependency —
//! CI needs no model, same posture as the sidecar's shell-script mocks.

use runner::exec::Endpoint;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;

/// A one-shot HTTP server: accepts a single connection, captures the
/// request, answers with a canned response. No dependency — the same
/// posture as the sidecar's shell-script mocks: CI needs no model.
// Each integration-test binary compiles this module separately, so an
// item unused by one binary is not dead code.
#[allow(dead_code)]
pub struct MockModel {
    port: u16,
    request: mpsc::Receiver<String>,
}

impl MockModel {
    #[allow(dead_code)]
    pub fn respond_once(status_line: &'static str, body: String) -> Self {
        Self::respond_sequence(vec![(status_line, body)])
    }

    /// A wedged sidecar: accept one connection, read the request,
    /// announce it on the `request` channel, then never answer — hold
    /// the connection open. This is how a cancellation arriving *while a
    /// model call is in flight* is exercised (#46): the client blocks
    /// reading a response that never comes, exactly as it would against
    /// a stalled llama-server.
    #[allow(dead_code)]
    pub fn hang() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock model");
        let port = listener.local_addr().expect("mock addr").port();
        let (send_request, request) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("mock accept");
            // Read as far as the end of the request headers: enough to
            // know the client has sent its request and settled into
            // waiting for a response we will never send.
            let mut raw = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let n = stream.read(&mut buffer).expect("mock read");
                raw.extend_from_slice(&buffer[..n]);
                if n == 0 || raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            send_request
                .send(String::from_utf8_lossy(&raw).into_owned())
                .ok();
            // Never respond. Park until the test process exits; the
            // point is a call that does not return on its own.
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        });
        MockModel { port, request }
    }

    /// Answer successive connections with successive canned responses —
    /// how the retry path is exercised without a model.
    pub fn respond_sequence(responses: Vec<(&'static str, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock model");
        let port = listener.local_addr().expect("mock addr").port();
        let (send_request, request) = mpsc::channel();
        std::thread::spawn(move || {
            for (status_line, body) in responses {
                let (mut stream, _) = listener.accept().expect("mock accept");
                let mut raw = Vec::new();
                let mut buffer = [0u8; 4096];
                // Read headers, then exactly content-length bytes of body.
                let body_from = loop {
                    let n = stream.read(&mut buffer).expect("mock read");
                    raw.extend_from_slice(&buffer[..n]);
                    if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let head = String::from_utf8_lossy(&raw[..body_from]).to_string();
                let content_length: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .map(str::to_owned)
                    })
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                while raw.len() < body_from + content_length {
                    let n = stream.read(&mut buffer).expect("mock read body");
                    raw.extend_from_slice(&buffer[..n]);
                }
                let request_body = String::from_utf8_lossy(&raw[body_from..]).to_string();
                send_request.send(request_body).ok();
                let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
                stream.write_all(response.as_bytes()).expect("mock respond");
            }
        });
        MockModel { port, request }
    }

    pub fn endpoint(&self) -> Endpoint {
        Endpoint::local(self.port)
    }

    #[allow(dead_code)]
    pub fn request_body(&self) -> String {
        self.request
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("mock never received a request")
    }
}

/// Wrap step output in the OpenAI-compatible completion envelope
/// llama-server answers with.
#[allow(dead_code)]
pub fn completion_envelope(content: &str) -> String {
    serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": content } }]
    })
    .to_string()
}

/// The envelope llama-server answers with when generation hit the
/// prediction limit: `finish_reason` "length", and content cut off
/// mid-answer.
#[allow(dead_code)]
pub fn truncated_envelope(content: &str) -> String {
    serde_json::json!({
        "choices": [{
            "finish_reason": "length",
            "message": { "role": "assistant", "content": content }
        }]
    })
    .to_string()
}

/// Documents pooled under one role are read one per batch (#624), so a
/// run over several letters makes one model call per letter. Tests
/// still write one `results` array numbered across the step, as the
/// model sees the ids; this deals one pooled envelope into an envelope
/// per batch, splitting before each id in `starts`.
#[allow(dead_code)]
pub fn per_batch(answer: &str, starts: &[usize]) -> Vec<(&'static str, String)> {
    let envelope: serde_json::Value = serde_json::from_str(answer).expect("envelope");
    let content = envelope["choices"][0]["message"]["content"]
        .as_str()
        .expect("content");
    let results: serde_json::Value = serde_json::from_str(content).expect("results");
    let results = results["results"].as_array().expect("array");
    let mut bounds = vec![0usize];
    bounds.extend_from_slice(starts);
    bounds.push(usize::MAX);
    bounds
        .windows(2)
        .map(|window| {
            let batch: Vec<serde_json::Value> = results
                .iter()
                .filter(|result| {
                    let id = result["id"].as_u64().expect("id") as usize;
                    id >= window[0] && id < window[1]
                })
                .cloned()
                .collect();
            (
                "200 OK",
                completion_envelope(&serde_json::json!({ "results": batch }).to_string()),
            )
        })
        .collect()
}
