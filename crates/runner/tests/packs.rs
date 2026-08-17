//! Contract tests for the pack loader (#16) and capability refusal
//! (#17). Authored red as the handoff contract — the delegate's job is
//! to make these green by implementing `packs::load_pack`, not to edit
//! them. See the CONTRACT FILE note in `src/packs.rs`.

use runner::eval::{EvalCost, EvalMetric, HarmClass};
use runner::packs::{load_pack, step_batches, PackError, PipelineStep};
use std::path::{Path, PathBuf};

fn subscription_audit_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.subscription-audit")
}

/// The scratch pack's copy block (#244), a named constant so the
/// negative test can strip exactly this and nothing else.
const SCRATCH_COPY: &str = r#""copy": {
                "time": { "kind": "varies", "estimate": "by file", "on_this_computer": "This task hasn't been timed yet." },
                "will": [ { "doing": "Sort the rows", "detail": "each one on its own.", "steps": ["Grouping payments by merchant"] } ],
                "run_verb": "Run this task"
              },"#;

/// A minimal valid pack written to a per-test scratch directory; each
/// negative test starts from valid and breaks exactly one thing.
/// Tests share one process, so pid + name is unique enough.
struct ScratchPack {
    dir: PathBuf,
}

impl ScratchPack {
    fn valid(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("kettle-pack-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["prompts", "schemas"] {
            std::fs::create_dir_all(dir.join(sub)).expect("create scratch pack dirs");
        }
        let write = |relative: &str, content: &str| {
            std::fs::write(dir.join(relative), content).expect("write scratch pack file");
        };
        write(
            "pack.json",
            &format!(
                r#"{{
              "id": "app.kttl.scratch",
              "name": "Scratch Pack",
              "version": "0.0.1",
              "min_runner_version": "0.1.0",
              "inputs": [{{ "role": "statement", "label": "Your bank statements", "accept": ["text/csv"], "multiple": false }}],
              "capabilities": ["read"],
              "model": {{ "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 }},
              {SCRATCH_COPY}
              "pipeline": [
                {{ "step": "model", "role": "normalise", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 }},
                {{ "step": "render", "template": "report.html.tera" }}
              ],
              "outputs": ["report.html"]
            }}"#
            ),
        );
        write("prompts/one.md", "Sort these:\n{{ batch_json }}\n");
        write("schemas/one.schema.json", r#"{ "type": "object" }"#);
        write("report.html.tera", "<html></html>");
        ScratchPack { dir }
    }

    /// Re-write pack.json with one field changed, keeping the rest valid.
    fn amend_manifest(&self, from: &str, to: &str) {
        let path = self.dir.join("pack.json");
        let manifest = std::fs::read_to_string(&path).expect("read scratch manifest");
        assert!(
            manifest.contains(from),
            "amendment target {from:?} not in manifest"
        );
        std::fs::write(&path, manifest.replace(from, to)).expect("amend scratch manifest");
    }
}

impl Drop for ScratchPack {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ── #16: manifest parse + validation ────────────────────────────────

#[test]
fn subscription_audit_manifest_loads() {
    let pack = load_pack(&subscription_audit_dir()).expect("the real pack loads");
    let manifest = &pack.manifest;

    assert_eq!(manifest.id, "app.kttl.subscription-audit");
    assert_eq!(manifest.version, "1.5.0");
    assert_eq!(manifest.capabilities, ["read"]);
    assert_eq!(manifest.pipeline.len(), 6);
    assert_eq!(
        manifest.eval_metrics.iter().copied().collect::<Vec<_>>(),
        [EvalMetric::Classification]
    );
    assert!(!manifest.eval.contains_key("max_review_rate"));

    // The normalise step carries its schema and batch size.
    let PipelineStep::Model {
        prompt,
        role,
        schema,
        batch,
        examples,
        optional,
    } = &manifest.pipeline[1]
    else {
        panic!("pipeline[1] should be the normalise model step");
    };
    // #120: what the step means is declared here, not inferred from it
    // being the first schema-bearing step in the list.
    assert_eq!(role.as_deref(), Some("normalise"));
    assert_eq!(prompt, "prompts/normalise.md");
    assert_eq!(schema.as_deref(), Some("schemas/normalise.schema.json"));
    assert_eq!(*batch, Some(20));
    assert_eq!(*examples, None);
    assert!(!optional);

    // The summary step is why schema and batch are optional: prose
    // output has neither.
    let PipelineStep::Model {
        schema,
        batch,
        optional,
        ..
    } = &manifest.pipeline[4]
    else {
        panic!("pipeline[4] should be the summary model step");
    };
    assert_eq!(*schema, None);
    assert_eq!(*batch, None);
    assert!(optional);
}

#[test]
fn review_cost_declaration_carries_pack_provenance() {
    let pack = load_pack(&subscription_audit_dir()).expect("the real pack loads");
    let review = pack
        .manifest
        .eval_costs
        .get(&EvalCost::ReviewRate)
        .expect("review rate is declared as a tracked cost");

    assert_eq!(review.date.to_string(), "2026-07-29");
    assert!(
        review.reason.contains("human"),
        "the number must say why this pack records it: {}",
        review.reason
    );
}

#[test]
fn classification_strata_and_floors_are_declared_before_fixtures() {
    let pack = load_pack(&subscription_audit_dir()).expect("the real pack loads");
    let strata = &pack.manifest.eval_strata;

    for required in [
        "any-statement",
        "clean",
        "messy-merchant-strings",
        "ambiguous-categories",
        "no-subscriptions",
        "subscription-heavy",
        "annual-subscription-once-yearly",
        "free-trial-conversion",
        "cancelled-then-resumed",
        "price-rise-mid-series",
        "refunds-and-chargebacks",
        "multi-descriptor-merchant",
        "duplicate-charge-not-subscription",
    ] {
        assert!(
            strata.contains_key(required),
            "{required} must be defined before its fixtures are written"
        );
    }

    // #316: both classes gate pooled. A ceiling counts distinct
    // decisions (#310), and sliced nineteen ways this bed's worst cells
    // held eight against the 73 a 5% ceiling needs — so every gate read
    // UNPROVEN and no slice could say anything.
    let pooled = &strata["any-statement"];
    for class in [HarmClass::Subscription, HarmClass::NotSubscription] {
        let floor = &pooled.classes[&class];
        assert_eq!(floor.max_wilson_95, 0.05, "the 5% bar itself is unchanged");
        assert_eq!(floor.date.to_string(), "2026-08-01");
        assert!(
            floor.reason.contains("gated pooled"),
            "the threshold must retain why it was chosen: {}",
            floor.reason
        );
    }

    // The three strata that keep a ceiling of their own, because a
    // confident denial there is the harm they were built to catch. This
    // is the half of #316 that stops pooling hiding a pack which is safe
    // on average and dangerous on annual renewals.
    for stratum in [
        "annual-subscription-once-yearly",
        "free-trial-conversion",
        "price-rise-mid-series",
    ] {
        assert_eq!(
            strata[stratum].classes.keys().copied().collect::<Vec<_>>(),
            [HarmClass::Subscription],
            "{stratum} gates the denial it exists to catch, and only that"
        );
    }

    // Every other stratum is a diagnostic: it slices the results so a
    // pooled failure can be read, and gates nothing.
    for (name, declaration) in strata {
        let gated = name == "any-statement"
            || matches!(
                name.as_str(),
                "annual-subscription-once-yearly"
                    | "free-trial-conversion"
                    | "price-rise-mid-series"
            );
        if !gated {
            assert!(
                declaration.classes.is_empty(),
                "{name} is a diagnostic stratum and must declare no ceiling — \
                 a bed this size cannot carry one (#316)"
            );
        }
    }
}

#[test]
fn classification_floor_without_reason_is_refused() {
    let pack = ScratchPack::valid("blank-classification-floor-reason");
    pack.amend_manifest(
        r#""outputs": ["report.html"]"#,
        r#""eval_strata": {
              "clean": {
                "description": "Clear strings.",
                "classes": {
                  "subscription": {
                    "max_wilson_95": 0.05,
                    "reason": "  ",
                    "date": "2026-07-29"
                  }
                }
              }
            },
            "outputs": ["report.html"]"#,
    );

    let problem = load_pack(&pack.dir).expect_err("a threshold without reasoning is not valid");
    assert!(problem.to_string().contains("clean"), "{problem}");
    assert!(problem.to_string().contains("reason"), "{problem}");
}

// ── #94: forward-compatible manifests ───────────────────────────────

#[test]
fn manifest_min_runner_version_above_current_is_refused() {
    let pack = ScratchPack::valid("future-runner");
    pack.amend_manifest(
        r#""min_runner_version": "0.1.0""#,
        r#""min_runner_version": "999.0.0""#,
    );

    let error = load_pack(&pack.dir).expect_err("a pack requiring a future runner must be refused");
    let message = error.to_string();

    assert!(
        matches!(error, PackError::NeedsNewerRunner { .. }),
        "expected NeedsNewerRunner, got: {error}"
    );
    assert!(
        message.contains("999.0.0"),
        "names the requirement: {message}"
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "names this runner: {message}"
    );
    assert!(
        message.contains("newer Kettle"),
        "offers the useful next step: {message}"
    );
}

#[test]
fn equal_and_lower_min_runner_versions_load() {
    let equal = ScratchPack::valid("current-runner");
    load_pack(&equal.dir).expect("the current runner satisfies an equal minimum");

    let lower = ScratchPack::valid("older-runner");
    lower.amend_manifest(
        r#""min_runner_version": "0.1.0""#,
        r#""min_runner_version": "0.0.1""#,
    );
    load_pack(&lower.dir).expect("the current runner satisfies a lower minimum");
}

#[test]
fn an_invalid_min_runner_version_is_a_manifest_problem() {
    let pack = ScratchPack::valid("invalid-runner-version");
    pack.amend_manifest(
        r#""min_runner_version": "0.1.0""#,
        r#""min_runner_version": "new enough""#,
    );

    let error = load_pack(&pack.dir).expect_err("versions must be semantic versions");
    assert!(
        matches!(error, PackError::InvalidRunnerVersion { .. }),
        "expected InvalidRunnerVersion, got: {error}"
    );
    let message = error.to_string();
    assert!(message.contains("min_runner_version"), "{message}");
    assert!(message.contains("new enough"), "{message}");
}

#[test]
fn a_non_namespaced_pack_id_is_refused() {
    let pack = ScratchPack::valid("bare-id");
    pack.amend_manifest(
        r#""id": "app.kttl.scratch""#,
        r#""id": "subscription-audit""#,
    );

    let error = load_pack(&pack.dir).expect_err("pack ids must be reverse-DNS namespaced");
    assert!(
        matches!(error, PackError::InvalidId { .. }),
        "expected InvalidId, got: {error}"
    );
    let message = error.to_string();
    assert!(message.contains("subscription-audit"), "{message}");
    assert!(message.contains("app.kttl"), "{message}");
}

#[test]
fn a_manifest_containing_wasm_steps_needs_a_newer_kettle() {
    let pack = ScratchPack::valid("wasm-reserved");
    pack.amend_manifest(
        r#""capabilities": ["read"],"#,
        r#""capabilities": ["read"],
              "wasm_steps": [{
                "step": "gtm-container-parse",
                "module": "steps/gtm_parse.wasm",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "limits": { "fuel": 5000000, "memory_mb": 64 }
              }],"#,
    );

    let error = load_pack(&pack.dir).expect_err("WASM is reserved but not implemented");
    assert!(
        matches!(error, PackError::UnsupportedWasm { .. }),
        "expected UnsupportedWasm, got: {error}"
    );
    let message = error.to_string();
    assert!(message.contains("WASM"), "{message}");
    assert!(message.contains("newer Kettle"), "{message}");
}

#[test]
fn the_reserved_wasm_shape_is_validated_before_it_is_refused() {
    let pack = ScratchPack::valid("wasm-invalid-hash");
    pack.amend_manifest(
        r#""capabilities": ["read"],"#,
        r#""capabilities": ["read"],
              "wasm_steps": [{
                "step": "gtm-container-parse",
                "module": "steps/gtm_parse.wasm",
                "sha256": "not-a-sha256",
                "limits": { "fuel": 5000000, "memory_mb": 64 }
              }],"#,
    );

    let error = load_pack(&pack.dir).expect_err("a malformed WASM declaration is not accepted");
    assert!(
        matches!(error, PackError::InvalidWasmStep { .. }),
        "expected InvalidWasmStep, got: {error}"
    );
    assert!(error.to_string().contains("sha256"), "{error}");
}

#[test]
fn a_reserved_wasm_module_path_cannot_leave_the_pack() {
    let pack = ScratchPack::valid("wasm-escaping");
    pack.amend_manifest(
        r#""capabilities": ["read"],"#,
        r#""capabilities": ["read"],
              "wasm_steps": [{
                "step": "gtm-container-parse",
                "module": "../gtm_parse.wasm",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "limits": { "fuel": 5000000, "memory_mb": 64 }
              }],"#,
    );

    let error =
        load_pack(&pack.dir).expect_err("a reserved path is still part of the trust boundary");
    assert!(
        matches!(error, PackError::EscapingPath { .. }),
        "expected EscapingPath, got: {error}"
    );
}

#[test]
fn unknown_manifest_and_pipeline_fields_are_rejected() {
    let root = ScratchPack::valid("unknown-root-field");
    root.amend_manifest(
        r#""version": "0.0.1","#,
        r#""version": "0.0.1", "future_security_mode": true,"#,
    );
    let error = load_pack(&root.dir).expect_err("unknown root fields must not be ignored");
    assert!(matches!(error, PackError::Manifest(_)), "{error}");
    assert!(
        error.to_string().contains("future_security_mode"),
        "{error}"
    );

    let pipeline = ScratchPack::valid("unknown-step-field");
    pipeline.amend_manifest(r#""batch": 5 }"#, r#""batch": 5, "network": true }"#);
    let error = load_pack(&pipeline.dir).expect_err("unknown step fields must not be ignored");
    assert!(matches!(error, PackError::Manifest(_)), "{error}");
    assert!(error.to_string().contains("network"), "{error}");
}

#[test]
fn subscription_audit_only_advertises_inputs_that_reach_a_report() {
    // #116 removed PDF until #137 could prove choose/drop-to-report.
    // Both advertised shapes now have an end-to-end path.
    let pack = load_pack(&subscription_audit_dir()).expect("the real pack loads");
    let statement = &pack.manifest.inputs[0];
    assert_eq!(statement.accept, ["text/csv", "application/pdf"]);
}

#[test]
fn missing_schema_file_rejected() {
    let pack = ScratchPack::valid("missing-schema");
    std::fs::remove_file(pack.dir.join("schemas/one.schema.json")).expect("remove schema");

    let error = load_pack(&pack.dir).expect_err("missing schema file must refuse to load");
    let PackError::MissingFile { path } = error else {
        panic!("expected MissingFile, got: {error}");
    };
    assert!(
        path.ends_with("schemas/one.schema.json"),
        "error should name the missing file: {}",
        path.display()
    );
}

#[test]
fn invalid_schema_file_rejected() {
    let pack = ScratchPack::valid("invalid-schema");
    // Valid JSON, but not a usable schema: "type" must be a string.
    std::fs::write(
        pack.dir.join("schemas/one.schema.json"),
        r#"{ "type": 42 }"#,
    )
    .expect("write broken schema");

    let error = load_pack(&pack.dir).expect_err("unusable schema must refuse to load");
    assert!(
        matches!(error, PackError::InvalidSchema { .. }),
        "expected InvalidSchema, got: {error}"
    );
}

#[test]
fn zero_batch_size_rejected() {
    let pack = ScratchPack::valid("zero-batch");
    pack.amend_manifest(r#""batch": 5"#, r#""batch": 0"#);

    let error = load_pack(&pack.dir).expect_err("batch: 0 is a broken manifest");
    assert!(
        matches!(error, PackError::ZeroBatch { .. }),
        "expected ZeroBatch, got: {error}"
    );
}

// ── #17: capability refusal beyond ["read"] ─────────────────────────

#[test]
fn capability_beyond_read_is_refused() {
    let pack = ScratchPack::valid("write-refused");
    pack.amend_manifest(
        r#""capabilities": ["read"]"#,
        r#""capabilities": ["read", "write"]"#,
    );

    let error = load_pack(&pack.dir).expect_err("a pack asking to write must be refused");
    let PackError::Refused { capabilities } = error else {
        panic!("expected Refused, got: {error}");
    };
    // Only the capabilities beyond read — the ones actually refused.
    assert_eq!(capabilities, ["write"]);
}

#[test]
fn unknown_capability_is_refused_not_ignored() {
    // Future-proofing: a capability this version doesn't recognise is
    // treated as beyond read, never silently dropped.
    let pack = ScratchPack::valid("unknown-refused");
    pack.amend_manifest(
        r#""capabilities": ["read"]"#,
        r#""capabilities": ["read", "calendar_export"]"#,
    );

    let error = load_pack(&pack.dir).expect_err("an unrecognised capability must be refused");
    let PackError::Refused { capabilities } = error else {
        panic!("expected Refused, got: {error}");
    };
    assert_eq!(capabilities, ["calendar_export"]);
}

#[test]
fn refusal_message_is_plain_language() {
    // The refusal surfaces to people. No jargon, and it must say what
    // the pack asked for.
    let pack = ScratchPack::valid("refusal-copy");
    pack.amend_manifest(
        r#""capabilities": ["read"]"#,
        r#""capabilities": ["read", "write"]"#,
    );

    let message = load_pack(&pack.dir).expect_err("refused").to_string();
    assert!(message.contains("write"), "names the capability: {message}");
    assert!(
        message.to_lowercase().contains("won't run"),
        "plain refusal, not an error code: {message}"
    );
}

#[test]
fn schema_outside_grammar_subset_rejected_at_load() {
    // A schema llama-server can't fully convert means generation runs
    // unconstrained while re-validation still passes — invisible at run
    // time (§4a), so the loader must refuse it loudly, by name.
    let pack = ScratchPack::valid("grammar-unsafe");
    std::fs::write(
        pack.dir.join("schemas/one.schema.json"),
        r#"{ "type": "object", "additionalProperties": false }"#,
    )
    .expect("write grammar-unsafe schema");

    let error = load_pack(&pack.dir).expect_err("grammar-unsafe schema must refuse to load");
    let PackError::InvalidSchema { reason, .. } = &error else {
        panic!("expected InvalidSchema, got: {error}");
    };
    assert!(
        reason.contains("additionalProperties"),
        "should name the offending keyword: {reason}"
    );
}

// ── #77: manifest paths stay inside the pack directory ──────────────

#[test]
fn manifest_path_escaping_pack_dir_rejected() {
    // The file genuinely exists outside the pack, so MissingFile cannot
    // be what refuses it — only containment can.
    let pack = ScratchPack::valid("escaping-prompt");
    let outside = pack.dir.join("../kettle-pack-test-outside.md");
    std::fs::write(&outside, "Sort these:\n{{ batch_json }}\n").expect("write file outside pack");
    pack.amend_manifest(
        r#""prompt": "prompts/one.md""#,
        r#""prompt": "../kettle-pack-test-outside.md""#,
    );

    let error = load_pack(&pack.dir).expect_err("a path leaving the pack must refuse to load");
    let _ = std::fs::remove_file(&outside);

    let PackError::EscapingPath { path } = &error else {
        panic!("expected EscapingPath, got: {error}");
    };
    assert_eq!(path, Path::new("../kettle-pack-test-outside.md"));
    // Surfaces to people: name the path, say what the rule is.
    let message = error.to_string();
    assert!(
        message.contains("../kettle-pack-test-outside.md"),
        "names the path: {message}"
    );
    assert!(
        message.to_lowercase().contains("outside"),
        "plain language, not an error code: {message}"
    );
}

#[test]
fn absolute_manifest_path_rejected() {
    // `Path::join` replaces the base entirely for an absolute path, so
    // without containment a manifest could point the loader at any file
    // on disk. /etc/hosts exists here, so again only containment can be
    // what refuses this.
    let pack = ScratchPack::valid("absolute-prompt");
    pack.amend_manifest(r#""prompt": "prompts/one.md""#, r#""prompt": "/etc/hosts""#);

    let error = load_pack(&pack.dir).expect_err("an absolute path must refuse to load");
    let PackError::EscapingPath { path } = &error else {
        panic!("expected EscapingPath, got: {error}");
    };
    assert_eq!(path, Path::new("/etc/hosts"));
}

#[test]
fn every_manifest_reference_is_contained_not_just_prompts() {
    // Containment is the trust boundary for the whole manifest, not one
    // field: schemas, examples and the render template escape the same
    // way.
    let pack = ScratchPack::valid("escaping-template");
    pack.amend_manifest(
        r#""template": "report.html.tera""#,
        r#""template": "../report.html.tera""#,
    );

    let error = load_pack(&pack.dir).expect_err("an escaping template must refuse to load");
    assert!(
        matches!(error, PackError::EscapingPath { .. }),
        "expected EscapingPath, got: {error}"
    );
}

#[test]
fn broken_prompt_template_rejected_at_load() {
    // A prompt that doesn't render is a broken pack; it must fail at
    // load, not three minutes into a run when its step comes up.
    let pack = ScratchPack::valid("broken-prompt");
    std::fs::write(
        pack.dir.join("prompts/one.md"),
        "Sort these:\n{{ unclosed\n",
    )
    .expect("write broken prompt");

    let error = load_pack(&pack.dir).expect_err("broken template must refuse to load");
    let PackError::BrokenPrompt { path, .. } = &error else {
        panic!("expected BrokenPrompt, got: {error}");
    };
    assert!(
        path.ends_with("prompts/one.md"),
        "names the prompt: {}",
        path.display()
    );
}

#[test]
fn model_step_batches_at_its_declared_size() {
    // #21's last piece: the per-step batch size comes from pack.json,
    // not from whoever happens to call the executor.
    let pack = load_pack(&subscription_audit_dir()).expect("real pack loads");
    let items: Vec<String> = (0..45).map(|n| format!("MERCHANT {n}")).collect();

    // normalise declares batch: 20, classify batch: 15 (brief §3).
    let normalise = step_batches(&pack.manifest.pipeline[1], &items).expect("normalise is batched");
    assert_eq!(
        normalise.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![20, 20, 5]
    );
    let classify = step_batches(&pack.manifest.pipeline[2], &items).expect("classify is batched");
    assert_eq!(
        classify.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![15, 15, 15]
    );
    // Ids stay step-wide across the batches either way.
    assert_eq!(classify[2][0].id, 30);

    // The summary step declares no batch — one call over the aggregate,
    // not a batch sweep. Non-model steps aren't batched at all.
    assert!(step_batches(&pack.manifest.pipeline[4], &items).is_none());
    assert!(step_batches(&pack.manifest.pipeline[0], &items).is_none());
}

// ── #120: the execution contract — semantics are declared, not positional ──

#[test]
fn unsupported_builtin_is_refused_at_load_not_mid_run() {
    // Today `builtin:` names are only checked inside the run loop, so a
    // pack naming a step this runner has never heard of loads clean and
    // fails after the person has already chosen their files. The set of
    // builtins is closed and known at load time; there is no reason to
    // wait.
    let pack = ScratchPack::valid("unknown-builtin");
    pack.amend_manifest(
        r#"{ "step": "model", "role": "normalise", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 }"#,
        r#"{ "step": "aggregate", "impl": "builtin:obligation-extract" }"#,
    );

    let error = load_pack(&pack.dir)
        .expect_err("a builtin this runner cannot execute must be refused at load");
    assert!(
        matches!(error, PackError::UnsupportedStep { .. }),
        "expected UnsupportedStep, got: {error}"
    );
}

#[test]
fn model_step_role_is_declared_not_positional() {
    // The bug this contract exists to kill: `run_pack` decides what a
    // model step *means* by counting schema-bearing steps — 0 is
    // "normalise merchants", anything after is "classify them". A second
    // pack whose first model step asks something else entirely is read as
    // merchant cleanup, silently. The manifest has to say.
    let pack = ScratchPack::valid("declared-role");
    pack.amend_manifest(r#""role": "normalise""#, r#""role": "summarise""#);

    let error = load_pack(&pack.dir)
        .expect_err("a role this runner cannot execute must be refused at load");
    assert!(
        matches!(error, PackError::UnsupportedStep { .. }),
        "expected UnsupportedStep, got: {error}"
    );
}

#[test]
fn a_schema_bearing_model_step_must_say_which_role_it_plays() {
    // Omitting the role is the silent case, so it cannot be the lenient
    // one: a schema-bearing step with nothing declared is exactly the
    // pack that would be read positionally.
    let pack = ScratchPack::valid("missing-role");
    pack.amend_manifest(r#""role": "normalise", "#, "");

    let error =
        load_pack(&pack.dir).expect_err("a schema-bearing model step with no role must be refused");
    assert!(
        matches!(error, PackError::UnsupportedStep { .. }),
        "expected UnsupportedStep, got: {error}"
    );
}

#[test]
fn a_second_pack_cannot_be_silently_executed_as_subscription_audit() {
    // The test #120 asks for by name. The smallest letter-to-actions
    // shaped pack (#51): read a letter, pull out obligations and dates,
    // write a report. Every path resolves and every file exists, so it
    // passed the loader before this contract — and then `run_pack` would
    // have read its first schema-bearing step as "normalise merchants"
    // and run a subscription audit over a solicitor's letter.
    //
    // It must refuse at load, in language a person can act on, and never
    // reach merchant cleanup or recurrence detection.
    let pack = ScratchPack::valid("letter-to-actions");
    pack.amend_manifest(
        r#"{ "step": "model", "role": "normalise", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 },"#,
        r#"{ "step": "preprocess", "impl": "builtin:letter-text" },
           { "step": "model", "role": "obligations", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 },"#,
    );

    let error = load_pack(&pack.dir)
        .expect_err("a pack this runner cannot execute must refuse before it runs");
    let PackError::UnsupportedStep { step } = error else {
        panic!("expected UnsupportedStep, got: {error}");
    };
    // The preprocess builtin is the first thing it cannot do, and the
    // message names it rather than the pack.
    assert_eq!(step, "builtin:letter-text");

    let message = format!("{}", PackError::UnsupportedStep { step });
    assert!(
        message.contains("newer version of Kettle"),
        "the refusal should tell a person what to do about it: {message}"
    );
    // No jargon leaks into a message a person reads.
    for word in ["pipeline", "manifest", "enum", "variant"] {
        assert!(!message.contains(word), "{word:?} is jargon: {message}");
    }
}

/// #66: the shipped comparison pack. Held to the same contract as the
/// other two — it loads, and what it declares is what this runner can
/// actually execute.
#[test]
fn renewal_diff_manifest_loads() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/app.kttl.renewal-diff");

    let pack = load_pack(&dir).expect("the renewal pack loads");

    assert_eq!(pack.manifest.id, "app.kttl.renewal-diff");
    // Two documents, declared earlier-first: the diff resolves which
    // year is which from this order (#350), and reversing it reports a
    // price cut where there was a rise.
    let roles: Vec<&str> = pack
        .manifest
        .inputs
        .iter()
        .map(|input| input.role.as_str())
        .collect();
    assert_eq!(roles, ["previous", "renewal"]);
    assert!(
        pack.manifest
            .inputs
            .iter()
            .all(|input| input.count == runner::packs::Count::Exactly(1)),
        "a compared document is exactly one file"
    );
    assert_eq!(pack.manifest.capabilities, ["read"]);
    // It promises a report, and the template that renders one is on
    // disk beside the manifest (#66). The promise and the file are
    // asserted together: naming an output nothing can produce is the
    // failure the empty list used to prevent, and it stays prevented by
    // checking rather than by promising nothing.
    assert_eq!(pack.manifest.outputs, ["report.html"]);
    assert!(
        dir.join("report.html.tera").exists(),
        "a declared report with no template is a promise nothing keeps"
    );
    // #380: every term it models says what kind of value it can hold,
    // so a policy period read as a cover limit is a passage for a
    // person rather than a monetary finding.
    assert!(
        pack.manifest
            .value_kinds
            .get("cover_limit")
            .expect("a cover limit declares its kind")
            .holds("£1,000,000"),
        "a cover limit is money"
    );
}

// ── #350: the role and the diff step a two-document comparison needs ──

/// A comparison pack's shape (#66): two documents in, named terms out
/// of each, one diff across them. The closed sets are what stopped it
/// being writable, so this is the test that they opened by exactly the
/// width of this change and no further.
fn comparison_pack(name: &str) -> ScratchPack {
    let pack = ScratchPack::valid(name);
    // A terms schema with a real closed set, because the value_kinds
    // map (#380) is validated against the question as it is asked —
    // the enum the model can actually answer, not a list kept somewhere
    // else.
    std::fs::write(
        pack.dir.join("schemas/one.schema.json"),
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "terms": { "type": "array", "items": { "type": "object", "properties": {
                    "term": { "enum": ["premium", "cooling_off_period", "other"] },
                    "value": { "type": "string" }
                }, "required": ["term", "value"] } }
            }, "required": ["id", "terms"] } } },
            "required": ["results"] }"#,
    )
    .expect("write terms schema");
    pack.amend_manifest(
        r#""outputs": ["report.html"]"#,
        r#""value_kinds": { "premium": "money", "cooling_off_period": "duration" },
          "outputs": ["report.html"]"#,
    );
    pack.amend_manifest(
        r#"[{ "role": "statement", "label": "Your bank statements", "accept": ["text/csv"], "multiple": false }]"#,
        r#"[{ "role": "previous", "label": "Last year's policy", "accept": ["application/pdf"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["application/pdf"], "multiple": false }]"#,
    );
    pack.amend_manifest(
        r#"{ "step": "model", "role": "normalise", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 },"#,
        r#"{ "step": "preprocess", "impl": "builtin:document-text" },
           { "step": "model", "role": "policy-terms", "prompt": "prompts/one.md", "schema": "schemas/one.schema.json", "batch": 5 },
           { "step": "aggregate", "impl": "builtin:term-diff" },"#,
    );
    // The pipeline changed, so the copy's will reference follows it —
    // an unresolvable reference is refused, which is its own test.
    pack.amend_manifest(
        r#""steps": ["Grouping payments by merchant"]"#,
        r#""steps": ["Reading what each document says"]"#,
    );
    pack
}

#[test]
fn a_pack_may_declare_a_policy_terms_role_and_a_term_diff() {
    let pack = comparison_pack("term-diff");

    load_pack(&pack.dir).expect("a comparison pack is a pipeline this runner can execute");
}

// ── #380: what kind of value each term can hold ─────────────────────

#[test]
fn a_terms_pack_says_what_kind_of_value_each_term_holds() {
    // The guard is only as good as the declaration behind it, and a
    // missing declaration is the silent case: every term would be
    // checked against nothing, and the pack would look guarded. Same
    // reasoning as the kinds map (#253) — the runner cannot hold the
    // answer, because "a cover limit is money" is pack policy and a
    // runner that knew it would be pack-specific runner code (#51).
    let complete = comparison_pack("value-kinds-complete");
    load_pack(&complete.dir).expect("a complete value_kinds map loads");

    let missing = comparison_pack("value-kinds-missing");
    missing.amend_manifest(
        r#""value_kinds": { "premium": "money", "cooling_off_period": "duration" },"#,
        "",
    );
    let error = load_pack(&missing.dir).expect_err("a terms pack needs a value_kinds map");
    assert!(
        matches!(error, PackError::InvalidValueKinds { .. }),
        "expected InvalidValueKinds, got: {error}"
    );

    // A term the map does not cover. The dangerous half: an uncovered
    // term is one whose value nothing checks, and it fails open.
    let uncovered = comparison_pack("value-kinds-uncovered");
    uncovered.amend_manifest(r#", "cooling_off_period": "duration""#, "");
    let error = load_pack(&uncovered.dir).expect_err("every modelled term needs a kind");
    let PackError::InvalidValueKinds { reason } = error else {
        panic!("expected InvalidValueKinds");
    };
    assert!(
        reason.contains("cooling_off_period"),
        "names the gap: {reason}"
    );
}

#[test]
fn other_is_never_given_a_value_kind() {
    // `other` is a routing answer, not a value: it never pairs and
    // never reaches the diff, so a kind for it is a declaration about
    // something that has no value to check.
    let pack = comparison_pack("value-kinds-other");
    pack.amend_manifest(
        r#""value_kinds": { "premium""#,
        r#""value_kinds": { "other": "text", "premium""#,
    );
    let error = load_pack(&pack.dir).expect_err("`other` carries no value to check");
    let PackError::InvalidValueKinds { reason } = error else {
        panic!("expected InvalidValueKinds");
    };
    assert!(reason.contains("other"), "names the entry: {reason}");
}

#[test]
fn a_value_kind_outside_the_vocabulary_is_refused() {
    // An invented kind is a check nothing performs, arriving with the
    // appearance of one.
    // Refused while the manifest is being read rather than after, so
    // the sentence can name what the author actually wrote — the reason
    // the kinds are not a serde derive.
    let pack = comparison_pack("value-kinds-vocabulary");
    pack.amend_manifest(r#""premium": "money""#, r#""premium": "sterling""#);
    let error = load_pack(&pack.dir).expect_err("sterling is not a kind of value");
    let message = error.to_string();
    assert!(message.contains("sterling"), "names the kind: {message}");
    assert!(
        message.contains("money"),
        "and says what the kinds are: {message}"
    );
}

#[test]
fn a_value_kinds_key_no_term_can_produce_is_refused() {
    // A typo that sits silent until someone wonders why a term nobody
    // declared went unchecked.
    let pack = comparison_pack("value-kinds-stray");
    pack.amend_manifest(
        r#""value_kinds": { "premium""#,
        r#""value_kinds": { "hovercraft": "money", "premium""#,
    );
    let error = load_pack(&pack.dir).expect_err("a stray value_kinds key is refused");
    let PackError::InvalidValueKinds { reason } = error else {
        panic!("expected InvalidValueKinds");
    };
    assert!(reason.contains("hovercraft"), "names the stray: {reason}");
}

#[test]
fn an_unknown_aggregate_builtin_is_still_refused() {
    // The closed set opened by one entry, not by the category. A pack
    // naming a diff this runner does not have must still be refused at
    // load — the guarantee #120 bought is that the set is closed, and
    // widening it once must not quietly turn it into a suggestion.
    let pack = comparison_pack("unknown-diff");
    pack.amend_manifest(
        r#""impl": "builtin:term-diff""#,
        r#""impl": "builtin:policy-diff""#,
    );

    let error =
        load_pack(&pack.dir).expect_err("a diff this runner cannot execute must be refused");
    let PackError::UnsupportedStep { step } = error else {
        panic!("expected UnsupportedStep, got: {error}");
    };
    assert_eq!(step, "builtin:policy-diff");
}

#[test]
fn a_term_diff_needs_two_documents_to_compare() {
    // A diff over one document has nothing to pair against, and every
    // term in it would be reported as "added" — a renewal report
    // claiming every term is new. The manifest already says how many
    // documents the pack takes, so this is knowable at load.
    let pack = comparison_pack("one-sided-diff");
    pack.amend_manifest(
        r#"{ "role": "previous", "label": "Last year's policy", "accept": ["application/pdf"], "multiple": false },
            { "role": "renewal", "label": "This year's renewal", "accept": ["application/pdf"], "multiple": false }"#,
        r#"{ "role": "renewal", "label": "This year's renewal", "accept": ["application/pdf"], "multiple": false }"#,
    );

    let error = load_pack(&pack.dir).expect_err("a comparison of one document is not a comparison");
    let PackError::CannotCompare { roles } = error else {
        panic!("expected CannotCompare, got: {error}");
    };
    assert_eq!(roles, 1);

    let message = format!("{}", PackError::CannotCompare { roles });
    for word in ["pipeline", "manifest", "enum", "variant", "role"] {
        assert!(!message.contains(word), "{word:?} is jargon: {message}");
    }
}

#[test]
fn a_compared_document_cannot_be_several_documents() {
    // The same silent-reversal risk as an unstated role, one step
    // along: if "last year's policy" can be three files, which of them
    // the renewal is compared against is unstated, and the run does not
    // fail — it reports a diff against whichever arrived first.
    let pack = comparison_pack("multiple-previous");
    pack.amend_manifest(
        r#"{ "role": "previous", "label": "Last year's policy", "accept": ["application/pdf"], "multiple": false }"#,
        r#"{ "role": "previous", "label": "Last year's policy", "accept": ["application/pdf"], "multiple": true }"#,
    );

    let error = load_pack(&pack.dir)
        .expect_err("a comparison against an unstated one of several files must be refused");
    let PackError::AmbiguousComparison { role } = error else {
        panic!("expected AmbiguousComparison, got: {error}");
    };
    assert_eq!(role, "previous");
}

#[test]
fn role_names_match_the_runner() {
    // Two lists say what a role is: `MODEL_ROLES`, which load-time
    // validation accepts, and `ModelRole::declared`, which the runner
    // dispatches on. Drift between them is silent in the dangerous
    // direction — a role accepted at load with no arm to execute it
    // would reach the runner and fail mid-run, which is the whole thing
    // this contract exists to prevent.
    for role in runner::packs::MODEL_ROLES {
        assert!(
            runner::run::ModelRole::declared(Some(role)).is_some(),
            "{role:?} is accepted at load but the runner has no arm for it"
        );
    }
    // And the other way: an arm nothing can declare is dead code.
    assert!(runner::run::ModelRole::declared(Some("summarise")).is_none());
    assert!(runner::run::ModelRole::declared(None).is_none());
}

/// A scratch pack with a classify-role step, its category enum, and a
/// complete kinds map — the shape #253 requires of an audit pack.
fn classify_pack(name: &str) -> ScratchPack {
    let pack = ScratchPack::valid(name);
    std::fs::write(
        pack.dir.join("schemas/classify.schema.json"),
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "category": { "enum": ["streaming", "housing", "unknown"] },
                "confidence": { "enum": ["high", "medium", "low"] }
            }, "required": ["id", "category", "confidence"] } } },
            "required": ["results"] }"#,
    )
    .expect("write classify schema");
    pack.amend_manifest(
        r#"{ "step": "render""#,
        r#"{ "step": "model", "role": "classify", "prompt": "prompts/one.md", "schema": "schemas/classify.schema.json", "batch": 5 },
        { "step": "render""#,
    );
    pack.amend_manifest(
        r#""outputs": ["report.html"]"#,
        r#""kinds": { "streaming": "subscription", "housing": "utility", "unknown": "subscription" },
          "outputs": ["report.html"]"#,
    );
    pack
}

#[test]
fn a_classify_pack_carries_a_complete_kinds_map_or_does_not_load() {
    // #253: the runner derives a recurring merchant's kind from the
    // pack's category→kind map, so a classify pack without one is a
    // pipeline the runner cannot finish honestly — refused at load,
    // like every other unexecutable pipeline (#120).
    let complete = classify_pack("kinds-complete");
    load_pack(&complete.dir).expect("a complete kinds map loads");

    // No map at all.
    let missing = classify_pack("kinds-missing");
    missing.amend_manifest(
        r#""kinds": { "streaming": "subscription", "housing": "utility", "unknown": "subscription" },"#,
        "",
    );
    let error = load_pack(&missing.dir).expect_err("a classify pack needs a kinds map");
    assert!(
        matches!(error, PackError::InvalidKinds { .. }),
        "expected InvalidKinds, got: {error}"
    );

    // A category the map does not cover: silence is the dangerous case.
    let uncovered = classify_pack("kinds-uncovered");
    uncovered.amend_manifest(r#""housing": "utility", "#, "");
    let error = load_pack(&uncovered.dir).expect_err("every category must be mapped");
    let PackError::InvalidKinds { reason } = error else {
        panic!("expected InvalidKinds");
    };
    assert!(reason.contains("housing"), "names the gap: {reason}");
}

#[test]
fn a_kinds_value_outside_the_recurring_vocabulary_is_refused() {
    // The map answers one question: what is a *recurring* series of
    // this category? "one_off" is not an answer to it (a series
    // recurs), and an invented kind is a report nothing downstream
    // understands.
    let pack = classify_pack("kinds-vocabulary");
    pack.amend_manifest(
        r#""streaming": "subscription""#,
        r#""streaming": "one_off""#,
    );
    let error = load_pack(&pack.dir).expect_err("one_off cannot be a recurring kind");
    let PackError::InvalidKinds { reason } = error else {
        panic!("expected InvalidKinds");
    };
    assert!(reason.contains("streaming"), "names the entry: {reason}");
}

#[test]
fn a_kinds_key_no_category_can_produce_is_refused() {
    // A mapping for a category the schema cannot emit is a typo, and a
    // typo that sits silent until someone wonders why rent looks wrong.
    let pack = classify_pack("kinds-stray");
    pack.amend_manifest(
        r#""kinds": { "streaming""#,
        r#""kinds": { "hovercraft": "utility", "streaming""#,
    );
    let error = load_pack(&pack.dir).expect_err("a stray kinds key is refused");
    let PackError::InvalidKinds { reason } = error else {
        panic!("expected InvalidKinds");
    };
    assert!(reason.contains("hovercraft"), "names the stray: {reason}");
}

// ── #244: every pack says what it will do, in its own words ─────────

#[test]
fn a_pack_without_copy_is_refused_and_says_what_to_write() {
    // The missing declaration is the silent case (#380's argument):
    // optional copy means the shell keeps a fallback branch alive for
    // ever. So a pack with no copy block is a load error — one that
    // tells the author what to write, not just that something is absent.
    let scratch = ScratchPack::valid("no-copy");
    scratch.amend_manifest(SCRATCH_COPY, "");
    let error = load_pack(&scratch.dir).expect_err("a pack with no copy must not load");
    let PackError::MissingCopy = error else {
        panic!("expected MissingCopy, got {error:?}");
    };
    let message = error.to_string();
    for word in ["copy", "time", "will", "run_verb"] {
        assert!(message.contains(word), "{message:?} should name {word:?}");
    }
}

#[test]
fn a_will_entry_naming_an_unknown_step_is_refused() {
    // References must resolve; coverage is not forced (#244, decision
    // 2). The provenance.test.ts shape: prose is free to group steps,
    // but a named step that does not exist is a bug, not an opinion.
    let scratch = ScratchPack::valid("bad-will-step");
    scratch.amend_manifest(
        r#""steps": ["Grouping payments by merchant"]"#,
        r#""steps": ["Sorting merchants"]"#,
    );
    let error = load_pack(&scratch.dir).expect_err("an unresolvable will reference must not load");
    let PackError::UnknownWillStep { named, available } = &error else {
        panic!("expected UnknownWillStep, got {error:?}");
    };
    assert_eq!(named, "Sorting merchants");
    assert_eq!(
        available,
        &["Grouping payments by merchant", "Writing your report"]
    );
    let message = error.to_string();
    assert!(
        message.contains("Sorting merchants") && message.contains("Grouping payments by merchant"),
        "the message names the bad reference and the real labels: {message:?}"
    );
}

#[test]
fn a_will_entry_need_not_name_any_step() {
    // Decision 2's other half: prose stays free to say what a person
    // cares about without narrating the pipeline. No steps field, no
    // check.
    let scratch = ScratchPack::valid("free-will");
    scratch.amend_manifest(r#", "steps": ["Grouping payments by merchant"]"#, "");
    load_pack(&scratch.dir).expect("a will entry with no steps reference loads");
}
