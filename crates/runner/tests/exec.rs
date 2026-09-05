use runner::exec::{batch_items, render_prompt, BatchContext, BatchItem};
use std::path::Path;

fn pack_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn pack_prompt(name: &str) -> String {
    pack_file(&format!("prompts/{name}"))
}

#[test]
fn normalise_prompt_renders_batch_json() {
    let batch = [
        BatchItem::new(0, "SQ *KAFFA COFFEE"),
        BatchItem::new(1, "AMZNMktplace*2K4J"),
    ];

    let rendered =
        render_prompt(&pack_prompt("normalise.md"), &batch, None).expect("prompt renders");

    // The batch is rendered as ids Rust owns, so results can be rejoined
    // even if the model drops or reorders items.
    //
    // Asserted on the rendered batch, not on the prompt's prose: the
    // wording is the pack's to change and is guarded by `cli eval`
    // against fixtures (CLAUDE.md), so pinning it here would break this
    // plumbing test on every prompt edit and say nothing about the
    // plumbing.
    assert!(
        rendered.contains(
            r#"[
  {
    "id": 0,
    "raw": "SQ *KAFFA COFFEE"
  },
  {
    "id": 1,
    "raw": "AMZNMktplace*2K4J"
  }
]"#
        ),
        "batch not rendered as ids: {rendered}"
    );
    // The placeholder is replaced, not left standing in what reaches
    // the model.
    assert!(!rendered.contains("batch_json"), "{rendered}");
    // And the pack's own instructions still arrive with it.
    assert!(
        rendered.starts_with("You clean up merchant names"),
        "{rendered}"
    );
}

#[test]
fn classify_prompt_renders_examples() {
    let examples = pack_file("examples/classify.examples.json");
    let batch = [BatchItem::new(0, "Netflix")];

    let rendered = render_prompt(&pack_prompt("classify.md"), &batch, Some(&examples))
        .expect("prompt renders");

    // Few-shot examples go in verbatim — the pack owns this copy, Rust
    // only places it. They must reach the model demonstrating the id
    // echo, or the examples contradict the schema.
    assert!(
        rendered.contains(examples.trim()),
        "examples not rendered verbatim: {rendered}"
    );
    assert!(
        rendered.contains(r#""id": 0"#),
        "batch not rendered: {rendered}"
    );
    assert!(
        !rendered.contains("{{"),
        "template slot left unfilled: {rendered}"
    );
}

#[test]
fn prompt_without_examples_renders_clean() {
    // normalise.md never references {{ examples }}; supplying None must
    // not leave a stray slot or fail the render.
    let rendered = render_prompt(
        &pack_prompt("normalise.md"),
        &[BatchItem::new(0, "X")],
        None,
    )
    .expect("prompt renders");

    assert!(
        !rendered.contains("{{"),
        "template slot left unfilled: {rendered}"
    );
}

#[test]
fn items_split_into_batches_of_pack_size() {
    let items: Vec<String> = (0..45).map(|n| format!("MERCHANT {n}")).collect();

    let batches = batch_items(&items, 20);

    assert_eq!(
        batches.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![20, 20, 5],
        "batch sizes follow the pack's batch setting"
    );

    // Ids run across the whole step, not per batch — a result carrying
    // id 41 must be unambiguous no matter which batch produced it.
    let ids: Vec<usize> = batches.iter().flatten().map(|item| item.id).collect();
    assert_eq!(ids, (0..45).collect::<Vec<_>>());
    assert_eq!(batches[2][0], BatchItem::new(40, "MERCHANT 40"));
}

// ── #22: constrained calls + re-validation ──────────────────────────

use runner::exec::{assert_grammar_safe, call_constrained, Endpoint, ModelCallError};
use std::sync::atomic::AtomicBool;

mod support;
use std::net::TcpListener;
use support::{completion_envelope, MockModel};

fn results_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": { "results": { "type": "array" } },
        "required": ["results"]
    })
}

#[test]
fn response_violating_schema_is_rejected() {
    // The grammar constraint should make this impossible; re-validation
    // is belt-and-braces (brief §2), so a violation must surface as the
    // model's answer being invalid — never as a pass, never as a crash.
    let mock = MockModel::respond_once("200 OK", completion_envelope(r#"{"wrong": true}"#));

    let error = call_constrained(
        &mock.endpoint(),
        "prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect_err("schema violation must be rejected");

    let ModelCallError::Invalid { errors, content } = error else {
        panic!("expected Invalid, got: {error}");
    };
    // The validation errors travel with the rejection — #23 appends them
    // to the retry prompt.
    assert!(!errors.is_empty(), "validation errors must be carried");
    assert!(
        errors.iter().any(|e| e.contains("results")),
        "errors should name what's missing: {errors:?}"
    );
    assert_eq!(
        content, r#"{"wrong": true}"#,
        "the offending answer travels too"
    );
}

#[test]
fn valid_response_is_parsed_and_request_is_constrained() {
    let mock = MockModel::respond_once("200 OK", completion_envelope(r#"{"results": []}"#));

    let answer = call_constrained(
        &mock.endpoint(),
        "the rendered prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect("valid answer passes");
    assert_eq!(answer, serde_json::json!({ "results": [] }));

    // The request shape is the contract with llama-server (brief §3):
    // temperature 0, the prompt as the user message, and the step
    // schema under response_format.json_schema — never hand-written GBNF.
    let request: serde_json::Value =
        serde_json::from_str(&mock.request_body()).expect("request body is JSON");
    assert_eq!(request["temperature"], 0);
    assert_eq!(request["messages"][0]["role"], "user");
    assert_eq!(request["messages"][0]["content"], "the rendered prompt");
    assert_eq!(request["response_format"]["type"], "json_schema");
    assert_eq!(
        request["response_format"]["json_schema"]["schema"],
        results_schema()
    );
}

/// Every call carries a user turn, because a chat template is entitled
/// to insist on one. Kettle sent the whole rendered prompt as a lone
/// `system` message until 2 August 2026, which Qwen2.5's template
/// tolerates and Qwen3.5's refuses outright — `raise_exception('No user
/// query found in messages.')`, HTTP 400, before a single token is
/// generated. That is not a model failing a pack's job; it is Kettle
/// unable to ask the question, and it silently limited every
/// measurement to the one family whose template let it through.
///
/// So the shape is pinned rather than left to whichever weights were
/// last tried: one user message, no system message. A system-only
/// conversation is the outlier, and the model we can't run is the one
/// we can't rule out.
#[test]
fn a_call_carries_a_user_turn_because_a_template_may_demand_one() {
    let mock = MockModel::respond_once("200 OK", completion_envelope(r#"{"results": []}"#));

    call_constrained(
        &mock.endpoint(),
        "the rendered prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect("valid answer passes");

    let request: serde_json::Value =
        serde_json::from_str(&mock.request_body()).expect("request body is JSON");
    let messages = request["messages"]
        .as_array()
        .expect("messages is an array");
    assert!(
        messages
            .iter()
            .any(|message| message["role"] == "user" && message["content"] == "the rendered prompt"),
        "the rendered prompt must reach the model in a user turn, not only a system one: {messages:?}"
    );
}

/// #232: the request names its own output bound. llama-server's default
/// is unlimited generation, which is a policy nobody chose — and the one
/// under which Gemma 4 spent 1,700–2,300 hidden tokens per answer. An
/// explicit bound makes runaway generation a *recorded, bounded* event:
/// exhaustion comes back as `finish_reason` "length" and takes the
/// truncation path that already exists, instead of running until the
/// context fills.
#[test]
fn every_call_carries_an_explicit_answer_bound() {
    let mock = MockModel::respond_once("200 OK", completion_envelope(r#"{"results": []}"#));

    call_constrained(
        &mock.endpoint(),
        "the rendered prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect("valid answer passes");

    let request: serde_json::Value =
        serde_json::from_str(&mock.request_body()).expect("request body is JSON");
    assert_eq!(
        request["max_tokens"],
        serde_json::json!(runner::exec::MAX_ANSWER_TOKENS),
        "the answer bound must reach the server, and must be the same \
         constant the recorded runtime policy claims: {request}"
    );
    let bound = request["max_tokens"]
        .as_u64()
        .expect("the bound is a number");
    assert!(bound > 0, "a zero bound would refuse every answer");
}

/// The bound must never silently cut off an answer. A completion whose
/// remnant happens to be complete, schema-valid JSON is the dangerous
/// case: parsed first, it would pass validation and be used as though
/// the model had finished. `finish_reason` "length" says it did not,
/// and that verdict has to win — exhaustion reaches the truncation and
/// review path, never a quiet acceptance.
#[test]
fn a_bound_exhausted_answer_is_truncated_even_when_the_remnant_parses() {
    let mock = MockModel::respond_once("200 OK", truncated_envelope(r#"{"results": []}"#));

    let error = call_constrained(
        &mock.endpoint(),
        "prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect_err("a cut-off answer must not be accepted, however well it parses");

    assert!(
        matches!(error, ModelCallError::Truncated),
        "expected Truncated, got: {error}"
    );
}

// The three-way error taxonomy (brief §11: transport failure, server
// refusal, and an invalid answer need different handling in #23 and
// must never be conflated). These pin the split.

#[test]
fn dead_server_is_transport_not_invalid() {
    // Bind-then-drop guarantees a port with nothing listening.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let error = call_constrained(
        &Endpoint::local(port),
        "prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect_err("nothing is listening");
    assert!(
        matches!(error, ModelCallError::Transport(_)),
        "expected Transport, got: {error}"
    );
}

#[test]
fn server_refusal_is_distinct_from_transport() {
    // llama-server answers 400 when it can't turn a schema into a
    // grammar — the server said no; retrying the same request is
    // pointless, so this must not look like a flaky connection.
    let mock = MockModel::respond_once(
        "400 Bad Request",
        r#"{"error": {"message": "failed to convert schema"}}"#.to_owned(),
    );

    let error = call_constrained(
        &mock.endpoint(),
        "prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect_err("the server refused");
    let ModelCallError::Refused { status, message } = error else {
        panic!("expected Refused, got: {error}");
    };
    assert_eq!(status, 400);
    assert!(message.contains("failed to convert schema"));
}

#[test]
fn prose_content_is_invalid_not_a_crash() {
    // If the model answers prose instead of JSON, that is the model's
    // answer being bad — Invalid, so #23 can retry with the error
    // appended — never a parse crash and never Transport.
    let mock = MockModel::respond_once("200 OK", completion_envelope("Sorry, I cannot help."));

    let error = call_constrained(
        &mock.endpoint(),
        "prompt",
        &results_schema(),
        &AtomicBool::new(false),
    )
    .expect_err("prose is not a valid answer");
    let ModelCallError::Invalid { errors, content } = error else {
        panic!("expected Invalid, got: {error}");
    };
    assert!(errors[0].contains("not valid JSON"), "{errors:?}");
    assert_eq!(content, "Sorry, I cannot help.");
}

// ── #22: schemas must stay inside the grammar-convertible subset ────

#[test]
fn pack_schemas_are_grammar_safe() {
    // Both shipped schemas must stay inside the subset we know
    // llama-server converts to a grammar. Drift outside it is silent:
    // generation goes unconstrained while full-draft re-validation
    // still reports everything fine (§4a).
    for name in ["normalise", "classify"] {
        let schema: serde_json::Value =
            serde_json::from_str(&pack_file(&format!("schemas/{name}.schema.json")))
                .expect("schema parses");
        assert_grammar_safe(&schema).unwrap_or_else(|why| panic!("{name}: {why}"));
    }
}

#[test]
fn schema_outside_grammar_subset_is_rejected() {
    // Keywords llama.cpp's converter drops or mishandles must be
    // refused loudly at load, naming the keyword — the failure they
    // cause at run time is invisible.
    for (schema, keyword) in [
        (serde_json::json!({ "$ref": "#/defs/thing" }), "$ref"),
        (
            serde_json::json!({ "type": "object", "additionalProperties": false }),
            "additionalProperties",
        ),
        (
            serde_json::json!({ "type": "array", "minItems": 1 }),
            "minItems",
        ),
        (
            serde_json::json!({ "type": "object", "properties": { "deep": { "type": "array", "items": { "oneOf": [] } } } }),
            "oneOf",
        ),
    ] {
        let why = assert_grammar_safe(&schema).expect_err("must be rejected");
        assert!(why.contains(keyword), "should name {keyword}: {why}");
    }
}

// ── #23: retry-once → needs-review ──────────────────────────────────

use runner::exec::{run_batch, ReviewReason};

fn kaffa_batch() -> Vec<BatchItem> {
    vec![
        BatchItem::new(0, "SQ *KAFFA COFFEE"),
        BatchItem::new(1, "NETFLIX.COM"),
    ]
}

#[test]
fn twice_failed_batch_lands_in_needs_review() {
    // First answer violates the schema; the retry violates it again.
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(r#"{"wrong": true}"#)),
        ("200 OK", completion_envelope(r#"{"wrong": "again"}"#)),
    ]);

    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect("one bad batch never crashes a run");

    assert!(outcome.answers.is_empty(), "nothing usable came back");
    assert_eq!(
        outcome.needs_review.len(),
        2,
        "the whole batch goes to review"
    );
    for review in &outcome.needs_review {
        let ReviewReason::BatchFailedTwice { errors } = &review.reason else {
            panic!("expected BatchFailedTwice, got: {:?}", review.reason);
        };
        assert!(
            !errors.is_empty(),
            "the reason carries the validation errors"
        );
        assert_eq!(
            outcome.attempts[&review.item.id].len(),
            2,
            "both schema-invalid attempts remain diagnostic evidence"
        );
    }

    // The retry is not a blind re-ask: the first failure's validation
    // errors are appended so the model can correct them.
    let first = mock.request_body();
    let second = mock.request_body();
    assert!(!first.contains("did not match"), "first ask is clean");
    assert!(
        second.contains("results"),
        "retry quotes the validation error: {second}"
    );
    assert!(
        second.contains("did not match what was asked for"),
        "retry explains in plain language: {second}"
    );
}

#[test]
fn failed_batch_recovers_on_retry() {
    // First answer bad, corrected answer on the retry: the run carries
    // on as if nothing happened.
    let good = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee", "recognised": true},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(r#"{"wrong": true}"#)),
        ("200 OK", completion_envelope(good)),
    ]);
    let endpoint = mock.endpoint();

    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect("recovered batch");

    assert_eq!(outcome.answers.len(), 2);
    assert!(outcome.needs_review.is_empty());
    assert_eq!(outcome.answers[&1]["name"], "Netflix");
    assert_eq!(outcome.attempts[&0].len(), 2);
    assert_eq!(outcome.attempts[&1].len(), 2);
    assert_eq!(
        outcome.attempts[&0][0].schema,
        runner::claim_trace::CheckOutcome::Failed
    );
    assert_eq!(
        outcome.attempts[&0][1].schema,
        runner::claim_trace::CheckOutcome::Passed
    );
    assert_eq!(
        endpoint.take_metrics().retries.schema,
        1,
        "the retry reported by perf is the retry that actually happened"
    );
}

#[test]
fn retry_shows_the_model_its_previous_answer() {
    // The retry quotes validation errors whose JSON paths point into
    // the model's own answer — so the answer has to be in front of it
    // too, or it is being asked to fix a document it cannot see (#150).
    let good = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee", "recognised": true},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(r#"{"wrong": true}"#)),
        ("200 OK", completion_envelope(good)),
    ]);
    let endpoint = mock.endpoint();

    run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect("recovered batch");

    let first = mock.request_body();
    let retry: serde_json::Value =
        serde_json::from_str(&mock.request_body()).expect("retry request is JSON");
    let retry_prompt = retry["messages"][0]["content"]
        .as_str()
        .expect("retry prompt");
    assert!(
        !first.contains(r#"{\"wrong\": true}"#),
        "the first ask cannot know the answer before it exists"
    );
    assert!(
        retry_prompt.contains(r#"{"wrong": true}"#),
        "the retry must show the model its previous answer, got:\n{retry_prompt}"
    );
}

// ── #150: retries counted by reason ─────────────────────────────────

use runner::exec::RetryReason;

#[test]
fn retries_are_counted_by_reason() {
    // A retry is not one kind of event: a truncated answer, a
    // schema-invalid answer and a failed rejoin are different failures
    // with different fixes, and the report must be able to say which
    // happened. One of each must drain as one of each — and the total
    // must still be there for the consumers that only want a number.
    let endpoint = Endpoint::local(0);
    endpoint.record_retry(RetryReason::Truncation);
    endpoint.record_retry(RetryReason::Schema);
    endpoint.record_retry(RetryReason::Rejoin);

    let metrics = endpoint.take_metrics();
    assert_eq!(metrics.retries.truncation, 1);
    assert_eq!(metrics.retries.schema, 1);
    assert_eq!(metrics.retries.rejoin, 1);
    assert_eq!(metrics.retries.total(), 3);

    // Draining resets the counters, same as the timing fields.
    assert_eq!(endpoint.take_metrics().retries.total(), 0);
}

// ── #150: a truncated answer splits the batch ───────────────────────

use support::truncated_envelope;

fn four_merchants() -> Vec<BatchItem> {
    vec![
        BatchItem::new(0, "SQ *KAFFA COFFEE"),
        BatchItem::new(1, "NETFLIX.COM"),
        BatchItem::new(2, "BRITISH GAS"),
        BatchItem::new(3, "THAMES WATER"),
    ]
}

fn half_answer(items: &[(usize, &str, &str)]) -> String {
    let results: Vec<serde_json::Value> = items
        .iter()
        .map(|(id, raw, name)| serde_json::json!({ "id": id, "raw": raw, "name": name }))
        .collect();
    completion_envelope(&serde_json::json!({ "results": results }).to_string())
}

#[test]
fn truncation_splits_the_batch() {
    // finish_reason "length" means the answer ran out of room. At
    // temperature 0 a longer, error-annotated retry truncates
    // identically — so the batch must be halved and re-asked, never
    // sent down the Invalid-retry path.
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            truncated_envelope(r#"{"results": [{"id": 0, "raw": "SQ *KAF"#),
        ),
        (
            "200 OK",
            half_answer(&[
                (0, "SQ *KAFFA COFFEE", "Kaffa Coffee"),
                (1, "NETFLIX.COM", "Netflix"),
            ]),
        ),
        (
            "200 OK",
            half_answer(&[
                (2, "BRITISH GAS", "British Gas"),
                (3, "THAMES WATER", "Thames Water"),
            ]),
        ),
    ]);
    let endpoint = mock.endpoint();

    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("split batch rejoins");

    assert_eq!(outcome.answers.len(), 4, "all four answers rejoined");
    assert!(outcome.needs_review.is_empty());
    assert_eq!(outcome.answers[&0]["name"], "Kaffa Coffee");
    assert_eq!(outcome.answers[&3]["name"], "Thames Water");

    // The re-asks are the two halves, not one error-annotated retry of
    // the whole batch.
    let first = mock.request_body();
    assert!(
        first.contains("THAMES WATER"),
        "first ask is the full batch"
    );
    let second = mock.request_body();
    assert!(
        !second.contains("did not match"),
        "truncation must not take the Invalid-retry path: {second}"
    );
    assert!(second.contains("NETFLIX.COM") && !second.contains("BRITISH GAS"));
    let third = mock.request_body();
    assert!(third.contains("BRITISH GAS") && !third.contains("NETFLIX.COM"));

    // The split is a truncation retry, and only that.
    let retries = endpoint.take_metrics().retries;
    assert_eq!(retries.truncation, 1, "one split, one truncation retry");
    assert_eq!(retries.schema, 0);
}

#[test]
fn single_item_truncation_lands_in_needs_review() {
    // Floor of the split: one item that still truncates has nowhere
    // smaller to go, so it needs a person — with a reason that says why.
    let mock = MockModel::respond_once(
        "200 OK",
        truncated_envelope(r#"{"results": [{"id": 0, "raw": "SQ *KAF"#),
    );
    let endpoint = mock.endpoint();

    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &[BatchItem::new(0, "SQ *KAFFA COFFEE")],
        "raw",
        &BatchContext::none(),
    )
    .expect("a truncated item never crashes a run");

    assert!(outcome.answers.is_empty());
    assert_eq!(outcome.needs_review.len(), 1);
    assert!(matches!(
        outcome.needs_review[0].reason,
        ReviewReason::Truncated
    ));
    // No split happened, so no truncation retry is counted.
    assert_eq!(endpoint.take_metrics().retries.truncation, 0);
}

// ── #150: failed pairings get one targeted re-ask ───────────────────

#[test]
fn failed_pairing_is_reasked_once() {
    // One id missing, one echo mismatched: those two — and only those
    // two — are re-asked as a single targeted sub-batch, and a clean
    // second answer means nobody has to review anything.
    let partial = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee"},
        {"id": 2, "raw": "SOMEONE ELSE", "name": "Wrong Pairing"},
        {"id": 3, "raw": "THAMES WATER", "name": "Thames Water"}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(partial)),
        (
            "200 OK",
            half_answer(&[
                (1, "NETFLIX.COM", "Netflix"),
                (2, "BRITISH GAS", "British Gas"),
            ]),
        ),
    ]);
    let endpoint = mock.endpoint();

    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("re-asked batch rejoins");

    assert_eq!(outcome.answers.len(), 4, "all four answers rejoined");
    assert!(outcome.needs_review.is_empty());
    assert_eq!(outcome.answers[&1]["name"], "Netflix");
    assert_eq!(outcome.answers[&2]["name"], "British Gas");

    // The re-ask carries only the failed items, with a note saying what
    // went wrong last time.
    let first = mock.request_body();
    assert!(!first.contains("exactly once"), "first ask is clean");
    let second = mock.request_body();
    assert!(
        second.contains("NETFLIX.COM") && second.contains("BRITISH GAS"),
        "the failed items are re-asked: {second}"
    );
    assert!(
        !second.contains("THAMES WATER"),
        "answered items are not re-asked: {second}"
    );
    assert!(
        second.contains("exactly once"),
        "the re-ask says what is needed this time: {second}"
    );

    // The re-ask is a rejoin retry, and only that.
    let retries = endpoint.take_metrics().retries;
    assert_eq!(retries.rejoin, 1);
    assert_eq!(retries.schema, 0);
    assert_eq!(retries.truncation, 0);
}

#[test]
fn rejoin_routes_each_problem_to_review() {
    // One good answer, one id missing, one echo mismatch. The failed
    // pairings are re-asked once (#150); when that re-ask fails them
    // again they go to review with their original reasons — a partial
    // batch is normal, not an error.
    let partial = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee", "recognised": true},
        {"id": 2, "raw": "SOMEONE ELSE", "name": "Wrong Pairing", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(partial)),
        // The re-ask answers neither item: both fail pairing again.
        ("200 OK", completion_envelope(r#"{"results": []}"#)),
    ]);
    let endpoint = mock.endpoint();

    let batch = vec![
        BatchItem::new(0, "SQ *KAFFA COFFEE"),
        BatchItem::new(1, "NETFLIX.COM"),
        BatchItem::new(2, "BRITISH GAS"),
    ];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("partial batch is fine");

    assert_eq!(outcome.answers.len(), 1);
    assert!(outcome.answers.contains_key(&0));

    assert_eq!(outcome.needs_review.len(), 2);
    let for_item = |id: usize| {
        outcome
            .needs_review
            .iter()
            .find(|r| r.item.id == id)
            .unwrap_or_else(|| panic!("item {id} should be in review"))
    };
    assert!(matches!(
        for_item(1).reason,
        ReviewReason::MissingFromResults
    ));
    // The mismatched echo keeps its original reason — the re-ask that
    // also failed must not overwrite what the report can say about it.
    let ReviewReason::MismatchedEcho { echoed } = &for_item(2).reason else {
        panic!("item 2 echoed the wrong input");
    };
    assert_eq!(echoed, "SOMEONE ELSE");

    // The re-ask happened, once.
    assert_eq!(endpoint.take_metrics().retries.rejoin, 1);
}

#[test]
fn low_confidence_triggers_no_reask() {
    // Low confidence is the model being honest, not wrong — re-asking
    // would be badgering it for a confident-sounding guess. The mock
    // serves exactly one response, so a second call fails the run.
    let unsure = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee", "confidence": "high"},
        {"id": 1, "raw": "NETFLIX.COM", "name": "J Henderson Windows", "confidence": "low"}
    ]}"#;
    let mock = MockModel::respond_once("200 OK", completion_envelope(unsure));
    let endpoint = mock.endpoint();

    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect("low confidence alone must not trigger a second call");

    assert_eq!(outcome.answers.len(), 1);
    assert!(matches!(
        outcome.needs_review[0].reason,
        ReviewReason::LowConfidence
    ));
    assert_eq!(endpoint.take_metrics().retries.rejoin, 0);
}

#[test]
fn low_confidence_answer_is_shown_not_used() {
    let unsure = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee", "confidence": "high"},
        {"id": 1, "raw": "NETFLIX.COM", "name": "J Henderson Windows", "confidence": "low"}
    ]}"#;
    let mock = MockModel::respond_once("200 OK", completion_envelope(unsure));

    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect("low confidence is not a failure");

    assert_eq!(outcome.answers.len(), 1, "the confident answer is used");
    let review = &outcome.needs_review[0];
    assert!(matches!(review.reason, ReviewReason::LowConfidence));
    // The guess travels with the review item so the report can show it
    // in "check these yourself" — shown, never silently used.
    assert_eq!(
        review.answer.as_ref().expect("guess kept")["name"],
        "J Henderson Windows"
    );
}

#[test]
fn transport_failure_stops_the_step_not_the_bucket() {
    // A dead server is a run-level problem: routing the batch to
    // needs-review would misreport "the model was unsure" when the
    // truth is "the model was unreachable".
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let error = run_batch(
        &Endpoint::local(port),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &kaffa_batch(),
        "raw",
        &BatchContext::none(),
    )
    .expect_err("unreachable server is an error, not a review item");
    assert!(matches!(
        error,
        runner::exec::StepError::Call(ModelCallError::Transport(_))
    ));
}

// ---------------------------------------------------------------------------
// #312: an echo pairs on prefix uniqueness, not on being verbatim

/// The saving the #283 spike endorsed: the verbatim `segment` echo is
/// ~25% of what the letter pack generates, and shortening it is worth
/// ~20% of a bed run across every pack.
///
/// It costs the pairing check nothing. The echo was never a source of
/// data — the runner already holds every item's text — and a prefix
/// unique within its batch is a bijection onto that batch's items,
/// which is all equality ever bought.
#[test]
fn a_unique_prefix_pairs_an_answer_to_its_item() {
    let short = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA", "name": "Kaffa Coffee", "recognised": true},
        {"id": 1, "raw": "NETFLIX", "name": "Netflix", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![("200 OK", completion_envelope(short))]);
    let endpoint = mock.endpoint();

    let batch = vec![
        BatchItem::new(0, "SQ *KAFFA COFFEE"),
        BatchItem::new(1, "NETFLIX.COM"),
    ];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("a batch answered by prefix is fine");

    assert_eq!(outcome.answers.len(), 2, "{:?}", outcome.needs_review);
    assert!(outcome.needs_review.is_empty());
    assert_eq!(endpoint.take_metrics().retries.rejoin, 0);
}

/// The guard the whole change rests on.
///
/// Deciding uniqueness per batch, in Rust, is what makes a short echo
/// safe on real documents. A hardcoded "three words" would be tuned to
/// today's bed and would silently weaken on a letter full of repetitive
/// boilerplate. An ambiguous prefix degrades into review instead, which
/// is the honest failure — and it matters here because where two
/// passages do differ, the discriminating detail is usually at the
/// *end* ("within 21 days" against "within 42 days"), exactly where a
/// leading prefix is blindest.
#[test]
fn an_ambiguous_prefix_is_a_mismatched_echo() {
    let ambiguous = r#"{"results": [
        {"id": 0, "raw": "PAYMENT OF", "name": "Guessing", "recognised": true},
        {"id": 1, "raw": "PAYMENT OF", "name": "Guessing", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(ambiguous)),
        ("200 OK", completion_envelope(r#"{"results": []}"#)),
    ]);
    let endpoint = mock.endpoint();

    let batch = vec![
        BatchItem::new(0, "PAYMENT OF £21.00 IS DUE"),
        BatchItem::new(1, "PAYMENT OF £42.00 IS DUE"),
    ];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("an ambiguous batch still returns");

    assert!(
        outcome.answers.is_empty(),
        "a prefix matching two different texts pairs with neither"
    );
    assert_eq!(outcome.needs_review.len(), 2);
    for review in &outcome.needs_review {
        assert!(
            matches!(review.reason, ReviewReason::MismatchedEcho { .. }),
            "{:?}",
            review.reason
        );
    }
}

/// Where two items are byte-identical, no echo of any length
/// distinguishes them — including today's full echo — and none needs to:
/// swapping their answers produces the same answer. The #283 spike found
/// 88% of the letter bed's segments are non-unique across letters, so
/// this is not hypothetical and must not read as a mismatch.
#[test]
fn identical_items_are_not_an_ambiguous_pairing() {
    let same = r#"{"results": [
        {"id": 0, "raw": "THANK YOU", "name": "Courtesy", "recognised": true},
        {"id": 1, "raw": "THANK YOU", "name": "Courtesy", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![("200 OK", completion_envelope(same))]);
    let endpoint = mock.endpoint();

    let batch = vec![
        BatchItem::new(0, "THANK YOU FOR YOUR PAYMENT"),
        BatchItem::new(1, "THANK YOU FOR YOUR PAYMENT"),
    ];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("identical items are fine");

    assert_eq!(outcome.answers.len(), 2, "{:?}", outcome.needs_review);
    assert!(outcome.needs_review.is_empty());
}

/// A missing echo field reads as an empty string, which is a prefix of
/// everything. Pairing rules alone would let it through on a
/// single-item batch, where there is nothing to pair against — so an
/// echo must still be *something*. The model not answering the field is
/// not evidence that it read the right item.
#[test]
fn an_empty_echo_never_pairs_even_with_one_item_in_the_batch() {
    let nothing = r#"{"results": [
        {"id": 0, "name": "No echo at all", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(nothing)),
        ("200 OK", completion_envelope(r#"{"results": []}"#)),
    ]);
    let endpoint = mock.endpoint();

    let batch = vec![BatchItem::new(0, "SQ *KAFFA COFFEE")];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("the batch returns");

    assert!(outcome.answers.is_empty());
    assert_eq!(outcome.needs_review.len(), 1);
    assert!(matches!(
        outcome.needs_review[0].reason,
        ReviewReason::MismatchedEcho { .. }
    ));
}

/// An exact echo is conclusive, even when another item extends it.
///
/// `TESCO` and `TESCO STORES` in one batch: a verbatim echo of the
/// first is a prefix of the second, so a pure uniqueness rule would
/// reject it and send a correctly paired answer to review. It pairs
/// today and must keep pairing — #312 is a relaxation of the echo
/// check, and a relaxation that newly rejects anything is not one.
///
/// Principled rather than grandfathered: a short prefix needs
/// uniqueness because it is weak evidence about which item was read. An
/// exact match is not a prefix of the item, it *is* the item, which is
/// the strongest evidence there is.
#[test]
fn an_exact_echo_pairs_even_when_another_item_extends_it() {
    let exact = r#"{"results": [
        {"id": 0, "raw": "TESCO", "name": "Tesco", "recognised": true},
        {"id": 1, "raw": "TESCO STORES", "name": "Tesco", "recognised": true}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![("200 OK", completion_envelope(exact))]);
    let endpoint = mock.endpoint();

    let batch = vec![
        BatchItem::new(0, "TESCO"),
        BatchItem::new(1, "TESCO STORES"),
    ];
    let outcome = run_batch(
        &endpoint,
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &batch,
        "raw",
        &BatchContext::none(),
    )
    .expect("exact echoes pair");

    assert_eq!(outcome.answers.len(), 2, "{:?}", outcome.needs_review);
    assert!(outcome.needs_review.is_empty());
}

/// Every retained answer says which ids were in the request that
/// produced it (review of #626): the batch it was first asked in is
/// not the window once a retry or a split has re-asked it.
#[test]
fn a_reasked_answer_is_shown_the_reask_and_not_the_original_batch() {
    let partial = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee"},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix"}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(partial)),
        (
            "200 OK",
            half_answer(&[
                (2, "BRITISH GAS", "British Gas"),
                (3, "THAMES WATER", "Thames Water"),
            ]),
        ),
    ]);
    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("re-asked batch rejoins");

    let set = |ids: &[usize]| {
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        outcome.shown[&0],
        set(&[0, 1, 2, 3]),
        "answered in the original batch"
    );
    assert_eq!(outcome.shown[&2], set(&[2, 3]), "answered in the re-ask");
    assert_eq!(outcome.shown[&3], set(&[2, 3]));
}

#[test]
fn a_noncontiguous_reask_is_shown_exactly_its_ids() {
    let partial = r#"{"results": [
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix"},
        {"id": 3, "raw": "THAMES WATER", "name": "Thames Water"}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(partial)),
        (
            "200 OK",
            half_answer(&[
                (0, "SQ *KAFFA COFFEE", "Kaffa Coffee"),
                (2, "BRITISH GAS", "British Gas"),
            ]),
        ),
    ]);
    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("re-asked batch rejoins");

    let set = |ids: &[usize]| {
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        outcome.shown[&0],
        set(&[0, 2]),
        "a range 0..3 would vouch for id 1"
    );
    assert_eq!(outcome.shown[&2], set(&[0, 2]));
    assert_eq!(outcome.shown[&1], set(&[0, 1, 2, 3]));
}

#[test]
fn a_split_answer_is_shown_its_own_half() {
    let mock = MockModel::respond_sequence(vec![
        (
            "200 OK",
            truncated_envelope(r#"{"results": [{"id": 0, "raw": "SQ *KAF"#),
        ),
        (
            "200 OK",
            half_answer(&[
                (0, "SQ *KAFFA COFFEE", "Kaffa Coffee"),
                (1, "NETFLIX.COM", "Netflix"),
            ]),
        ),
        (
            "200 OK",
            half_answer(&[
                (2, "BRITISH GAS", "British Gas"),
                (3, "THAMES WATER", "Thames Water"),
            ]),
        ),
    ]);
    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("split batch rejoins");

    let set = |ids: &[usize]| {
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(outcome.shown[&0], set(&[0, 1]));
    assert_eq!(outcome.shown[&1], set(&[0, 1]));
    assert_eq!(outcome.shown[&2], set(&[2, 3]));
    assert_eq!(outcome.shown[&3], set(&[2, 3]));
}

/// A low-confidence answer is retained for review, so it carries
/// provenance too; a pairing failure the re-ask did not resolve keeps
/// the original entry and the original request's ids with it.
#[test]
fn review_entries_carry_the_request_that_produced_them() {
    let partial = r#"{"results": [
        {"id": 0, "raw": "SQ *KAFFA COFFEE", "name": "Kaffa Coffee"},
        {"id": 1, "raw": "NETFLIX.COM", "name": "Netflix", "confidence": "low"},
        {"id": 2, "raw": "SOMEONE ELSE", "name": "Wrong Pairing"}
    ]}"#;
    let reask = r#"{"results": [
        {"id": 2, "raw": "SOMEONE ELSE AGAIN", "name": "Still Wrong"}
    ]}"#;
    let mock = MockModel::respond_sequence(vec![
        ("200 OK", completion_envelope(partial)),
        ("200 OK", completion_envelope(reask)),
    ]);
    let outcome = run_batch(
        &mock.endpoint(),
        "Sort these:\n{{ batch_json }}",
        None,
        &results_schema(),
        &four_merchants(),
        "raw",
        &BatchContext::none(),
    )
    .expect("batch rejoins");

    let set = |ids: &[usize]| {
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        outcome.shown[&1],
        set(&[0, 1, 2, 3]),
        "low confidence, from the original"
    );
    // Ids 2 and 3 failed pairing twice; their review entries are the
    // originals, and so is their provenance.
    assert_eq!(outcome.shown[&2], set(&[0, 1, 2, 3]));
    assert_eq!(outcome.shown[&3], set(&[0, 1, 2, 3]));
    for review in &outcome.needs_review {
        assert!(
            outcome.shown.contains_key(&review.item.id),
            "every review entry has provenance"
        );
    }
}
