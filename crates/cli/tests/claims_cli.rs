//! #434: `kettle claims` — the registry, rendered.
//!
//! Proven, unproven and failed are three different states and the table
//! must keep them apart; a reader skimming for trouble is exactly who a
//! blurred state misleads. Exit codes mirror `mutate`'s contract: 0 the
//! registry is a sound record (downgrades and failures are states, not
//! errors), 1 user-facing copy stands on a claim that is no longer
//! proven, 2 the registry itself cannot be trusted.

use std::path::PathBuf;

fn today() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 8, 9).expect("date")
}

/// A root this test owns, holding a registry and the evidence it names.
fn root(name: &str, registry: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-claims-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assurance")).expect("create root");
    std::fs::write(dir.join("assurance/claims.json"), registry).expect("write registry");
    dir
}

#[test]
fn the_three_states_render_distinctly_with_their_reasons() {
    let dir = root(
        "states",
        r##"{
          "claims": [
            {
              "id": "quotes-exist",
              "wording": "A quote absent from the source never becomes a finding.",
              "status": "proven",
              "scope": {},
              "evidence": [ { "kind": "test", "path": "tests/guard.rs", "name": "quote_guard_holds" } ],
              "recorded": "2026-08-09",
              "invalidation": ["guardrail change"],
              "surfaces": [],
              "review_route": "the owning test"
            },
            {
              "id": "network-quiet",
              "wording": "Task execution makes no non-loopback connection.",
              "status": "unproven",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 233 } ],
              "recorded": "2026-08-09",
              "invalidation": ["networking change"],
              "surfaces": [],
              "review_route": "#233"
            },
            {
              "id": "quotes-identify-values",
              "wording": "A quote is evidence only of a value it contains.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 460 } ],
              "recorded": "2026-08-09",
              "invalidation": ["guardrail change"],
              "surfaces": [],
              "review_route": "#460"
            }
          ]
        }"##,
    );
    std::fs::create_dir_all(dir.join("tests")).expect("create tests dir");
    std::fs::write(
        dir.join("tests/guard.rs"),
        "#[test]\nfn quote_guard_holds() {}\n",
    )
    .expect("write test file");

    let outcome = cli::claims::run(&dir, 12, today());

    assert_eq!(outcome.code, cli::claims::ExitCode::Ok, "{}", outcome.text);
    let text = &outcome.text;
    assert!(
        text.contains("proven") && text.contains("unproven") && text.contains("FAILED"),
        "all three states appear, and failed is loud: {text}"
    );
    // Each state sits on its own claim's line.
    let line_with = |id: &str| {
        text.lines()
            .find(|line| line.contains(id))
            .unwrap_or_else(|| panic!("{id} is rendered: {text}"))
            .to_owned()
    };
    assert!(line_with("quotes-exist").contains("proven"));
    assert!(!line_with("quotes-exist").contains("unproven"));
    assert!(line_with("network-quiet").contains("unproven"));
    assert!(line_with("quotes-identify-values").contains("FAILED"));

    // The public build consumes the same assessment as JSON. It is a
    // closed set, not a hand-selected list, and keeps all three states.
    let public = cli::claims::run_json(&dir, 12, today());
    assert_eq!(public.code, cli::claims::ExitCode::Ok, "{}", public.text);
    let document: serde_json::Value =
        serde_json::from_str(&public.text).expect("public claims are JSON");
    let claims = document["claims"].as_array().expect("claims array");
    assert_eq!(claims.len(), 3, "every registry claim is emitted");
    assert_eq!(claims[0]["status"], "proven");
    assert_eq!(claims[1]["status"], "unproven");
    assert_eq!(claims[2]["status"], "failed");
}

/// A claim downgraded by a scoring bump renders both halves of its
/// history — recorded proven, standing unproven — and the reason names
/// the versions. If a surface still quotes it, that is exit 1.
#[test]
fn stale_copy_is_named_and_exits_one() {
    let dir = root(
        "stale-copy",
        r#"{
          "claims": [
            {
              "id": "letter-ceilings",
              "wording": "Qwen3.5-4B clears the letter pack's harm ceilings.",
              "status": "proven",
              "scope": { "pack": "app.kttl.letter-to-actions", "pack_version": "0.2.0" },
              "evidence": [ { "kind": "baseline", "path": "evals/baseline.json" } ],
              "recorded": "2026-08-08",
              "invalidation": ["scoring version change"],
              "surfaces": ["app/copy.ts"],
              "review_route": "re-measure"
            }
          ]
        }"#,
    );
    std::fs::create_dir_all(dir.join("evals")).expect("create evals dir");
    std::fs::write(
        dir.join("evals/baseline.json"),
        r#"{
          "scoring_version": 12,
          "recorded_at": "2026-08-08T18:00:00Z",
          "reports": [
            { "pack": "app.kttl.letter-to-actions", "pack_version": "0.2.0", "eval_set": "development" }
          ]
        }"#,
    )
    .expect("write baseline");
    std::fs::create_dir_all(dir.join("app")).expect("create app dir");
    std::fs::write(dir.join("app/copy.ts"), "// quotes letter-ceilings\n").expect("write surface");

    let outcome = cli::claims::run(&dir, 13, today());

    assert_eq!(
        outcome.code,
        cli::claims::ExitCode::StaleCopy,
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("recorded proven"),
        "history is kept beside the verdict: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("app/copy.ts"),
        "the stale surface is named: {}",
        outcome.text
    );

    let public = cli::claims::run_json(&dir, 13, today());
    let document: serde_json::Value =
        serde_json::from_str(&public.text).expect("public claims are JSON");
    assert_eq!(document["claims"][0]["status"], "unproven");
    assert_eq!(document["claims"][0]["recorded_status"], "proven");
    assert!(document["claims"][0]["status_reasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty()));
}

#[test]
fn a_broken_registry_exits_two() {
    let dir = root(
        "broken",
        r#"{
          "claims": [
            {
              "id": "ghost",
              "wording": "Stands on a file nobody committed.",
              "status": "proven",
              "scope": {},
              "evidence": [ { "kind": "baseline", "path": "evals/never.json" } ],
              "recorded": "2026-08-09",
              "invalidation": ["anything"],
              "surfaces": [],
              "review_route": "none"
            }
          ]
        }"#,
    );

    let outcome = cli::claims::run(&dir, 12, today());
    assert_eq!(
        outcome.code,
        cli::claims::ExitCode::Broken,
        "{}",
        outcome.text
    );
    assert!(outcome.text.contains("never.json"), "{}", outcome.text);
}

/// #478: an issue citation is the one evidence kind a reader outside
/// the tracker cannot open, and the six in the committed registry sit
/// on the failed and unproven claims — precisely the rows the
/// falsifiers page exists to publish. The registry already holds what
/// each of those issues found, in the claim's `note`; the projection
/// simply did not emit it, so the public page could show "Issue #457"
/// and nothing else.
///
/// Publishing the note is the fix, rather than writing new prose: one
/// source, already reviewed, and it cannot drift from the record it is
/// drawn from. Optional, because a claim whose wording needs no gloss
/// should not be made to invent one.
#[test]
fn the_public_projection_carries_what_the_cited_issue_found() {
    let dir = root(
        "issue-note",
        r##"{
          "claims": [
            {
              "id": "renewal-v12-fail-verdict",
              "wording": "Qwen3.5-4B fails the renewal pack's harm gates.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 457 } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "re-measure at v13",
              "note": "The verdict rested on the eval's own join, not on the model."
            },
            {
              "id": "no-gloss-needed",
              "wording": "An action is only ever proposed.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 1 } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "the owning test"
            }
          ]
        }"##,
    );

    let outcome = cli::claims::run_json(&dir, runner::eval::SCORING_VERSION, today());
    let document: serde_json::Value =
        serde_json::from_str(&outcome.text).expect("the projection is JSON");
    let claims = document["claims"].as_array().expect("claims array");

    let cited = claims
        .iter()
        .find(|claim| claim["id"] == "renewal-v12-fail-verdict")
        .expect("the cited claim is projected");
    assert_eq!(
        cited["note"], "The verdict rested on the eval's own join, not on the model.",
        "a reader who cannot open issue 457 still learns what it found: {cited}"
    );

    let plain = claims
        .iter()
        .find(|claim| claim["id"] == "no-gloss-needed")
        .expect("the un-noted claim is projected");
    assert!(
        plain.get("note").is_none_or(serde_json::Value::is_null),
        "a claim with no note does not grow an empty one: {plain}"
    );
}

/// #478, after the flip. The measurement layer is public, so a path a
/// claim cites is a page a reader can open — but only where the
/// declared boundary publishes it, and only once the registry says
/// where the published tree lives. Both halves are the registry's to
/// state: a second list, or a hard-coded repository, is how the links
/// come to disagree with the tree they point into.
#[test]
fn published_evidence_carries_the_address_a_reader_can_open() {
    let dir = root(
        "evidence-links",
        r##"{
          "published_at": "https://github.com/dogwonder/kettle-measurement",
          "published": ["evals/", "crates/"],
          "claims": [
            {
              "id": "inside-the-boundary",
              "wording": "The letter pack meets its gates.",
              "status": "unproven",
              "scope": {},
              "evidence": [ { "kind": "baseline", "path": "evals/baseline-v14-letter.json" } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "the baseline"
            },
            {
              "id": "outside-the-boundary",
              "wording": "The shell refuses a pack asking for more than read.",
              "status": "unproven",
              "scope": {},
              "evidence": [
                { "kind": "test", "path": "app/src-tauri/tests/capabilities.rs", "name": "refuses_write" },
                { "kind": "issue", "number": 233 }
              ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "the owning test"
            }
          ]
        }"##,
    );

    let outcome = cli::claims::run_json(&dir, runner::eval::SCORING_VERSION, today());
    let document: serde_json::Value =
        serde_json::from_str(&outcome.text).expect("the projection is JSON");
    let claims = document["claims"].as_array().expect("claims array");

    assert_eq!(
        document["published_at"], "https://github.com/dogwonder/kettle-measurement",
        "the projection says where the published tree lives: {document}"
    );

    let inside = claims
        .iter()
        .find(|claim| claim["id"] == "inside-the-boundary")
        .expect("the in-boundary claim is projected");
    assert_eq!(
        inside["evidence"][0]["url"],
        "https://github.com/dogwonder/kettle-measurement/blob/main/evals/baseline-v14-letter.json",
        "a published path is an address, not a description: {inside}"
    );

    let outside = claims
        .iter()
        .find(|claim| claim["id"] == "outside-the-boundary")
        .expect("the out-of-boundary claim is projected");
    for (index, evidence) in outside["evidence"]
        .as_array()
        .expect("evidence array")
        .iter()
        .enumerate()
    {
        assert!(
            evidence.get("url").is_none_or(serde_json::Value::is_null),
            "evidence {index} is real here and unopenable there, so it carries no link: {evidence}"
        );
    }
}

/// A registry that has not declared where it is published links to
/// nothing. The boundary predates the flip and must stay valid for
/// anyone validating a tree that is not being published at all.
#[test]
fn an_undeclared_address_produces_no_links() {
    let dir = root(
        "no-address",
        r##"{
          "published": ["evals/"],
          "claims": [
            {
              "id": "inside-the-boundary",
              "wording": "The letter pack meets its gates.",
              "status": "unproven",
              "scope": {},
              "evidence": [ { "kind": "baseline", "path": "evals/baseline-v14-letter.json" } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "the baseline"
            }
          ]
        }"##,
    );

    let outcome = cli::claims::run_json(&dir, runner::eval::SCORING_VERSION, today());
    let document: serde_json::Value =
        serde_json::from_str(&outcome.text).expect("the projection is JSON");
    assert!(
        document
            .get("published_at")
            .is_none_or(serde_json::Value::is_null),
        "an undeclared address is absent, not empty: {document}"
    );
    assert!(
        document["claims"][0]["evidence"][0]
            .get("url")
            .is_none_or(serde_json::Value::is_null),
        "no address, no link: {document}"
    );
}
