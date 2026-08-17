//! #426: `kettle mutate` — run the semantic mutation harness over a
//! pack and answer one question: do the safeguards hold?
//!
//! Exit codes mirror `eval`'s posture: 0 every harmful mutant was
//! killed, 1 at least one survived (each named), 2 the harness could
//! not honestly run at all.

use cli::mutate::{self, Options};
use std::path::PathBuf;

/// A letter pack with a declared bar and gate — the guards hold, so
/// every mutant dies. Wholly invented.
const LETTER: &str = "3 March 2026\n\nPlease pay £120.00 to Harborne \
Parking Services within 14 days of the date of this letter.\n\nPlease \
also return the enclosed reply slip to Harborne Parking Services by \
28 March 2026.\n\nWe thank you for your co-operation.";

fn letter_pack(name: &str, with_gate: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-mutate-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["prompts", "schemas", "fixtures"] {
        std::fs::create_dir_all(dir.join(sub)).expect("create pack dirs");
    }
    let write = |relative: &str, content: &str| {
        std::fs::write(dir.join(relative), content).expect("write pack file");
    };
    let gate = if with_gate {
        r#""eval": { "obligations": 0.5 },
          "eval_gate": "per_fixture","#
    } else {
        ""
    };
    write(
        "pack.json",
        &format!(
            r#"{{
          "id": "app.kttl.test-mutate-cli",
          "name": "Mutate CLI test",
          "version": "0.0.1",
          "min_runner_version": "0.1.0",
          "inputs": [{{ "role": "letter", "label": "Your letters", "accept": ["text/plain"], "multiple": false }}],
          "capabilities": ["read"],
          "model": {{ "min_tier": "3b", "recommended_tier": "7b", "context": 8192, "temperature": 0 }},
          "copy": {{ "time": {{ "kind": "varies", "estimate": "by file", "on_this_computer": "This test pack has not been timed." }}, "will": [], "run_verb": "Run this task" }},
          "pipeline": [
            {{ "step": "preprocess", "impl": "builtin:document-text" }},
            {{ "step": "model", "role": "obligations", "prompt": "prompts/obligations.md", "schema": "schemas/obligations.schema.json", "batch": 8 }},
            {{ "step": "aggregate", "impl": "builtin:timeline-sort" }},
            {{ "step": "render", "template": "report.html.tera" }}
          ],
          {gate}
          "eval_metrics": ["extraction"],
          "eval_costs": {{ "review_rate": {{ "reason": "Tracks how many passages a person reads.", "date": "2026-08-07" }} }},
          "outputs": ["report.html"]
        }}"#
        ),
    );
    write(
        "prompts/obligations.md",
        "What does each passage oblige someone to do?\n{{ batch_json }}\n",
    );
    write(
        "schemas/obligations.schema.json",
        r#"{ "type": "object", "properties": { "results": { "type": "array", "items": {
            "type": "object", "properties": {
                "id": { "type": "integer" },
                "segment": { "type": "string" },
                "confidence": { "enum": ["high", "medium", "low"] },
                "obligations": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "enum": ["payment", "response", "attendance", "other"] },
                    "party": { "type": "string" },
                    "ask": { "type": "string" },
                    "deadline": { "type": "string" },
                    "anchor": { "type": "string" }
                }, "required": ["kind", "party", "ask", "deadline", "anchor"] } }
            }, "required": ["id", "segment", "confidence", "obligations"] } } },
            "required": ["results"] }"#,
    );
    write("report.html.tera", "<html></html>");
    write("fixtures/letter-01.txt", LETTER);
    write(
        "fixtures/letter-01.expected.json",
        r#"{
          "fixture_id": "mutate-cli-letter-01",
          "eval_set": "development",
          "obligations": [
            {
              "id": "mutate-cli-payment-01",
              "strata": [],
              "segment": "Please pay £120.00 to Harborne Parking Services within 14 days of the date of this letter.",
              "expect": {
                "kind": "payment",
                "party": "Harborne Parking Services",
                "deadline": "within 14 days",
                "anchor": "the date of this letter",
                "due": "2026-03-17"
              }
            },
            {
              "id": "mutate-cli-response-01",
              "strata": [],
              "segment": "Please also return the enclosed reply slip to Harborne Parking Services by 28 March 2026.",
              "expect": {
                "kind": "response",
                "party": "Harborne Parking Services",
                "deadline": "by 28 March 2026",
                "anchor": "28 March 2026",
                "due": "2026-03-28"
              }
            },
            {
              "id": "mutate-cli-closing-01",
              "strata": [],
              "segment": "We thank you for your co-operation.",
              "expect": null
            }
          ]
        }"#,
    );
    dir
}

/// The healthy case: a gated pack kills every mutant, the table names
/// each operator, and the exit code is 0.
#[test]
fn a_pack_whose_guards_hold_exits_zero_and_names_every_operator() {
    let dir = letter_pack("holds", true);
    let outcome = mutate::run(&Options {
        pack: "app.kttl.test-mutate-cli".to_owned(),
        packs_dir: dir.parent().unwrap().to_path_buf(),
        pack_dir: Some(dir.clone()),
    });

    assert_eq!(outcome.code, mutate::ExitCode::Ok, "{}", outcome.text);
    for operator in ["drop_obligation", "invent_obligation", "deadline_digit"] {
        assert!(
            outcome.text.contains(operator),
            "the table names {operator}: {}",
            outcome.text
        );
    }
    assert!(
        outcome.text.contains("survived: 0") || outcome.text.contains("0 survived"),
        "the summary states no survivors: {}",
        outcome.text
    );
}

/// An ungated pack cannot even measure mutants: a step nobody set a
/// bar for cannot clear it (#301), so the baseline replay fails and
/// the harness refuses at the sanity gate. That refusal is the
/// finding — exit 2, not a false zero.
#[test]
fn an_ungated_pack_is_refused_at_the_sanity_gate() {
    let dir = letter_pack("ungated", false);
    let outcome = mutate::run(&Options {
        pack: "app.kttl.test-mutate-cli".to_owned(),
        packs_dir: dir.parent().unwrap().to_path_buf(),
        pack_dir: Some(dir.clone()),
    });

    assert_eq!(
        outcome.code,
        mutate::ExitCode::CouldNotRun,
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("does not pass"),
        "the refusal says why: {}",
        outcome.text
    );
}

/// One survivor for taxonomy tests: which operator, where, and how
/// many scored items it moved.
fn survivor(
    operator: runner::eval::mutation::MutationOperator,
    site: &str,
    affected_items: &[&str],
) -> runner::eval::mutation::MutationRecord {
    runner::eval::mutation::MutationRecord {
        operator,
        source_digest: "blake3:prompt:test".to_owned(),
        site: site.to_owned(),
        before: serde_json::json!({"kind": "payment"}),
        after: serde_json::Value::Null,
        affected_items: affected_items
            .iter()
            .map(|item| (*item).to_owned())
            .collect(),
        verdict_moved: false,
        contained_in_review: false,
        retry_demanded: false,
        cost_shift: false,
        killed: false,
    }
}

fn report_of(
    records: Vec<runner::eval::mutation::MutationRecord>,
) -> runner::eval::mutation::MutationReport {
    runner::eval::mutation::MutationReport {
        mutation_version: runner::eval::mutation::MUTATION_VERSION,
        scoring_version: runner::eval::SCORING_VERSION,
        pack: "app.kttl.test-mutate-cli".to_owned(),
        records,
    }
}

/// A declaration like the renewal pack's, scaled down: one family of
/// ceiling-tolerated wrong values, one of already-contained claims.
fn declaration() -> runner::eval::mutation::ExpectedSurvivors {
    use runner::eval::mutation::{ExpectedFamily, ExpectedSurvivors, Moved, MutationOperator};
    ExpectedSurvivors {
        families: vec![
            ExpectedFamily {
                name: "ceiling-tolerated single errors".to_owned(),
                count: 2,
                operators: vec![MutationOperator::WrongValueFromMultiValueQuote],
                moved: Moved::OneItem,
                reason: "the gate honestly absorbs one error".to_owned(),
            },
            ExpectedFamily {
                name: "already-contained claims".to_owned(),
                count: 1,
                operators: vec![MutationOperator::UnmodelledTerm],
                moved: Moved::Nothing,
                reason: "already review-routed in the baseline".to_owned(),
            },
        ],
        notes: Vec::new(),
    }
}

/// The survivor path: with nothing declared, one mutant nobody caught
/// turns the exit code and puts the operator, site and missed
/// containment in the table.
#[test]
fn a_surviving_mutant_is_named_and_exits_one() {
    use runner::eval::mutation::{ExpectedSurvivors, MutationOperator};

    let report = report_of(vec![survivor(
        MutationOperator::DropObligation,
        "results[1].obligations[0]",
        &[],
    )]);

    let outcome = mutate::outcome_of(&report, &ExpectedSurvivors::default());

    assert_eq!(outcome.code, mutate::ExitCode::Survivors);
    assert!(
        outcome.text.contains("drop_obligation")
            && outcome.text.contains("results[1].obligations[0]"),
        "the survivor is named with its site: {}",
        outcome.text
    );
}

/// #484: the exit code means "something *new* survived". Survivors
/// matching every declared family plus one new site exit 1, and the
/// output names the undeclared family — the declared counts holding is
/// no cover for it.
#[test]
fn an_undeclared_survivor_family_fails_even_when_the_declared_count_holds() {
    use runner::eval::mutation::MutationOperator;

    let report = report_of(vec![
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[0].terms[0]",
            &["item-a"],
        ),
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[1].terms[0]",
            &["item-b"],
        ),
        survivor(MutationOperator::UnmodelledTerm, "results[2].terms[0]", &[]),
        // The new site: an operator no declared family covers.
        survivor(
            MutationOperator::DropObligation,
            "results[3].obligations[0]",
            &["item-c"],
        ),
    ]);

    let outcome = mutate::outcome_of(&report, &declaration());

    assert_eq!(
        outcome.code,
        mutate::ExitCode::Survivors,
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("undeclared") && outcome.text.contains("drop_obligation"),
        "the undeclared family is named: {}",
        outcome.text
    );
    assert!(
        outcome.text.contains("results[3].obligations[0]"),
        "the new survivor is named with its site: {}",
        outcome.text
    );
}

/// The sibling: every survivor inside a declared family, no family
/// over its count — nothing new survived, so the exit code is 0 and
/// the output says what was tolerated rather than listing 261 lines a
/// person must eyeball against memory.
#[test]
fn survivors_inside_the_declared_families_exit_zero() {
    use runner::eval::mutation::MutationOperator;

    let report = report_of(vec![
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[0].terms[0]",
            &["item-a"],
        ),
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[1].terms[0]",
            &["item-b"],
        ),
        survivor(MutationOperator::UnmodelledTerm, "results[2].terms[0]", &[]),
    ]);

    let outcome = mutate::outcome_of(&report, &declaration());

    assert_eq!(outcome.code, mutate::ExitCode::Ok, "{}", outcome.text);
    for family in [
        "ceiling-tolerated single errors",
        "already-contained claims",
    ] {
        assert!(
            outcome.text.contains(family),
            "the tolerated family is named: {}",
            outcome.text
        );
    }
    assert!(
        outcome
            .text
            .contains("nothing survived beyond the declared expectations"),
        "the summary says nothing new survived: {}",
        outcome.text
    );
}

/// A declared family above its count is something new surviving too —
/// exit 1, and the family over its count is named.
#[test]
fn a_declared_family_over_its_count_fails_and_is_named() {
    use runner::eval::mutation::MutationOperator;

    let report = report_of(vec![
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[0].terms[0]",
            &["item-a"],
        ),
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[1].terms[0]",
            &["item-b"],
        ),
        survivor(
            MutationOperator::WrongValueFromMultiValueQuote,
            "results[2].terms[0]",
            &["item-c"],
        ),
        survivor(MutationOperator::UnmodelledTerm, "results[3].terms[0]", &[]),
    ]);

    let outcome = mutate::outcome_of(&report, &declaration());

    assert_eq!(
        outcome.code,
        mutate::ExitCode::Survivors,
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("ceiling-tolerated single errors")
            && outcome.text.contains("over its declared count"),
        "the family over its count is named: {}",
        outcome.text
    );
}

/// A declaration the harness cannot read is exit 2, never quietly an
/// empty one — treating it as empty would flip the exit code's meaning.
#[test]
fn an_unreadable_declaration_is_refused_not_ignored() {
    let dir = letter_pack("bad-declaration", true);
    std::fs::write(dir.join("expected-survivors.json"), "{ not json").expect("write declaration");

    let outcome = mutate::run(&Options {
        pack: "app.kttl.test-mutate-cli".to_owned(),
        packs_dir: dir.parent().unwrap().to_path_buf(),
        pack_dir: Some(dir.clone()),
    });

    assert_eq!(
        outcome.code,
        mutate::ExitCode::CouldNotRun,
        "{}",
        outcome.text
    );
    assert!(
        outcome.text.contains("expected-survivors.json"),
        "the refusal names the declaration: {}",
        outcome.text
    );
}

/// The dishonest case: a pack that does not load exits 2 — the
/// measurement could not be made, which is not the same as surviving.
#[test]
fn an_unloadable_pack_exits_two() {
    let dir =
        std::env::temp_dir().join(format!("kettle-mutate-cli-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let outcome = mutate::run(&Options {
        pack: "app.kttl.nowhere".to_owned(),
        packs_dir: dir.clone(),
        pack_dir: None,
    });

    assert_eq!(outcome.code, mutate::ExitCode::CouldNotRun);
}
