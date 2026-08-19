//! #434: the assurance-claims registry — one reviewable place where a
//! product-level claim names the evidence it stands on and when it
//! stops applying.
//!
//! The failure this exists to catch is a sentence outliving its
//! measurement. The worked example is real: the renewal pack's v12 FAIL
//! verdict was a live, quotable claim that turned out to rest on the
//! eval's own broken join (#457), not on the model. A registry entry
//! carries its scope and its evidence; validation re-derives what the
//! claim is worth today, and never preserves "proven" on evidence the
//! current harness no longer stands behind.

use runner::assurance::{Registry, Status};
use std::path::PathBuf;

/// Any fixed date works: everything date-sensitive in these tests
/// declares its own dates relative to this one.
fn today() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 8, 9).expect("date")
}

/// A directory this test owns outright. Unique per process so parallel
/// test runs — including another session's — can never share it.
fn evidence_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-assurance-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("evals")).expect("create evidence root");
    dir
}

/// The issue's named first test. A claim naming the current letter pack
/// stands on a baseline recorded against an earlier pack version: the
/// evidence is real, inspectable, and about a different pack than the
/// claim is about. Validation must return unproven with both versions
/// named — never preserve the recorded "proven".
#[test]
fn a_current_model_claim_cannot_stand_on_stale_pack_evidence() {
    let root = evidence_root("stale-pack");
    // A baseline whose numbers are honest — recorded at the current
    // scoring version, so nothing about the harness has moved — but
    // measured against pack 0.1.0, one authored bed ago.
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-01T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.letter-to-actions",
              "pack_version": "0.1.0",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf", "params": "4B", "quant": "Q4_K_M", "context": 8192 }},
              "verdict": "pass"
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-old-letter.json"), baseline).expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "letter-harm-ceilings",
              "wording": "Qwen3.5-4B clears the letter pack's harm ceilings.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.letter-to-actions",
                "pack_version": "0.2.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-old-letter.json" }
              ],
              "recorded": "2026-08-08",
              "invalidation": [
                "pack version change",
                "scoring version change",
                "model or sidecar change"
              ],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(
        assessment.refusals.is_empty(),
        "a version mismatch is a downgrade, not a structural refusal: {:?}",
        assessment.refusals
    );
    let claim = assessment
        .claims
        .iter()
        .find(|claim| claim.id == "letter-harm-ceilings")
        .expect("the claim was assessed");
    assert_eq!(
        claim.effective,
        Status::Unproven,
        "proven must not survive evidence about a different pack version"
    );
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("0.1.0") && reasons.contains("0.2.0"),
        "the mismatched versions are named, so the reader can see what \
         to re-measure: {reasons}"
    );
}

/// The live case this lands into: `evals/baseline-v12-*.json` are on
/// disk now, and the moment #457's SCORING_VERSION 13 merges, every
/// claim standing on them ages in place. A scoring bump must downgrade,
/// with both versions named — and must not fail the registry, because
/// evidence goes stale on someone else's merge.
#[test]
fn a_scoring_version_bump_downgrades_every_claim_standing_on_the_old_evidence() {
    let root = evidence_root("stale-scoring");
    let baseline = r#"{
      "scoring_version": 12,
      "recorded_at": "2026-08-08T18:04:48Z",
      "reports": [
        {
          "pack": "app.kttl.renewal-diff",
          "pack_version": "0.1.0",
          "eval_set": "development",
          "model": { "file": "qwen3.5-4b-q4_k_m.gguf" },
          "verdict": "fail"
        }
      ]
    }"#;
    std::fs::write(root.join("evals/baseline-v12-renewal.json"), baseline).expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "renewal-v12-verdict",
              "wording": "Qwen3.5-4B fails the renewal pack's gates.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.renewal-diff",
                "pack_version": "0.1.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-v12-renewal.json" }
              ],
              "recorded": "2026-08-08",
              "invalidation": ["scoring version change"],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, 13, today());

    assert!(
        assessment.refusals.is_empty(),
        "stale scoring evidence downgrades; it must never fail the registry: {:?}",
        assessment.refusals
    );
    let claim = &assessment.claims[0];
    assert_eq!(claim.effective, Status::Unproven);
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("12") && reasons.contains("13"),
        "both scoring versions are named: {reasons}"
    );
}

/// A proven claim with no invalidation triggers is a registry defect,
/// not a stale claim: nothing could ever stop it applying, which makes
/// it a slogan wearing a measurement's clothes. Refused, so CI fails
/// until someone says what would invalidate it.
#[test]
fn a_proven_claim_with_no_invalidation_triggers_is_refused() {
    let root = evidence_root("no-triggers");
    std::fs::write(
        root.join("evals/baseline.json"),
        format!(
            r#"{{ "scoring_version": {}, "recorded_at": "2026-08-01T12:00:00Z", "reports": [] }}"#,
            runner::eval::SCORING_VERSION
        ),
    )
    .expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "eternal",
              "wording": "This claim can never stop being true.",
              "status": "proven",
              "scope": {},
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline.json" }
              ],
              "recorded": "2026-08-08",
              "invalidation": [],
              "surfaces": [],
              "review_route": "none"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("eternal") && refusal.contains("invalidation")),
        "the refusal names the claim and the missing triggers: {:?}",
        assessment.refusals
    );
}

/// An issue reference is where a finding is discussed, not where it is
/// measured. A proven claim standing on nothing but an issue number is
/// not evidenced, and that is a broken registry — whereas a failed
/// claim may stand on exactly that, because the issue documents the
/// failure. #460 and #461 enter the committed registry this way.
#[test]
fn a_proven_claim_standing_only_on_an_issue_is_refused_and_a_failed_one_is_not() {
    let root = evidence_root("issue-evidence");
    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "hopeful",
              "wording": "The pipeline is perfect; see the discussion.",
              "status": "proven",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 424 } ],
              "recorded": "2026-08-08",
              "invalidation": ["any code change"],
              "surfaces": [],
              "review_route": "none"
            },
            {
              "id": "quote-identifies-its-value",
              "wording": "A quote identifies the value it evidences.",
              "status": "failed",
              "scope": { "pack": "app.kttl.renewal-diff" },
              "evidence": [ { "kind": "issue", "number": 460 } ],
              "recorded": "2026-08-09",
              "invalidation": ["quote guardrail change"],
              "surfaces": [],
              "review_route": "reopen #460"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("hopeful")),
        "proven-on-an-issue is refused: {:?}",
        assessment.refusals
    );
    assert_eq!(
        assessment.refusals.len(),
        1,
        "the failed claim on the same evidence kind is fine: {:?}",
        assessment.refusals
    );
    let failed = assessment
        .claims
        .iter()
        .find(|claim| claim.id == "quote-identifies-its-value")
        .expect("assessed");
    assert_eq!(failed.effective, Status::Failed);
}

/// Not every claim can be proven in CI — the packaged network boundary
/// is measured by a release check on a real build, not a unit test.
/// That is "not automatable", which must stay distinct from "not
/// evidenced": a manual record with a date and an expiry carries a
/// proven claim while it is in date, and downgrades it — naming the
/// expiry — the day it is not.
#[test]
fn manual_release_evidence_carries_a_proven_claim_until_it_expires() {
    let root = evidence_root("manual-evidence");
    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "no-network-in-date",
              "wording": "Task execution makes no non-loopback connection.",
              "status": "proven",
              "scope": { "platform": "macOS" },
              "evidence": [
                {
                  "kind": "manual",
                  "description": "Packaged-build network capture, all packs run once",
                  "recorded": "2026-08-01",
                  "expires": "2026-09-01"
                }
              ],
              "recorded": "2026-08-01",
              "invalidation": ["dependency or networking change", "packaging change"],
              "surfaces": [],
              "review_route": "repeat the packaged capture (#233)"
            },
            {
              "id": "no-network-expired",
              "wording": "Task execution makes no non-loopback connection.",
              "status": "proven",
              "scope": { "platform": "windows" },
              "evidence": [
                {
                  "kind": "manual",
                  "description": "Packaged-build network capture, all packs run once",
                  "recorded": "2026-05-01",
                  "expires": "2026-06-01"
                }
              ],
              "recorded": "2026-05-01",
              "invalidation": ["dependency or networking change", "packaging change"],
              "surfaces": [],
              "review_route": "repeat the packaged capture (#233)"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(assessment.refusals.is_empty(), "{:?}", assessment.refusals);
    let in_date = assessment
        .claims
        .iter()
        .find(|claim| claim.id == "no-network-in-date")
        .expect("assessed");
    assert_eq!(
        in_date.effective,
        Status::Proven,
        "in-date manual evidence is evidence: {:?}",
        in_date.reasons
    );
    let expired = assessment
        .claims
        .iter()
        .find(|claim| claim.id == "no-network-expired")
        .expect("assessed");
    assert_eq!(expired.effective, Status::Unproven);
    assert!(
        expired.reasons.join("\n").contains("2026-06-01"),
        "the expiry is named: {:?}",
        expired.reasons
    );
}

/// A claim may stand on its owning test — that is what "quote existence
/// is enforced" stands on — but only if the named test exists in the
/// named file. A test that has been renamed or deleted leaves the claim
/// pointing at nothing, and a pointer at nothing is a broken registry,
/// not a stale claim.
#[test]
fn a_claim_standing_on_a_test_is_refused_when_the_test_is_not_in_the_file() {
    let root = evidence_root("test-evidence");
    std::fs::create_dir_all(root.join("tests")).expect("create tests dir");
    std::fs::write(
        root.join("tests/quotes.rs"),
        "#[test]\nfn a_quote_absent_from_the_source_is_refused() {}\n",
    )
    .expect("write test file");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "quote-existence",
              "wording": "A quoted passage absent from the source never becomes a finding.",
              "status": "proven",
              "scope": {},
              "evidence": [
                { "kind": "test", "path": "tests/quotes.rs", "name": "a_quote_absent_from_the_source_is_refused" }
              ],
              "recorded": "2026-08-09",
              "invalidation": ["quote guardrail change"],
              "surfaces": [],
              "review_route": "the test owns it"
            },
            {
              "id": "quote-existence-renamed",
              "wording": "The same claim, standing on a test that no longer exists.",
              "status": "proven",
              "scope": {},
              "evidence": [
                { "kind": "test", "path": "tests/quotes.rs", "name": "a_test_that_was_renamed_away" }
              ],
              "recorded": "2026-08-09",
              "invalidation": ["quote guardrail change"],
              "surfaces": [],
              "review_route": "the test owns it"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("quote-existence-renamed")
                && refusal.contains("a_test_that_was_renamed_away")),
        "the refusal names the claim and the missing test: {:?}",
        assessment.refusals
    );
    assert_eq!(assessment.refusals.len(), 1, "{:?}", assessment.refusals);
    let sound = assessment
        .claims
        .iter()
        .find(|claim| claim.id == "quote-existence")
        .expect("assessed");
    assert_eq!(sound.effective, Status::Proven, "{:?}", sound.reasons);
}

/// Two claims that cannot both hold, both effectively proven, is a
/// refusal: the registry is asserting nonsense and cannot say which
/// half. The relation is declared, not inferred — wording is for
/// people, and guessing contradiction from prose is how a validator
/// becomes a judge. A proven claim contradicting a failed one is fine;
/// that pair is the registry doing its job.
#[test]
fn two_contradictory_claims_cannot_both_stand_proven() {
    let root = evidence_root("contradiction");
    std::fs::create_dir_all(root.join("tests")).expect("create tests dir");
    std::fs::write(
        root.join("tests/guards.rs"),
        "#[test]\nfn quotes_are_verbatim() {}\n#[test]\nfn quotes_identify_values() {}\n",
    )
    .expect("write test file");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "quotes-suffice",
              "wording": "A verbatim quote is sufficient evidence for the value it accompanies.",
              "status": "proven",
              "scope": {},
              "evidence": [
                { "kind": "test", "path": "tests/guards.rs", "name": "quotes_are_verbatim" }
              ],
              "recorded": "2026-08-09",
              "invalidation": ["quote guardrail change"],
              "surfaces": [],
              "review_route": "none",
              "contradicts": ["quotes-do-not-suffice"]
            },
            {
              "id": "quotes-do-not-suffice",
              "wording": "A verbatim quote can evidence a value it does not contain.",
              "status": "proven",
              "scope": {},
              "evidence": [
                { "kind": "test", "path": "tests/guards.rs", "name": "quotes_identify_values" }
              ],
              "recorded": "2026-08-09",
              "invalidation": ["quote guardrail change"],
              "surfaces": [],
              "review_route": "none"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("quotes-suffice")
                && refusal.contains("quotes-do-not-suffice")),
        "the refusal names both halves: {:?}",
        assessment.refusals
    );
}

/// The registry's own joints: an id that appears twice is two claims
/// answering to one name, and a `contradicts` naming no claim is a
/// relation to nothing. Both are refusals — every later rule joins on
/// ids, so a registry loose about them cannot be validated at all.
#[test]
fn duplicate_ids_and_dangling_contradictions_are_refused() {
    let root = evidence_root("integrity");
    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "twice",
              "wording": "First of two claims under one name.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 1 } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "none"
            },
            {
              "id": "twice",
              "wording": "Second of two claims under one name.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 2 } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": [],
              "review_route": "none",
              "contradicts": ["nobody-home"]
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("twice") && refusal.contains("once")),
        "the duplicate id is refused: {:?}",
        assessment.refusals
    );
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("nobody-home")),
        "the dangling contradiction is refused: {:?}",
        assessment.refusals
    );
}

/// Companion pin: evidence that does not exist is a broken registry,
/// not a stale claim — nothing can be re-measured against a path that
/// points at nothing.
#[test]
fn a_claim_whose_evidence_file_does_not_exist_is_refused() {
    let root = evidence_root("missing-evidence");
    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "ghost",
              "wording": "This claim stands on a file nobody committed.",
              "status": "proven",
              "scope": {},
              "evidence": [ { "kind": "baseline", "path": "evals/never-written.json" } ],
              "recorded": "2026-08-09",
              "invalidation": ["anything"],
              "surfaces": [],
              "review_route": "none"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment
            .refusals
            .iter()
            .any(|refusal| refusal.contains("ghost") && refusal.contains("never-written.json")),
        "{:?}",
        assessment.refusals
    );
}

/// The issue's sharpest enforcement case: current copy backed only by
/// stale evidence. A claim quietly ageing to unproven is green — the
/// world moved, the registry recorded it. The same claim still being
/// quoted by a user-facing surface is not: that is a sentence in front
/// of people that its own evidence no longer supports. And a surface
/// that does not exist is a refusal — a claim cannot say "this copy
/// depends on me" about copy nobody can find.
#[test]
fn a_surface_quoting_a_claim_that_is_no_longer_proven_is_stale_copy() {
    let root = evidence_root("stale-copy");
    std::fs::write(
        root.join("evals/baseline-v12.json"),
        r#"{
          "scoring_version": 12,
          "recorded_at": "2026-08-08T18:04:48Z",
          "reports": [
            {
              "pack": "app.kttl.letter-to-actions",
              "pack_version": "0.2.0",
              "eval_set": "development",
              "model": { "file": "qwen3.5-4b-q4_k_m.gguf" },
              "verdict": "pass"
            }
          ]
        }"#,
    )
    .expect("write baseline");
    std::fs::create_dir_all(root.join("app/src")).expect("create surface dir");
    std::fs::write(
        root.join("app/src/copy.ts"),
        "// quotes letter-harm-ceilings\n",
    )
    .expect("write surface");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "letter-harm-ceilings",
              "wording": "Qwen3.5-4B clears the letter pack's harm ceilings.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.letter-to-actions",
                "pack_version": "0.2.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [ { "kind": "baseline", "path": "evals/baseline-v12.json" } ],
              "recorded": "2026-08-08",
              "invalidation": ["scoring version change"],
              "surfaces": ["app/src/copy.ts"],
              "review_route": "re-measure"
            },
            {
              "id": "phantom-surface",
              "wording": "A claim depended on by copy nobody can find.",
              "status": "failed",
              "scope": {},
              "evidence": [ { "kind": "issue", "number": 1 } ],
              "recorded": "2026-08-09",
              "invalidation": [],
              "surfaces": ["app/src/deleted.ts"],
              "review_route": "none"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    // At scoring version 12 the evidence is current: the quoted claim
    // stands proven and nothing is stale copy.
    let current = registry.validate(&root, 12, today());
    assert!(current.stale_copy.is_empty(), "{:?}", current.stale_copy);

    // One bump later the same surface is quoting an unproven claim.
    let bumped = registry.validate(&root, 13, today());
    assert!(
        bumped.stale_copy.iter().any(
            |stale| stale.contains("app/src/copy.ts") && stale.contains("letter-harm-ceilings")
        ),
        "the surface and the claim are both named: {:?}",
        bumped.stale_copy
    );

    // The missing surface is a refusal in both worlds.
    assert!(
        current
            .refusals
            .iter()
            .any(|refusal| refusal.contains("phantom-surface")
                && refusal.contains("app/src/deleted.ts")),
        "{:?}",
        current.refusals
    );
}

/// #489's motivating case, distilled. On 11 August the renewal
/// baseline was re-recorded, its verdict went fail → pass, and the
/// claim asserting the failure stayed proven — `kettle claims` printed
/// byte-identical output across the swap, because validation read the
/// baseline's provenance and never what it found. A claim may now state
/// the verdict it asserts; evidence that says the opposite cannot go on
/// supporting it. A downgrade, not a refusal: the registry is fine, the
/// world moved.
#[test]
fn a_claim_whose_baseline_verdict_contradicts_it_does_not_stay_proven() {
    let root = evidence_root("contradicted-verdict");
    // Fresh by every provenance measure — current scoring version,
    // exactly the pack, model and set the claim is scoped to — and its
    // verdict says the opposite of the claim's sentence.
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-11T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.renewal-diff",
              "pack_version": "0.1.0",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
              "verdict": "pass"
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-renewal.json"), baseline).expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "renewal-development-verdict",
              "wording": "Qwen3.5-4B does not clear the renewal pack's development gate.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.renewal-diff",
                "pack_version": "0.1.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-renewal.json" }
              ],
              "expects": { "verdict": "fail" },
              "recorded": "2026-08-09",
              "invalidation": ["scoring version change", "bed change"],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(
        assessment.refusals.is_empty(),
        "contradicting evidence is a downgrade, not a structural refusal: {:?}",
        assessment.refusals
    );
    let claim = &assessment.claims[0];
    assert_eq!(
        claim.effective,
        Status::Unproven,
        "proven must not survive a baseline whose verdict contradicts the claim: {:?}",
        claim.reasons
    );
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("pass") && reasons.contains("fail"),
        "both verdicts are named, so the reader sees the contradiction: {reasons}"
    );
}

/// The trigger gap, exactly as #489 found it: the claim declared "bed
/// change" as an invalidation trigger, the bed demonstrably changed,
/// and both digests were in the baseline file — unread. Recording the
/// digest the claim was measured against is what makes the declared
/// trigger mechanically checkable.
#[test]
fn a_claim_declaring_bed_change_is_downgraded_when_the_bed_digest_moves() {
    let root = evidence_root("moved-bed");
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-11T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.renewal-diff",
              "pack_version": "0.1.0",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
              "bed": "blake3:922f16d488daffa776c6477477639404d6469f26f76c9382e5c223c151d9723e",
              "verdict": "pass"
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-renewal.json"), baseline).expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "renewal-development-verdict",
              "wording": "Qwen3.5-4B does not clear the renewal pack's development gate.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.renewal-diff",
                "pack_version": "0.1.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-renewal.json" }
              ],
              "recorded_against": {
                "bed": "blake3:c438a0df2d88b84e0b29a5eab912a2ded1acc5d60d6fcfa1f6666faa8c429d3d"
              },
              "recorded": "2026-08-09",
              "invalidation": ["scoring version change", "bed change"],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(
        assessment.refusals.is_empty(),
        "a moved bed is a downgrade, not a structural refusal: {:?}",
        assessment.refusals
    );
    let claim = &assessment.claims[0];
    assert_eq!(
        claim.effective,
        Status::Unproven,
        "proven must not survive evidence re-recorded on a different bed: {:?}",
        claim.reasons
    );
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("c438a0df") && reasons.contains("922f16d4"),
        "both bed digests are named, so the reader sees what moved: {reasons}"
    );
}

/// The other two recorded-against fields, same contract as the bed:
/// a value the claim records is compared against the matched report,
/// a value it does not record is not checked. Model and sidecar are
/// half of the "model or sidecar change" trigger the measured claims
/// already declare.
#[test]
fn a_recorded_against_model_or_sidecar_moving_downgrades_the_claim() {
    let root = evidence_root("moved-sidecar");
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-11T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.renewal-diff",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
              "sidecar": {{ "version": "10200 (f00dfeed)", "file": "llama-server" }},
              "verdict": "pass"
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-renewal.json"), baseline).expect("write baseline");

    let registry = Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "renewal-sidecar-moved",
              "wording": "The renewal measurement stands on the sidecar it was recorded on.",
              "status": "proven",
              "scope": { "pack": "app.kttl.renewal-diff", "eval_set": "development" },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-renewal.json" }
              ],
              "recorded_against": {
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "sidecar": "10145 (ad256ded)"
              },
              "recorded": "2026-08-11",
              "invalidation": ["model or sidecar change"],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(assessment.refusals.is_empty(), "{:?}", assessment.refusals);
    let claim = &assessment.claims[0];
    assert_eq!(claim.effective, Status::Unproven, "{:?}", claim.reasons);
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("10145 (ad256ded)") && reasons.contains("10200 (f00dfeed)"),
        "both sidecar versions are named: {reasons}"
    );
    assert!(
        !reasons.contains("qwen3.5-4b-q4_k_m.gguf is"),
        "the matching model must not be reported as moved: {reasons}"
    );
}

// ---------------------------------------------------------------------------
// The committed registry

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The registry Kettle actually ships: it must parse, conform to its
/// committed schema, and validate against the real evidence in the
/// tree with no refusals — at this scoring version and every later
/// one, because downgrades are how evidence ages, and refusals are how
/// the registry breaks.
///
/// The stable statuses are asserted; the letter claim's is deliberately
/// not, because it stands on a recorded baseline (`baseline-v14-letter`
/// today) and is *supposed* to age from proven to unproven the day a
/// SCORING_VERSION bump lands under it. Asserting it here would turn the registry's
/// next live downgrade into a red build. The renewal development
/// verdict ages the same way on the same bump, so it gets the same
/// presence-only treatment.
#[test]
fn the_committed_registry_is_valid_against_the_real_tree() {
    let root = repo_root();
    let schema_text = std::fs::read_to_string(root.join("assurance/claims.schema.json"))
        .expect("assurance/claims.schema.json is committed");
    let registry_text = std::fs::read_to_string(root.join("assurance/claims.json"))
        .expect("assurance/claims.json is committed");

    runner::assurance::conforms_to_schema(&schema_text, &registry_text)
        .expect("the committed registry conforms to its committed schema");
    let registry = Registry::from_json(&registry_text).expect("the committed registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());
    assert!(
        assessment.refusals.is_empty(),
        "the committed registry must never be structurally broken: {:?}",
        assessment.refusals
    );

    let effective = |id: &str| {
        assessment
            .claims
            .iter()
            .find(|claim| claim.id == id)
            .unwrap_or_else(|| panic!("the committed registry carries {id}"))
            .effective
    };
    // Claim zero (#457): the renewal v12 FAIL rested on the eval's own
    // broken join, and the v13 re-measurement found the harm ceilings
    // pass — the claim is measured false. Failed is a finding, and
    // stale evidence does not un-find it, so this is stable at every
    // scoring version.
    assert_eq!(effective("renewal-v12-fail-verdict"), Status::Failed);
    // #460: measured false on the v12 run, then built as a refusal
    // behind SCORING_VERSION 14 — the registry's second completed arc.
    // The claim now stands on its owning test, which validation checks
    // exists in this tree.
    assert_eq!(effective("quote-identifies-its-value"), Status::Proven);
    // #461: a measured failure, and failure is a state the registry
    // renders, not an error it hides.
    assert_eq!(effective("ambiguous-term-referred"), Status::Failed);
    // The guardrail #460 does NOT weaken: existence is enforced by a
    // deterministic test that exists in this tree.
    assert_eq!(effective("quote-existence-enforced"), Status::Proven);
    // The packaged network capture has not been recorded, and "not
    // evidenced" must say so.
    assert_eq!(effective("no-non-loopback-network"), Status::Unproven);
    // The measured claims exist; their effective statuses are the
    // evidence's age, asserted above to be a downgrade at worst, never
    // a refusal.
    effective("letter-harm-ceilings");
    effective("renewal-development-verdict");

    // Merge day, rehearsed: the moment a SCORING_VERSION bump lands
    // above this branch, the v13 baselines go stale under every claim
    // standing on them. That future must be a downgrade with both
    // versions named — never a refusal, or the bump that made the
    // evidence stale would also break the build recording that fact.
    let bumped = registry.validate(&root, runner::eval::SCORING_VERSION + 1, today());
    assert!(
        bumped.refusals.is_empty(),
        "a scoring bump must age the registry, not break it: {:?}",
        bumped.refusals
    );
    let letter = bumped
        .claims
        .iter()
        .find(|claim| claim.id == "letter-harm-ceilings")
        .expect("assessed");
    assert_eq!(letter.effective, Status::Unproven);
    // Both versions in the mismatch are the evidence's *own* recorded
    // version and the bumped harness — neither of them today's
    // constant, which is neither half of the story once main has moved
    // past it. Read from the baseline rather than written down here:
    // the first cut checked the constant and the v13 merge caught it,
    // the second wrote "13" in and the v14 re-record caught that too
    // (14 August). A number that has now rotted twice is a number this
    // test should be asking for.
    let recorded_version: u32 = {
        let baseline = registry
            .claims
            .iter()
            .find(|claim| claim.id == "letter-harm-ceilings")
            .and_then(|claim| {
                claim.evidence.iter().find_map(|evidence| match evidence {
                    runner::assurance::Evidence::Baseline { path } => Some(path.clone()),
                    _ => None,
                })
            })
            .expect("the letter claim stands on a baseline");
        let text = std::fs::read_to_string(root.join(&baseline))
            .unwrap_or_else(|e| panic!("read {baseline}: {e}"));
        serde_json::from_str::<serde_json::Value>(&text).expect("a baseline is json")
            ["scoring_version"]
            .as_u64()
            .expect("a baseline records its scoring version") as u32
    };
    let reasons = letter.reasons.join("\n");
    assert!(
        reasons.contains(&format!("scoring version {recorded_version}"))
            && reasons.contains(&(runner::eval::SCORING_VERSION + 1).to_string()),
        "the recorded and current versions are both named: {:?}",
        letter.reasons
    );
}

/// The schema is only worth committing if it is as strict as the
/// parser: a field serde would refuse must not slide through the
/// schema, or the two contracts drift and the looser one wins review.
#[test]
fn the_committed_schema_refuses_what_the_parser_refuses() {
    let root = repo_root();
    let schema_text = std::fs::read_to_string(root.join("assurance/claims.schema.json"))
        .expect("assurance/claims.schema.json is committed");
    let stray = r#"{
      "claims": [
        {
          "id": "stray",
          "wording": "A claim with a field nobody defined.",
          "status": "failed",
          "scope": {},
          "evidence": [ { "kind": "issue", "number": 1 } ],
          "recorded": "2026-08-09",
          "invalidation": [],
          "surfaces": [],
          "review_route": "none",
          "confidence": "high"
        }
      ]
    }"#;
    assert!(
        runner::assurance::conforms_to_schema(&schema_text, stray).is_err(),
        "the schema accepts a field the parser refuses"
    );
    assert!(
        Registry::from_json(stray).is_err(),
        "the parser accepts a field it should refuse"
    );
}

/// #478: a claim's evidence is only evidence to somebody who can open
/// it. The publication boundary names the part of the tree that goes
/// public; `app/` and the exam bed do not. A proven claim whose only
/// inspectable evidence sits outside that boundary would render on
/// kttl.app as an assertion with a citation nobody can follow — which
/// is the force-loss #477 was reopened about, shipped knowingly.
///
/// A refusal rather than a downgrade, for the same reason "a proven
/// claim must reference inspectable evidence" is: the evidence is real
/// and the measurement stands. What is broken is the registry as a
/// public record, and it is broken at the moment the claim is written,
/// not later on somebody else's merge.
#[test]
fn a_proven_claim_cannot_rest_only_on_evidence_the_public_cannot_open() {
    let root = evidence_root("publication-boundary");
    std::fs::create_dir_all(root.join("app/src")).expect("create app dir");
    std::fs::write(
        root.join("app/src/tokens-sync.test.ts"),
        "test('carries the compiled report stylesheet', () => {});",
    )
    .expect("write private test");

    let registry = Registry::from_json(
        r#"{
          "published": ["crates/", "packs/", "evals/", "assurance/"],
          "claims": [
            {
              "id": "report-keeps-its-stylesheet",
              "wording": "A rendered report keeps the stylesheet it was born with.",
              "status": "proven",
              "scope": {},
              "evidence": [
                {
                  "kind": "test",
                  "path": "app/src/tokens-sync.test.ts",
                  "name": "carries the compiled report stylesheet"
                }
              ],
              "recorded": "2026-08-09",
              "invalidation": ["the report template stops inlining its CSS"],
              "surfaces": [],
              "review_route": "bun run test in app/"
            }
          ]
        }"#,
    )
    .expect("registry parses");

    let assessment = registry.validate(&root, runner::eval::SCORING_VERSION, today());

    let refusals = assessment.refusals.join("\n");
    assert!(
        refusals.contains("report-keeps-its-stylesheet"),
        "the claim standing on unpublishable evidence is named: {refusals:?}"
    );
    assert!(
        refusals.contains("app/src/tokens-sync.test.ts"),
        "the path a reader could not open is named, so the fix is obvious \
         from the message: {refusals:?}"
    );
}

/// #478: the generated public tree is a projection, and validation must
/// know which tree it is looking at.
///
/// A claim may name a surface or a test inside the half that stays
/// closed — `app/src/tokens-sync.test.ts` is the real one. In the working
/// tree that file is there, and its absence means somebody deleted the
/// copy this claim says depends on it: a refusal, and one worth keeping.
/// In the projection the same absence is the boundary working as
/// designed, and refusing there would ship a public repository whose
/// `assurance/claims.json` fails its own check — on a project whose whole
/// credential is that claims match evidence.
///
/// So the projection carries a marker, and the rule is narrow: *outside*
/// the published boundary, absent is expected. Inside it, absent is still
/// broken. The next two tests are the two halves, and the second is the
/// one that matters — a mode that excused every absence would be an
/// amnesty rather than a boundary.
fn projected_registry(surface: &str) -> String {
    format!(
        r#"{{
          "published": ["crates/", "packs/", "assurance/"],
          "claims": [
            {{
              "id": "reports-self-contained",
              "wording": "A rendered report opens with no network.",
              "status": "proven",
              "scope": {{}},
              "evidence": [
                {{
                  "kind": "test",
                  "path": "crates/runner/tests/report.rs",
                  "name": "inlines_its_stylesheet"
                }}
              ],
              "recorded": "2026-08-09",
              "invalidation": ["the template stops inlining its CSS"],
              "surfaces": ["{surface}"],
              "review_route": "the owning test"
            }}
          ]
        }}"#
    )
}

/// A root holding the registry's in-boundary evidence, and optionally the
/// marker that says this tree is the projection.
fn projected_root(name: &str, projection: bool) -> PathBuf {
    let root = evidence_root(name);
    std::fs::create_dir_all(root.join("crates/runner/tests")).expect("create evidence dir");
    std::fs::write(
        root.join("crates/runner/tests/report.rs"),
        "#[test] fn inlines_its_stylesheet() {}",
    )
    .expect("write the owning test");
    if projection {
        std::fs::write(
            root.join(runner::assurance::PROJECTION_MARKER),
            r#"{"schema":"kettle/projection@0"}"#,
        )
        .expect("write the projection marker");
    }
    root
}

#[test]
fn in_the_projection_a_surface_in_the_closed_half_is_expected_absent() {
    let closed = "app/src/lib/components/ExportPanel.svelte";

    let working_tree = projected_root("closed-surface-working", false);
    let refusals = Registry::from_json(&projected_registry(closed))
        .expect("registry parses")
        .validate(&working_tree, runner::eval::SCORING_VERSION, today())
        .refusals
        .join("\n");
    assert!(
        refusals.contains(closed),
        "in the working tree a missing surface is still a refusal, because \
         there it means the copy was deleted: {refusals:?}"
    );

    let projection = projected_root("closed-surface-projection", true);
    let refusals = Registry::from_json(&projected_registry(closed))
        .expect("registry parses")
        .validate(&projection, runner::eval::SCORING_VERSION, today())
        .refusals
        .join("\n");
    assert!(
        refusals.is_empty(),
        "the projection refuses a surface it was never meant to carry, so \
         the published registry fails its own check: {refusals:?}"
    );
}

#[test]
fn in_the_projection_a_surface_inside_the_boundary_is_still_missing() {
    let inside = "packs/app.kttl.renewal-diff/report.html.tera";

    let projection = projected_root("inside-surface-projection", true);
    let refusals = Registry::from_json(&projected_registry(inside))
        .expect("registry parses")
        .validate(&projection, runner::eval::SCORING_VERSION, today())
        .refusals
        .join("\n");

    assert!(
        refusals.contains(inside),
        "a path the projection publishes and does not carry is broken \
         wherever it is validated — otherwise the marker is an amnesty on \
         every absence, not a boundary: {refusals:?}"
    );
}

/// The property that makes the narrow rule safe. #516 already refuses a
/// proven claim whose only probative evidence sits outside the boundary,
/// so every proven claim keeps at least one citation the projection
/// carries — and that citation is checked there exactly as strictly as
/// here. Excusing the closed half costs no real check.
#[test]
fn the_projection_still_checks_the_evidence_it_does_carry() {
    let root = projected_root("published-evidence-projection", true);
    std::fs::write(
        root.join("crates/runner/tests/report.rs"),
        "#[test] fn renamed_since_the_claim_was_written() {}",
    )
    .expect("rewrite the owning test");

    let refusals = Registry::from_json(&projected_registry("packs/"))
        .expect("registry parses")
        .validate(&root, runner::eval::SCORING_VERSION, today())
        .refusals
        .join("\n");

    assert!(
        refusals.contains("inlines_its_stylesheet"),
        "a renamed test inside the published boundary must still be caught \
         in the projection: {refusals:?}"
    );
}

/// A `proven` claim standing on repeats that disagreed with each other
/// is the registry asserting more than the evidence shows (#533). Both
/// live claims were recorded at `--runs 1` — `letter-harm-ceilings`
/// passes at Wilson upper 0.02 against a 0.02 ceiling, so one decision
/// moving in one repeat is the difference between proven and withdrawn.
///
/// A downgrade rather than a refusal, on the same argument as a moved
/// bed digest: the registry file is fine, the evidence simply stopped
/// supporting the sentence, and a person has to re-read one against the
/// other.
#[test]
fn a_baseline_whose_repeats_disagreed_cannot_keep_a_claim_proven() {
    let root = evidence_root("moved-spread");
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-19T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.letter-to-actions",
              "pack_version": "0.2.0",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
              "verdict": "pass",
              "fixtures": [
                {{
                  "fixture": "steady-one",
                  "stability": {{
                    "runs": 3,
                    "steps": {{ "obligations": {{ "low": 0.98, "high": 0.98 }} }},
                    "end_to_end": {{ "low": 0.97, "high": 0.97 }},
                    "needs_review_rate": {{ "low": 0.0, "high": 0.0 }}
                  }}
                }},
                {{
                  "fixture": "wobbled-two",
                  "stability": {{
                    "runs": 3,
                    "steps": {{ "obligations": {{ "low": 0.94, "high": 0.98 }} }},
                    "end_to_end": {{ "low": 0.97, "high": 0.97 }},
                    "needs_review_rate": {{ "low": 0.0, "high": 0.0 }}
                  }}
                }}
              ]
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-letter.json"), baseline).expect("write baseline");

    let assessment =
        registry_for_letter_ceilings().validate(&root, runner::eval::SCORING_VERSION, today());

    assert!(
        assessment.refusals.is_empty(),
        "repeats that disagreed are a downgrade, not a structural refusal: {:?}",
        assessment.refusals
    );
    let claim = &assessment.claims[0];
    assert_eq!(
        claim.effective,
        Status::Unproven,
        "proven must not survive a run set that disagreed with itself: {:?}",
        claim.reasons
    );
    let reasons = claim.reasons.join("\n");
    assert!(
        reasons.contains("wobbled-two"),
        "the fixture that moved is named, so there is something to chase: {reasons}"
    );
}

/// The asymmetry the check encodes deliberately (#533): what is refused
/// is *measured and moved*, never *unmeasured*. Requiring repeats
/// everywhere would make every `--runs 1` measurement unusable as
/// evidence, which is most of them and all of the cheap ones.
#[test]
fn stability_that_held_or_was_never_measured_leaves_a_claim_proven() {
    for (name, fixtures) in [
        (
            "held",
            r#"[{ "fixture": "steady-one", "stability": { "runs": 3, "steps": { "obligations": { "low": 0.98, "high": 0.98 } }, "end_to_end": { "low": 0.97, "high": 0.97 }, "needs_review_rate": { "low": 0.0, "high": 0.0 } } }]"#,
        ),
        ("unmeasured", r#"[{ "fixture": "steady-one" }]"#),
        ("no-fixtures-at-all", "[]"),
    ] {
        let root = evidence_root(&format!("stability-{name}"));
        let baseline = format!(
            r#"{{
              "scoring_version": {},
              "recorded_at": "2026-08-19T12:00:00Z",
              "reports": [
                {{
                  "pack": "app.kttl.letter-to-actions",
                  "pack_version": "0.2.0",
                  "eval_set": "development",
                  "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
                  "verdict": "pass",
                  "fixtures": {fixtures}
                }}
              ]
            }}"#,
            runner::eval::SCORING_VERSION,
        );
        std::fs::write(root.join("evals/baseline-letter.json"), baseline).expect("write baseline");

        let assessment =
            registry_for_letter_ceilings().validate(&root, runner::eval::SCORING_VERSION, today());

        let claim = &assessment.claims[0];
        assert_eq!(
            claim.effective,
            Status::Proven,
            "{name}: nothing moved, so nothing is withdrawn: {:?}",
            claim.reasons
        );
    }
}

/// The claim both stability tests stand on, so the two differ only in
/// the baseline they are handed.
fn registry_for_letter_ceilings() -> Registry {
    Registry::from_json(
        r#"{
          "claims": [
            {
              "id": "letter-harm-ceilings",
              "wording": "Kettle's letter pack meets its harm ceilings on the development bed.",
              "status": "proven",
              "scope": {
                "pack": "app.kttl.letter-to-actions",
                "pack_version": "0.2.0",
                "model": "qwen3.5-4b-q4_k_m.gguf",
                "eval_set": "development"
              },
              "evidence": [
                { "kind": "baseline", "path": "evals/baseline-letter.json" }
              ],
              "expects": { "verdict": "pass" },
              "recorded": "2026-08-19",
              "invalidation": ["scoring version change", "bed change"],
              "surfaces": [],
              "review_route": "re-measure with kettle eval --write-baseline"
            }
          ]
        }"#,
    )
    .expect("registry parses")
}

/// The spreads a claim's baseline records are the quantities somebody
/// remembered to watch, and the harm ceiling is not among them (#533).
/// So validation reads the total form too: repeats that recorded
/// different results downgrade the claim even when every spread held.
#[test]
fn repeats_that_recorded_different_results_downgrade_even_with_flat_spreads() {
    let root = evidence_root("moved-digest");
    let baseline = format!(
        r#"{{
          "scoring_version": {},
          "recorded_at": "2026-08-19T12:00:00Z",
          "reports": [
            {{
              "pack": "app.kttl.letter-to-actions",
              "pack_version": "0.2.0",
              "eval_set": "development",
              "model": {{ "file": "qwen3.5-4b-q4_k_m.gguf" }},
              "verdict": "pass",
              "fixtures": [
                {{
                  "fixture": "flat-but-divergent",
                  "stability": {{
                    "runs": 3,
                    "steps": {{ "obligations": {{ "low": 0.98, "high": 0.98 }} }},
                    "end_to_end": {{ "low": 0.97, "high": 0.97 }},
                    "needs_review_rate": {{ "low": 0.0, "high": 0.0 }},
                    "record_digests": ["blake3:aaa", "blake3:bbb"]
                  }}
                }}
              ]
            }}
          ]
        }}"#,
        runner::eval::SCORING_VERSION,
    );
    std::fs::write(root.join("evals/baseline-letter.json"), baseline).expect("write baseline");

    let assessment =
        registry_for_letter_ceilings().validate(&root, runner::eval::SCORING_VERSION, today());

    let claim = &assessment.claims[0];
    assert_eq!(
        claim.effective,
        Status::Unproven,
        "every spread held and the repeats still disagreed: {:?}",
        claim.reasons
    );
    assert!(
        claim.reasons.join("\n").contains("flat-but-divergent"),
        "the fixture that diverged is named"
    );
}
