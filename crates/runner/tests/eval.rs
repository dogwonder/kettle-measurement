//! Eval report types and the verdict rules (#37). The verdict is the
//! one piece of judgement in the harness — "is this model good enough
//! for this pack?" — so its boundaries are tested exactly, not near
//! enough.
//!
//! Rules: Pass is every model step at or above its pack threshold and
//! end-to-end at or above 0.95. Review is tracked as cost, never used as
//! a quality gate. Fail is anything below a quality threshold.

use chrono::NaiveDate;
use runner::eval::{
    classification_metrics, paired_classification_comparison, Classification,
    ClassificationOutcome, ClassificationStratum, ConfidentWrongCeiling, EvalMetric, EvalReport,
    FixtureResult, HarmClass, MachineInfo, MetricReport, ModelInfo, Perf, ProportionEstimate,
    ScoredDecision, ScoredItem, StepScore, Thresholds, Tier, Verdict, END_TO_END_BAR,
};
use runner::eval::{decisions_needed, GateOutcome};
use runner::kinds::KindFrom;
use std::collections::BTreeMap;

/// The subscription-audit quality thresholds, plus the retired review
/// key to prove older installed manifests cannot revive that gate.
fn pack_thresholds() -> Thresholds {
    Thresholds::from_eval(&BTreeMap::from([
        ("normalise".to_string(), 0.85),
        ("classify".to_string(), 0.90),
        ("max_review_rate".to_string(), 0.15),
    ]))
}

fn step(score: f32) -> StepScore {
    StepScore {
        score,
        expected: 100,
        correct: (score * 100.0).round() as usize,
    }
}

fn perf() -> Perf {
    Perf {
        wall_ms: 250_000,
        model_ms: 240_000,
        tokens_per_second: 18.4,
        peak_rss_mb: 3_200,
    }
}

/// #237 Stage 2's first failing test. A bare 0.90 conceals whether it
/// means 9/10 or 90/100; the estimate therefore owns its sample size
/// and interval wherever it travels.
#[test]
fn classification_metrics_report_n_and_wilson_interval() {
    let estimate = ProportionEstimate::from_counts(90, 100);

    assert_eq!(estimate.successes, 90);
    assert_eq!(estimate.n, 100);
    assert_eq!(estimate.estimate, Some(0.9));
    let interval = estimate.wilson_95.expect("n > 0 has an interval");
    assert!((interval.low - 0.8256).abs() < 0.0001, "{interval:?}");
    assert!((interval.high - 0.9448).abs() < 0.0001, "{interval:?}");

    let undefined = ProportionEstimate::from_counts(0, 0);
    assert_eq!(undefined.estimate, None);
    assert_eq!(undefined.wilson_95, None);
}

/// The same item, said to be the same decision as every other row
/// carrying this key (#310).
fn keyed(mut item: ScoredItem, decision_key: &str) -> ScoredItem {
    item.decision_key = decision_key.to_owned();
    item
}

fn scored_classification(
    ordinal: usize,
    expected_kind: &str,
    actual_kind: Option<&str>,
    strata: &[&str],
) -> ScoredItem {
    scored_classification_from(
        ordinal,
        expected_kind,
        actual_kind,
        strata,
        KindFrom::CategoryMap,
    )
}

/// The same, saying which decision produced the kind (#272).
fn scored_classification_from(
    ordinal: usize,
    expected_kind: &str,
    actual_kind: Option<&str>,
    strata: &[&str],
    kind_from: KindFrom,
) -> ScoredItem {
    let actual = match actual_kind {
        Some(kind) => ClassificationOutcome::Classified {
            classification: Classification {
                kind: kind.to_owned(),
                category: "example".to_owned(),
            },
        },
        None => ClassificationOutcome::NeedsReview {
            proposed: None,
            reason: "A person decides.".to_owned(),
        },
    };
    ScoredItem {
        id: format!("app.kttl.test/mixed-cases-01/scored-case-{ordinal:02}"),
        item_id: format!("scored-case-{ordinal:02}"),
        pack: "app.kttl.test".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:test".to_owned(),
        fixture: "mixed.csv".to_owned(),
        fixture_id: "mixed-cases-01".to_owned(),
        strata: strata.iter().map(|stratum| (*stratum).to_owned()).collect(),
        raw_input: format!("CASE {ordinal}"),
        decision_key: format!("CASE {ordinal}"),
        decision: ScoredDecision::Classification {
            expected: Classification {
                kind: expected_kind.to_owned(),
                category: "example".to_owned(),
            },
            actual,
            kind_from: Some(kind_from),
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: Vec::new(),
    }
}

/// #310: a ceiling counts decisions, not rows.
///
/// The same merchant string, asked again, is the same question — at
/// temperature 0 it gets the same answer, and 87% of repeated merchants
/// in the 7B run did. Wilson assumes independent trials, so scoring
/// rows lets a bed clear a ceiling on repetition rather than on
/// evidence: `annual-subscription-once-yearly` read 0/110 and is 19
/// merchants, against a 5% ceiling that needs 73 clean decisions.
#[test]
fn a_ceiling_is_judged_on_distinct_decisions_not_repeated_rows() {
    let mut items: Vec<ScoredItem> = Vec::new();
    // One merchant, ten rows, wrong every time: one wrong decision.
    for ordinal in 1..=10 {
        items.push(keyed(
            scored_classification(ordinal, "subscription", Some("regular_spend"), &["mixed"]),
            "Backblaze",
        ));
    }
    // Two more merchants, right, one row each.
    items.push(keyed(
        scored_classification(11, "subscription", Some("subscription"), &["mixed"]),
        "Netflix",
    ));
    items.push(keyed(
        scored_classification(12, "subscription", Some("subscription"), &["mixed"]),
        "Spotify",
    ));

    let metrics = classification_metrics(&items);
    let subscription = &metrics.overall.harm_classes[&HarmClass::Subscription];

    assert_eq!(
        subscription.confident_wrong_rate,
        ProportionEstimate::from_counts(10, 12),
        "the row rate stays, as exposure: ten of twelve statement lines were wrong"
    );
    assert_eq!(
        subscription.confident_wrong_distinct,
        ProportionEstimate::from_counts(1, 3),
        "but one merchant asked ten times is one decision, and it is the decision a ceiling gates"
    );

    // And the gate reads the distinct estimate, not the row one.
    let declarations = BTreeMap::from([(
        "mixed".to_owned(),
        ClassificationStratum {
            description: "Everything.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Appliance-risk ceiling.".to_owned(),
                    date: NaiveDate::from_ymd_opt(2026, 7, 29).expect("date"),
                },
            )]),
        },
    )]);
    let gated = classification_metrics(&items).with_gates(&declarations);
    let gate = &gated.gates["mixed"][&HarmClass::Subscription];
    assert_eq!(gate.observed, ProportionEstimate::from_counts(1, 3));
    assert!(!gate.outcome.clears());
}

/// #310: a ceiling the bed cannot support is not a ceiling the model
/// breached, and the table must not say the same word for both.
///
/// The case that produced this rule: the letter pack made **zero**
/// errors on 207 distinct decisions, against a 1% ceiling needing 381,
/// so the gate could not be met by any result whatsoever. Reporting that
/// as FAIL says the model got it wrong; what happened is that the bed
/// could not prove it right. (That pack now declares 2%, which its bed
/// can carry — #315. The rule outlives the instance, and the ceiling
/// below is synthetic so this test does not move when a pack does.)
#[test]
fn a_ceiling_the_bed_cannot_support_is_unproven_not_failed() {
    let declarations = BTreeMap::from([(
        "mixed".to_owned(),
        ClassificationStratum {
            description: "Everything.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Appliance-risk ceiling.".to_owned(),
                    date: NaiveDate::from_ymd_opt(2026, 7, 29).expect("date"),
                },
            )]),
        },
    )]);

    // Ten distinct merchants, every one right. A 5% ceiling needs 73.
    let spotless: Vec<ScoredItem> = (1..=10)
        .map(|ordinal| {
            keyed(
                scored_classification(ordinal, "subscription", Some("subscription"), &["mixed"]),
                &format!("Merchant {ordinal}"),
            )
        })
        .collect();
    let gate = &classification_metrics(&spotless)
        .with_gates(&declarations)
        .gates["mixed"][&HarmClass::Subscription];
    assert_eq!(
        gate.outcome,
        GateOutcome::Unproven {
            decisions_needed: 73
        },
        "no errors at all, and still not enough evidence to assert a 5% ceiling"
    );
    assert!(
        !gate.outcome.clears(),
        "unproven must not count as cleared: you cannot claim what you cannot show"
    );

    // Enough decisions, and one of them wrong beyond the ceiling: that
    // is a real breach, and it keeps the word FAIL to itself.
    let mut breached: Vec<ScoredItem> = (1..=80)
        .map(|ordinal| {
            keyed(
                scored_classification(ordinal, "subscription", Some("subscription"), &["mixed"]),
                &format!("Merchant {ordinal}"),
            )
        })
        .collect();
    for (ordinal, slot) in breached.iter_mut().enumerate().take(10) {
        *slot = keyed(
            scored_classification(ordinal, "subscription", Some("regular_spend"), &["mixed"]),
            &format!("Merchant {ordinal}"),
        );
    }
    let gate = &classification_metrics(&breached)
        .with_gates(&declarations)
        .gates["mixed"][&HarmClass::Subscription];
    assert_eq!(gate.outcome, GateOutcome::Fail);
}

/// A bed too small to *prove* a ceiling may still be large enough to
/// *disprove* it, and the two are not the same question.
///
/// `decisions_needed` is derived for one direction only: with zero
/// errors Wilson's upper bound is `3.84/(n + 3.84)`, so a ceiling of
/// `c` is unreachable below `3.84/c - 3.84` decisions however well the
/// model does. That is the arithmetic of demonstrating *compliance*,
/// and it is right. It says nothing about demonstrating a *breach* —
/// which needs far fewer decisions, because a rate three times the
/// ceiling separates from it much sooner than a rate just under it.
///
/// So when the Wilson **lower** bound already exceeds the ceiling, the
/// evidence has answered the question: at 95% confidence the true rate
/// is over the line. Reporting that as UNPROVEN tells a reader "we
/// could not tell", when what happened is that we could, and the answer
/// was bad. The 24 August subscription recordings are the instance —
/// the 9B's pooled `subscription` gate read 0.16 over 32 decisions with
/// a lower bound of 0.069 against a 0.05 ceiling, and said UNPROVEN.
#[test]
fn a_ceiling_the_bed_can_disprove_fails_even_when_it_could_not_prove_it() {
    let declarations = BTreeMap::from([(
        "mixed".to_owned(),
        ClassificationStratum {
            description: "Everything.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Appliance-risk ceiling.".to_owned(),
                    date: NaiveDate::from_ymd_opt(2026, 7, 29).expect("date"),
                },
            )]),
        },
    )]);

    // Eight distinct decisions, two of them confidently wrong. A 5%
    // ceiling needs 73 decisions to prove, so this bed can never say
    // PASS — but 2/8 puts the Wilson lower bound at 0.071, above the
    // ceiling, so it can say FAIL.
    let mut breached: Vec<ScoredItem> = (1..=8)
        .map(|ordinal| {
            keyed(
                scored_classification(ordinal, "subscription", Some("subscription"), &["mixed"]),
                &format!("Merchant {ordinal}"),
            )
        })
        .collect();
    for (ordinal, slot) in breached.iter_mut().enumerate().take(2) {
        *slot = keyed(
            scored_classification(ordinal, "subscription", Some("regular_spend"), &["mixed"]),
            &format!("Merchant {ordinal}"),
        );
    }

    let metrics = classification_metrics(&breached).with_gates(&declarations);
    let gate = &metrics.gates["mixed"][&HarmClass::Subscription];

    let interval = gate
        .observed
        .wilson_95
        .expect("eight decisions carry an interval");
    assert!(
        interval.low > gate.max_wilson_95,
        "the case only means anything if the lower bound really is above \
         the ceiling: low {} vs ceiling {}",
        interval.low,
        gate.max_wilson_95
    );
    assert!(
        gate.observed.n < decisions_needed(gate.max_wilson_95),
        "and only if the bed is genuinely too small to prove the ceiling"
    );

    assert_eq!(
        gate.outcome,
        GateOutcome::Fail,
        "a breach the evidence establishes is a breach, not an absence of evidence"
    );
    assert!(!gate.outcome.clears(), "and it certainly does not clear");
}

/// One error never fails a ceiling, however small the bed.
///
/// This is the boundary on the rule above, and it is not a statistical
/// nicety — it is CLAUDE.md's `kettle mutate` decision of 10 August
/// 2026 held to: *the gates tolerate single errors at the declared risk
/// appetite*, and the v13 census enumerates 237 survivors that are
/// exactly that. A rule letting one wrong decision fail a gate would
/// rewrite that census as a side effect of a scoring change.
///
/// It is also where Wilson is least worth trusting. A single wrong
/// decision out of one puts the lower bound at 0.207, which would fail
/// every ceiling Kettle declares — on one observation. A 5% rate is not
/// expressible in one decision at all: 0% and 100% are the only
/// readings the denominator can produce.
#[test]
fn one_wrong_decision_never_fails_a_ceiling() {
    let declarations = BTreeMap::from([(
        "mixed".to_owned(),
        ClassificationStratum {
            description: "Everything.".to_owned(),
            classes: BTreeMap::from([(
                HarmClass::Subscription,
                ConfidentWrongCeiling {
                    max_wilson_95: 0.05,
                    reason: "Appliance-risk ceiling.".to_owned(),
                    date: NaiveDate::from_ymd_opt(2026, 7, 29).expect("date"),
                },
            )]),
        },
    )]);

    // Three decisions, one of them wrong. Wilson's lower bound is 0.061,
    // over the 0.05 ceiling — so the rule above would fail this gate on
    // a single error if nothing stopped it.
    let mut one_error: Vec<ScoredItem> = (1..=3)
        .map(|ordinal| {
            keyed(
                scored_classification(ordinal, "subscription", Some("subscription"), &["mixed"]),
                &format!("Merchant {ordinal}"),
            )
        })
        .collect();
    one_error[0] = keyed(
        scored_classification(0, "subscription", Some("regular_spend"), &["mixed"]),
        "Merchant 0",
    );

    let metrics = classification_metrics(&one_error).with_gates(&declarations);
    let gate = &metrics.gates["mixed"][&HarmClass::Subscription];

    let interval = gate
        .observed
        .wilson_95
        .expect("three decisions carry an interval");
    assert!(
        interval.low > gate.max_wilson_95,
        "the test only bites while the lower bound is over the ceiling: \
         low {} vs ceiling {}",
        interval.low,
        gate.max_wilson_95
    );

    assert_eq!(
        gate.outcome,
        GateOutcome::Unproven {
            decisions_needed: 73
        },
        "a single error is tolerated at the declared risk appetite, so \
         this bed still has nothing to say about the ceiling"
    );
}

/// #272: the confident-wrong cell must say which decisions produced it.
///
/// Attribution by hand got this backwards twice on the 7B run — 91%
/// cadence claimed, 90% category actual — because `regular_spend` is
/// what both branches emit and the item recorded only the string. A
/// gate that fails should be able to say what to fix without anybody
/// reading raw exchanges.
#[test]
fn the_confident_wrong_cell_says_which_decisions_produced_it() {
    let items = vec![
        // Three subscriptions confidently denied: two because the
        // category map was fed a wrong category, one because cadence
        // found no series though the payments looked periodic.
        scored_classification_from(
            1,
            "subscription",
            Some("regular_spend"),
            &["mixed"],
            KindFrom::CategoryMap,
        ),
        scored_classification_from(
            2,
            "subscription",
            Some("regular_spend"),
            &["mixed"],
            KindFrom::CategoryMap,
        ),
        scored_classification_from(
            3,
            "subscription",
            Some("one_off"),
            &["mixed"],
            KindFrom::CadenceDespitePeriodic,
        ),
        // Right answers and review-routed items contribute nothing to
        // the decomposition: only the silent cell is decomposed.
        scored_classification_from(
            4,
            "subscription",
            Some("subscription"),
            &["mixed"],
            KindFrom::CategoryMap,
        ),
        scored_classification_from(5, "subscription", None, &["mixed"], KindFrom::Cadence),
    ];

    let metrics = classification_metrics(&items);
    let subscription = &metrics.overall.harm_classes[&HarmClass::Subscription];

    assert_eq!(
        subscription.confident_wrong_rate,
        ProportionEstimate::from_counts(3, 5)
    );
    assert_eq!(
        subscription.confident_wrong_by_path,
        BTreeMap::from([
            (KindFrom::CategoryMap, 2),
            (KindFrom::CadenceDespitePeriodic, 1),
        ]),
        "two category errors and one cadence error, said rather than inferred"
    );
    assert_eq!(
        metrics.strata["mixed"].harm_classes[&HarmClass::Subscription].confident_wrong_by_path,
        subscription.confident_wrong_by_path,
        "the decomposition is per stratum, not only overall"
    );
}

#[test]
fn classification_metrics_preserve_all_six_review_aware_cells() {
    let items = vec![
        scored_classification(1, "subscription", Some("subscription"), &["mixed"]),
        scored_classification(2, "subscription", None, &["mixed", "annual-renewal"]),
        scored_classification(3, "subscription", Some("regular_spend"), &["mixed"]),
        scored_classification(4, "regular_spend", None, &["mixed"]),
        scored_classification(5, "regular_spend", Some("subscription"), &["mixed"]),
        scored_classification(6, "regular_spend", Some("regular_spend"), &["mixed"]),
    ];

    let metrics = classification_metrics(&items);
    let subscription = &metrics.overall.kinds["subscription"];

    assert_eq!(
        subscription.precision,
        ProportionEstimate::from_counts(1, 2)
    );
    assert_eq!(subscription.recall, ProportionEstimate::from_counts(2, 3));
    assert_eq!(
        subscription.confident_wrong_rate,
        ProportionEstimate::from_counts(1, 3),
        "the silent confident miss is the cell a later per-pack ceiling gates"
    );
    assert_eq!(subscription.cells.expected_class_asserted_class, 1);
    assert_eq!(subscription.cells.expected_class_needs_review, 1);
    assert_eq!(subscription.cells.expected_class_asserted_other, 1);
    assert_eq!(subscription.cells.expected_other_needs_review, 1);
    assert_eq!(subscription.cells.expected_other_asserted_class, 1);
    assert_eq!(subscription.cells.expected_other_asserted_other, 1);

    let annual = &metrics.strata["annual-renewal"];
    assert_eq!(annual.n, 1);
    assert_eq!(
        annual.kinds["subscription"].recall,
        ProportionEstimate::from_counts(1, 1),
        "needs-review is surfaced recall, not a silent miss"
    );
    assert_eq!(
        annual.kinds["regular_spend"].recall,
        ProportionEstimate::from_counts(0, 0),
        "an absent class is undefined rather than zero"
    );
}

#[test]
fn paired_comparison_uses_exact_mcnemar_on_changed_item_outcomes() {
    let before: Vec<_> = (1..=6)
        .map(|ordinal| {
            scored_classification(ordinal, "subscription", Some("subscription"), &["clean"])
        })
        .collect();
    let after: Vec<_> = (1..=6)
        .map(|ordinal| {
            scored_classification(ordinal, "subscription", Some("regular_spend"), &["clean"])
        })
        .collect();

    let comparison = paired_classification_comparison(&before, &after);

    assert_eq!(comparison.regressions, 6);
    assert_eq!(comparison.improvements, 0);
    assert_eq!(comparison.matched, 6);
    assert_eq!(comparison.discordant, 6);
    assert_eq!(
        comparison.discordant_item_ids,
        before
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>()
    );
    assert!(comparison.can_reach_significance);
    assert!(
        (comparison.exact_two_sided_p - 0.03125).abs() < f64::EPSILON,
        "{comparison:?}"
    );

    let too_small = paired_classification_comparison(&before[..5], &after[..5]);
    assert_eq!(too_small.discordant, 5);
    assert!(!too_small.can_reach_significance);
    assert_eq!(too_small.exact_two_sided_p, 0.0625);
}

/// A fixture result with the given step scores, end-to-end score and
/// needs-review rate.
fn fixture(
    normalise: f32,
    classify: f32,
    end_to_end: f32,
    needs_review_rate: f32,
) -> FixtureResult {
    FixtureResult {
        fixture: "statement-01.csv".to_string(),
        step_scores: BTreeMap::from([
            ("normalise".to_string(), step(normalise)),
            ("classify".to_string(), step(classify)),
        ]),
        items: Vec::new(),
        containment: Default::default(),
        end_to_end,
        needs_review_rate,
        retries: 0,
        perf: Some(perf()),
        stability: None,
    }
}

/// #237 Stage 1 addendum's first failing test. Identity, provenance,
/// exchanges and diffability belong to the runner; classification is
/// only one decision shape a pack may ask it to carry.
#[test]
fn per_item_records_are_metric_shape_neutral() {
    let item = ScoredItem {
        id: "app.kttl.subscription-audit/clean-everyday-01/monthly-video-streaming-01".to_owned(),
        item_id: "monthly-video-streaming-01".to_owned(),
        pack: "app.kttl.subscription-audit".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:example".to_owned(),
        fixture: "statement-01.csv".to_owned(),
        fixture_id: "clean-everyday-01".to_owned(),
        strata: vec!["clean".to_owned()],
        raw_input: "NETFLIX.COM".to_owned(),
        decision_key: "Netflix".to_owned(),
        decision: ScoredDecision::Classification {
            expected: Classification {
                kind: "subscription".to_owned(),
                category: "streaming".to_owned(),
            },
            actual: ClassificationOutcome::Classified {
                classification: Classification {
                    kind: "subscription".to_owned(),
                    category: "streaming".to_owned(),
                },
            },
            kind_from: Some(runner::kinds::KindFrom::CategoryMap),
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: Vec::new(),
    };
    assert_eq!(item.decision.metric(), EvalMetric::Classification);

    let mut result = fixture(1.0, 1.0, 1.0, 0.0);
    result.items.push(item);
    let json = serde_json::to_value(result).expect("scored fixture serialises");

    assert_eq!(json["items"][0]["metric"], "classification");
    assert!(
        json.get("classifications").is_none(),
        "the runner's item container must not assume one metric shape: {json}"
    );
}

#[test]
fn stage_one_classification_records_remain_readable() {
    let legacy = serde_json::json!({
        "fixture": "statement-01.csv",
        "step_scores": {},
        "classifications": [{
            "id": "app.kttl.subscription-audit/clean-everyday-01/monthly-video-streaming-01",
            "item_id": "monthly-video-streaming-01",
            "pack": "app.kttl.subscription-audit",
            "pack_version": "1.0.0",
            "prompt_version": "blake3:example",
            "fixture": "statement-01.csv",
            "fixture_id": "clean-everyday-01",
            "strata": ["clean"],
            "raw_merchant": "NETFLIX.COM",
            "expected": {"kind": "subscription", "category": "streaming"},
            "actual": {
                "state": "classified",
                "classification": {"kind": "subscription", "category": "streaming"}
            },
            "exchanges": []
        }],
        "end_to_end": 1.0,
        "needs_review_rate": 0.0,
        "perf": {
            "wall_ms": 1,
            "model_ms": 1,
            "tokens_per_second": 1.0,
            "peak_rss_mb": 1,
            "retries": 2
        }
    });

    let result: FixtureResult =
        serde_json::from_value(legacy).expect("the just-shipped Stage 1 shape remains readable");

    assert_eq!(
        result.items[0].decision.metric(),
        EvalMetric::Classification
    );
    assert_eq!(result.items[0].raw_input, "NETFLIX.COM");
    assert_eq!(
        result.retries, 2,
        "the durable retry count is lifted out of a legacy perf block"
    );
}

#[test]
fn an_explicit_fixture_retry_count_wins_over_the_legacy_location() {
    let fixture = serde_json::json!({
        "fixture": "statement-01.csv",
        "step_scores": {},
        "end_to_end": 1.0,
        "needs_review_rate": 0.0,
        "retries": 0,
        "perf": {
            "wall_ms": 1,
            "model_ms": 1,
            "tokens_per_second": 1.0,
            "peak_rss_mb": 1,
            "retries": 3
        }
    });

    let result: FixtureResult = serde_json::from_value(fixture).expect("fixture reads");
    assert_eq!(
        result.retries, 0,
        "the current wire shape is authoritative when both spellings exist"
    );
}

// --- Thresholds --------------------------------------------------------

#[test]
fn thresholds_ignore_the_retired_review_rate_target() {
    let thresholds = pack_thresholds();

    assert_eq!(thresholds.step("normalise"), Some(0.85));
    assert_eq!(thresholds.step("classify"), Some(0.90));
    // Review remains a reported cost, never a score or verdict bar.
    assert_eq!(thresholds.step("max_review_rate"), None);
}

// --- verdict -----------------------------------------------------------

mod verdict {
    use super::*;

    /// #237 Stage 3's first failing test. A pack passes only when every
    /// declared class in every declared stratum clears its own
    /// provenance-bearing ceiling. One strong slice cannot average away
    /// another slice's silent misses.
    #[test]
    fn pack_verdict_fails_when_any_declared_class_stratum_ceiling_fails() {
        let ceiling = ConfidentWrongCeiling {
            max_wilson_95: 0.05,
            reason: "Initial appliance-risk ceiling.".to_owned(),
            date: "2026-07-29".parse().expect("date"),
        };
        let stratum = |description: &str| ClassificationStratum {
            description: description.to_owned(),
            classes: BTreeMap::from([(HarmClass::Subscription, ceiling.clone())]),
        };
        let thresholds = pack_thresholds().with_classification_strata(BTreeMap::from([
            ("clean".to_owned(), stratum("Plain merchant strings.")),
            (
                "messy-merchant-strings".to_owned(),
                stratum("Noisy payment descriptors."),
            ),
        ]));

        let report_with = |messy_wrong: usize| {
            let mut items = Vec::new();
            for ordinal in 1..=110 {
                items.push(scored_classification(
                    ordinal,
                    "subscription",
                    Some("subscription"),
                    &["clean"],
                ));
            }
            for offset in 1..=110 {
                items.push(scored_classification(
                    110 + offset,
                    "subscription",
                    Some(if offset <= messy_wrong {
                        "regular_spend"
                    } else {
                        "subscription"
                    }),
                    &["messy-merchant-strings"],
                ));
            }
            let mut report = report_of(vec![fixture(1.0, 1.0, 1.0, 0.0)]);
            report.metrics.insert(
                EvalMetric::Classification,
                MetricReport::Classification(classification_metrics(&items)),
            );
            report
        };

        assert_eq!(
            report_with(1).overall_verdict(&thresholds),
            Verdict::Pass,
            "1/110 has a Wilson upper bound below the approved 5% ceiling"
        );
        assert_eq!(
            report_with(2).overall_verdict(&thresholds),
            Verdict::Fail,
            "the messy stratum fails conjunctively even though clean is perfect"
        );
    }

    /// #306. `classification_strata_clear` read `EvalMetric::Classification`
    /// and nothing else, so an Extraction pack's declared ceilings were
    /// computed, reported, and then ignored by the verdict — it returned
    /// `false` whatever the gates measured.
    ///
    /// It failed closed, which is why this never surfaced as a wrong
    /// PASS, and why it survived: the letter pack failed the per-fixture
    /// rule as well, so one bug masked the other until #301 removed the
    /// first.
    #[test]
    fn declared_strata_are_read_from_the_metric_the_pack_declares() {
        let thresholds = obligation_ceiling(0.05);

        // 110 obligations, every one found correctly. The gate's Wilson
        // upper bound is far below its ceiling, so the pack passes.
        let items: Vec<ScoredItem> = (1..=110)
            .map(|ordinal| scored_extraction(ordinal, true, &["any-letter"]))
            .collect();

        let mut report = report_of(vec![fixture_scoring("obligations", 1.0)]);
        report.metrics.insert(
            EvalMetric::Extraction,
            MetricReport::Extraction(runner::eval::extraction_metrics(&items)),
        );

        assert_eq!(
            report.overall_verdict(&thresholds),
            Verdict::Pass,
            "an extraction pack whose declared gates all clear must pass"
        );
    }

    /// The other half, and the reason the first test has to exist: this
    /// one **passes today, for the wrong reason** — the verdict is Fail
    /// because the metric is unreadable, not because the ceiling was
    /// breached. A suite asserting only the failing direction cannot
    /// tell a working gate from an unreachable one.
    #[test]
    fn a_breached_ceiling_fails_an_extraction_pack_too() {
        let thresholds = obligation_ceiling(0.05);

        // 20 of 110 obligations missed outright — a confident-wrong rate
        // whose interval sits well above the 5% ceiling.
        let items: Vec<ScoredItem> = (1..=110)
            .map(|ordinal| scored_extraction(ordinal, ordinal > 20, &["any-letter"]))
            .collect();

        let mut report = report_of(vec![fixture_scoring("obligations", 1.0)]);
        report.metrics.insert(
            EvalMetric::Extraction,
            MetricReport::Extraction(runner::eval::extraction_metrics(&items)),
        );

        assert_eq!(
            report.overall_verdict(&thresholds),
            Verdict::Fail,
            "a breached ceiling must fail, and for that reason"
        );
    }

    #[test]
    fn classification_gate_records_floor_provenance_and_observed_interval() {
        let mut items = Vec::new();
        for ordinal in 1..=110 {
            items.push(scored_classification(
                ordinal,
                "subscription",
                Some(if ordinal == 1 {
                    "regular_spend"
                } else {
                    "subscription"
                }),
                &["clean"],
            ));
        }
        let declarations = BTreeMap::from([(
            "clean".to_owned(),
            ClassificationStratum {
                description: "Clear merchant strings.".to_owned(),
                classes: BTreeMap::from([(
                    HarmClass::Subscription,
                    ConfidentWrongCeiling {
                        max_wilson_95: 0.05,
                        reason: "Initial appliance-risk ceiling.".to_owned(),
                        date: NaiveDate::from_ymd_opt(2026, 7, 29).unwrap(),
                    },
                )]),
            },
        )]);

        let metrics = classification_metrics(&items).with_gates(&declarations);
        let gate = &metrics.gates["clean"][&HarmClass::Subscription];

        assert_eq!(gate.observed.n, 110);
        assert_eq!(gate.observed.successes, 1);
        assert!(gate.observed.wilson_95.unwrap().high < 0.05);
        assert_eq!(gate.max_wilson_95, 0.05);
        assert!(gate.outcome.clears());
        assert_eq!(gate.reason, "Initial appliance-risk ceiling.");
        assert_eq!(gate.date.to_string(), "2026-07-29");
    }

    #[test]
    fn pass_when_every_step_and_the_end_result_clear_their_bars() {
        let result = fixture(0.88, 0.91, 0.96, 0.12);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Pass);
    }

    #[test]
    fn review_rate_is_tracked_but_never_gates_a_verdict() {
        // Review is how the private appliance remains honest. Its cost
        // is reported, but a model must not become less recommendable
        // merely because it correctly asked a person to decide.
        let result = fixture(0.88, 0.91, 0.96, 0.20);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Pass);
    }

    #[test]
    fn fail_beats_marginal_when_a_step_is_below_threshold() {
        // A heavy review bucket does not rescue a model that is simply
        // wrong: below threshold is Fail, whatever the review rate.
        let result = fixture(0.61, 0.70, 0.74, 0.38);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Fail);
    }

    #[test]
    fn fail_when_only_the_end_to_end_score_falls_short() {
        // Nothing went to a human, so nothing absorbed the misses: the
        // report is simply short of the promise, and confidently so.
        let result = fixture(0.99, 0.99, 0.94, 0.02);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Fail);
    }

    #[test]
    fn review_does_not_rescue_a_quality_score_below_its_bar() {
        // Review is neither a failure nor a substitute for measured
        // quality. The end result still has to clear its own bar.
        let result = fixture(0.88, 0.91, 0.88, 0.22);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Fail);
    }

    #[test]
    fn a_large_review_bucket_does_not_hide_a_bad_end_result() {
        let result = fixture(0.88, 0.91, 0.62, 0.40);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Fail);
    }

    #[test]
    fn a_step_score_exactly_at_its_threshold_clears_it() {
        let result = fixture(0.85, 0.90, 0.99, 0.0);

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Pass);
    }

    #[test]
    fn a_step_score_a_hair_below_its_threshold_fails() {
        assert_eq!(
            fixture(0.84, 0.90, 0.99, 0.0).verdict(&pack_thresholds()),
            Verdict::Fail
        );
        assert_eq!(
            fixture(0.85, 0.89, 0.99, 0.0).verdict(&pack_thresholds()),
            Verdict::Fail
        );
    }

    #[test]
    fn end_to_end_exactly_at_the_bar_clears_it() {
        assert_eq!(END_TO_END_BAR, 0.95);
        assert_eq!(
            fixture(0.99, 0.99, END_TO_END_BAR, 0.0).verdict(&pack_thresholds()),
            Verdict::Pass
        );
    }

    #[test]
    fn a_step_nobody_set_a_bar_for_cannot_clear_it() {
        // A scored step missing from the pack's `eval` block is a pack
        // bug. Judging it silently against nothing would report an
        // unmeasured model as good.
        let mut result = fixture(0.88, 0.91, 0.96, 0.02);
        result
            .step_scores
            .insert("summarise".to_string(), step(0.99));

        assert_eq!(result.verdict(&pack_thresholds()), Verdict::Fail);
    }

    #[test]
    fn a_report_takes_the_worst_verdict_of_its_fixtures() {
        // One bad fixture is a bad model: an eval that averaged its way
        // past a failure would recommend a tier that cannot do the job.
        assert_eq!(
            Verdict::worst([Verdict::Pass, Verdict::Marginal, Verdict::Pass]),
            Verdict::Marginal
        );
        assert_eq!(
            Verdict::worst([Verdict::Marginal, Verdict::Fail, Verdict::Pass]),
            Verdict::Fail
        );
        assert_eq!(
            Verdict::worst([Verdict::Pass, Verdict::Pass]),
            Verdict::Pass
        );
    }

    #[test]
    fn no_fixtures_is_not_a_pass() {
        // Nothing ran, so nothing was shown to work.
        assert_eq!(Verdict::worst([]), Verdict::Fail);
    }

    #[test]
    fn a_report_verdict_is_computed_from_its_fixtures() {
        let report = report_of(vec![
            fixture(0.88, 0.91, 0.96, 0.02),
            fixture(0.88, 0.91, 0.96, 0.30),
        ]);

        assert_eq!(report.overall_verdict(&pack_thresholds()), Verdict::Pass);
    }

    #[test]
    fn verdicts_label_the_human_table_in_capitals() {
        assert_eq!(Verdict::Pass.label(), "PASS");
        assert_eq!(Verdict::Marginal.label(), "MARGINAL");
        assert_eq!(Verdict::Fail.label(), "FAIL");
    }
}

// --- extraction helpers (#306) -------------------------------------------

/// One scored obligation: either found correctly, or missed outright.
fn scored_extraction(ordinal: usize, found: bool, strata: &[&str]) -> ScoredItem {
    let obligation = runner::eval::ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Example Council".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "the date of this letter".to_owned(),
        amount: "no amount".to_owned(),
        due: None,
    };
    ScoredItem {
        id: format!("app.kttl.test/letters-01/obligation-{ordinal:02}"),
        item_id: format!("obligation-{ordinal:02}"),
        pack: "app.kttl.test".to_owned(),
        pack_version: "1.0.0".to_owned(),
        prompt_version: "blake3:test".to_owned(),
        fixture: "letter.txt".to_owned(),
        fixture_id: "letters-01".to_owned(),
        strata: strata.iter().map(|stratum| (*stratum).to_owned()).collect(),
        raw_input: format!("PASSAGE {ordinal}"),
        decision_key: format!("PASSAGE {ordinal}"),
        decision: ScoredDecision::Extraction {
            expected_review: false,
            expected: Some(runner::eval::Extracted::Obligation(obligation.clone())),
            unauthored_negative: false,
            actual: if found {
                runner::eval::ExtractionOutcome::Found {
                    extracted: runner::eval::Extracted::Obligation(obligation),
                }
            } else {
                runner::eval::ExtractionOutcome::Absent
            },
        },
        evidence: Default::default(),
        trace_ids: Vec::new(),
        confidence: None,
        exchanges: Vec::new(),
    }
}

/// A pack declaring one `any-letter` stratum with an obligation ceiling.
fn obligation_ceiling(max_wilson_95: f64) -> Thresholds {
    Thresholds::from_eval(&BTreeMap::from([("obligations".to_owned(), 0.95)]))
        .with_gate(runner::eval::Gate::Pooled)
        .with_classification_strata(BTreeMap::from([(
            "any-letter".to_owned(),
            ClassificationStratum {
                description: "Every letter in the bed.".to_owned(),
                classes: BTreeMap::from([(
                    HarmClass::Obligation,
                    ConfidentWrongCeiling {
                        max_wilson_95,
                        reason: "Test ceiling.".to_owned(),
                        date: "2026-08-01".parse().expect("date"),
                    },
                )]),
            },
        )]))
}

/// A fixture carrying one named step at a given score, plus a clean
/// end-to-end so only the gate under test can fail the verdict.
fn fixture_scoring(step: &str, score: f32) -> FixtureResult {
    FixtureResult {
        fixture: "letter-01.txt".to_owned(),
        step_scores: BTreeMap::from([(
            step.to_owned(),
            StepScore {
                score,
                expected: 445,
                correct: (score * 445.0).round() as usize,
            },
        )]),
        items: Vec::new(),
        containment: Default::default(),
        end_to_end: 1.0,
        needs_review_rate: 0.0,
        retries: 0,
        perf: Some(perf()),
        stability: None,
    }
}

// --- the verdict shape a pack declares (#301) ----------------------------

/// One fixture carrying exactly one decision, right or wrong. This is
/// what an Extraction bed is mostly made of — 245 of the letter pack's
/// 355 fixtures — and the shape the per-fixture rule cannot read.
fn single_decision(correct: bool) -> FixtureResult {
    let score = if correct { 1.0 } else { 0.0 };
    FixtureResult {
        fixture: "letter-01.txt".to_string(),
        step_scores: BTreeMap::from([(
            "obligations".to_string(),
            StepScore {
                score,
                expected: 1,
                correct: usize::from(correct),
            },
        )]),
        items: Vec::new(),
        containment: Default::default(),
        end_to_end: score,
        needs_review_rate: 0.0,
        retries: 0,
        perf: Some(perf()),
        stability: None,
    }
}

/// A single-decision letter fixture tagged with the strata it belongs
/// to, so a gate can tell a gated letter from an ungated one.
fn single_decision_in(ordinal: usize, correct: bool, strata: &[&str]) -> FixtureResult {
    let mut fixture = single_decision(correct);
    // The item is right either way: the miss lives in the fixture's
    // step score and end-to-end, so the harm ceilings stay clear and
    // the verdict turns on pooling alone. Distinct ordinals, because a
    // ceiling counts distinct decisions (#310).
    fixture.items = vec![scored_extraction(ordinal, true, strata)];
    fixture
}

/// A report whose harm metrics are computed from its fixtures' items,
/// as a real run's are.
fn report_with_items(fixtures: Vec<FixtureResult>) -> EvalReport {
    let items: Vec<ScoredItem> = fixtures.iter().flat_map(|f| f.items.clone()).collect();
    let mut report = report_of(fixtures);
    report.metrics.insert(
        EvalMetric::Extraction,
        MetricReport::Extraction(runner::eval::extraction_metrics(&items)),
    );
    report
}

/// `any-letter` gated, `conditional-advisory` declared and ungated —
/// the letter bed's shape on 30 August 2026.
fn letter_gate() -> Thresholds {
    let mut strata = obligation_ceiling(0.05).classification_strata;
    strata.insert(
        "conditional-advisory".to_owned(),
        ClassificationStratum {
            description:
                "Measured, not yet gating; promotes once real letters of this shape read correctly."
                    .to_owned(),
            classes: BTreeMap::new(),
        },
    );
    obligation_ceiling(0.05).with_classification_strata(strata)
}

#[test]
fn an_ungated_stratum_does_not_decide_the_pooled_verdict() {
    // #581, decided 30 August 2026. Sixty hard letters were added to
    // the bed in a stratum declared ungated, and the pack failed on
    // main's own prompt — 0.977 on the 425 fixtures before them, 0.926
    // on 455 — because the pooled end-to-end read every fixture
    // regardless of what its stratum declared. A bar that falls every
    // time a harm is measured inverts the incentive: measuring well
    // fails the pack. So the pool is the gated strata, and an ungated
    // stratum is reported beside its promotion condition until it is
    // promoted on purpose.
    let gate = letter_gate();

    let mut bed: Vec<FixtureResult> = (1..=445)
        .map(|n| single_decision_in(n, true, &["any-letter", "absolute-deadline"]))
        .collect();
    bed.extend((446..=505).map(|n| single_decision_in(n, false, &["conditional-advisory"])));
    assert_eq!(
        report_with_items(bed).overall_verdict(&gate),
        Verdict::Pass,
        "sixty ungated misses do not move a verdict the gated strata clear"
    );

    // The control: the same sixty letters, gated, fail it — the bar is
    // unchanged; only what it is read over.
    let mut gated: Vec<FixtureResult> = (1..=445)
        .map(|n| single_decision_in(n, true, &["any-letter"]))
        .collect();
    gated.extend(
        (446..=505).map(|n| single_decision_in(n, false, &["any-letter", "conditional-advisory"])),
    );
    assert_eq!(
        report_with_items(gated).overall_verdict(&gate),
        Verdict::Fail
    );

    // And a bed with nothing in a gated stratum has shown nothing.
    let unpooled: Vec<FixtureResult> = (1..=100)
        .map(|n| single_decision_in(n, true, &["conditional-advisory"]))
        .collect();
    assert_eq!(
        report_with_items(unpooled).overall_verdict(&gate),
        Verdict::Fail
    );
}

fn obligations_bar(bar: f32, gate: runner::eval::Gate) -> Thresholds {
    Thresholds::from_eval(&BTreeMap::from([("obligations".to_string(), bar)])).with_gate(gate)
}

#[test]
fn the_per_fixture_gate_reads_a_bed_of_single_decisions_as_all_or_nothing() {
    // Not a strict gate — a gate with no gradient. A fixture holding one
    // decision can only score 0.0 or 1.0, so any bar above zero demands
    // perfection, and the verdict is identical at 444/445 and at 0/445.
    // Days of the letter pack's runs could not distinguish improvement
    // from disaster for exactly this reason.
    let mut nearly_perfect: Vec<FixtureResult> = (0..444).map(|_| single_decision(true)).collect();
    nearly_perfect.push(single_decision(false));
    let hopeless: Vec<FixtureResult> = (0..445).map(|_| single_decision(false)).collect();

    let gate = obligations_bar(0.95, runner::eval::Gate::PerFixture);
    assert_eq!(
        report_of(nearly_perfect).overall_verdict(&gate),
        Verdict::Fail
    );
    assert_eq!(report_of(hopeless).overall_verdict(&gate), Verdict::Fail);
}

#[test]
fn the_pooled_gate_reads_the_same_bed_as_a_rate() {
    // The same evidence, gated on the rate across every decision rather
    // than on the worst fixture. 444/445 clears a 0.95 bar; 0/445 does
    // not. The gate now has a gradient, which is the whole point.
    let mut nearly_perfect: Vec<FixtureResult> = (0..444).map(|_| single_decision(true)).collect();
    nearly_perfect.push(single_decision(false));
    let hopeless: Vec<FixtureResult> = (0..445).map(|_| single_decision(false)).collect();

    let gate = obligations_bar(0.95, runner::eval::Gate::Pooled);
    assert_eq!(
        report_of(nearly_perfect).overall_verdict(&gate),
        Verdict::Pass
    );
    assert_eq!(report_of(hopeless).overall_verdict(&gate), Verdict::Fail);
}

#[test]
fn the_pooled_gate_is_read_by_its_wilson_lower_bound_not_its_point_estimate() {
    // A rate is an estimate, and a bar cleared by a point estimate on
    // thin evidence has not been shown to be cleared at all. The same
    // reasoning as `max_wilson_95` for harm, pointing the other way:
    // harm is read by its upper bound, quality by its lower one.
    //
    // The same rate — 0.98, comfortably above a 0.95 bar on the point
    // estimate — at two depths of evidence. Fifty decisions cannot
    // demonstrate it (lower bound near 0.89); two thousand can.
    let gate = obligations_bar(0.95, runner::eval::Gate::Pooled);

    let mut thin: Vec<FixtureResult> = (0..49).map(|_| single_decision(true)).collect();
    thin.push(single_decision(false));
    assert_eq!(report_of(thin).overall_verdict(&gate), Verdict::Fail);

    let mut deep: Vec<FixtureResult> = (0..1960).map(|_| single_decision(true)).collect();
    deep.extend((0..40).map(|_| single_decision(false)));
    assert_eq!(report_of(deep).overall_verdict(&gate), Verdict::Pass);

    // And a rate sitting exactly on the bar never clears it, however
    // deep the evidence: an interval around 0.95 always reaches below
    // it. A pack wanting 0.95 demonstrated must measure better than
    // 0.95, which is the honest reading of what a bar means.
    let mut exactly_at_the_bar: Vec<FixtureResult> =
        (0..1900).map(|_| single_decision(true)).collect();
    exactly_at_the_bar.extend((0..100).map(|_| single_decision(false)));
    assert_eq!(
        report_of(exactly_at_the_bar).overall_verdict(&gate),
        Verdict::Fail
    );
}

// --- serde -------------------------------------------------------------

fn report_of(fixtures: Vec<FixtureResult>) -> EvalReport {
    EvalReport {
        unrunnable: Vec::new(),
        reused_fixtures: 0,
        pack: "app.kttl.subscription-audit".to_string(),
        pack_version: "1.0.0".to_string(),
        eval_set: runner::eval::fixture::EvalSelection::Development,
        model: Some(ModelInfo {
            file: "qwen2.5-3b-instruct-q4_k_m.gguf".to_string(),
            params: "3B".to_string(),
            quant: "Q4_K_M".to_string(),
            context: 8192,
        }),
        machine: MachineInfo {
            cpu: "Apple M1".to_string(),
            ram_gb: 8,
            os: "macOS 15.5".to_string(),
        },
        evidence: None,
        relations: Vec::new(),
        sidecar: None,
        fixtures,
        bed: None,
        runtime: None,
        metrics: BTreeMap::new(),
        verdict: Verdict::Pass,
    }
}

#[test]
fn an_eval_report_round_trips_through_json() {
    let report = report_of(vec![fixture(0.88, 0.91, 0.96, 0.12)]);

    let json = serde_json::to_string(&report).expect("serialise report");
    let read_back: EvalReport = serde_json::from_str(&json).expect("deserialise report");

    assert_eq!(read_back, report);
}

/// #232: the recorded runtime policy is the executed one, by
/// construction. `effective` reads the same [`SidecarRuntime`] the
/// sidecar is spawned with and the same answer-bound constant every
/// request carries — there is no second copy of either fact to drift,
/// which is the lesson #251 taught about context.
#[test]
fn the_recorded_runtime_policy_is_the_policy_the_run_executed() {
    let runtime = runner::sidecar::SidecarRuntime {
        context: 4096,
        parallel: 2,
        reasoning: runner::sidecar::Reasoning::Off,
    };

    let policy = runner::eval::RuntimePolicy::effective(&runtime);

    assert_eq!(policy.context, runtime.context);
    assert_eq!(policy.parallel, runtime.parallel);
    assert_eq!(policy.reasoning, runtime.reasoning);
    assert_eq!(policy.max_answer_tokens, runner::exec::MAX_ANSWER_TOKENS);
}

/// The policy travels in the flags' own words — `"off"`, the same word
/// `--reasoning` carries — and a report from before the field existed
/// still reads, as every provenance field before it has had to.
#[test]
fn a_runtime_policy_travels_in_the_flags_own_words_and_older_reports_still_read() {
    let policy =
        runner::eval::RuntimePolicy::effective(&runner::sidecar::SidecarRuntime::default());
    let json = serde_json::to_value(&policy).expect("serialise policy");
    assert_eq!(json["reasoning"], "off", "the wire spelling is the flag's");
    assert!(
        policy.describe().contains("reasoning off"),
        "the CLI sentence names the choice: {}",
        policy.describe()
    );

    // A report serialised before #232 has no `runtime` key.
    let report = report_of(vec![fixture(0.88, 0.91, 0.96, 0.12)]);
    let json = serde_json::to_string(&report).expect("serialise report");
    assert!(
        !json.contains("\"runtime\""),
        "a policy nobody recorded must not be invented on the wire: {json}"
    );
    let read_back: EvalReport = serde_json::from_str(&json).expect("older reports still read");
    assert_eq!(read_back.runtime, None);
}

#[test]
fn verdicts_travel_as_lowercase_words() {
    // `tiers.json` ships with the app and is read by the model-manager
    // screen, so the wire spelling is a contract.
    assert_eq!(
        serde_json::to_string(&Verdict::Marginal).expect("serialise verdict"),
        "\"marginal\""
    );
    assert_eq!(
        serde_json::from_str::<Verdict>("\"fail\"").expect("deserialise verdict"),
        Verdict::Fail
    );
}

#[test]
fn an_eval_report_uses_the_field_names_from_the_brief() {
    let report = report_of(vec![fixture(0.88, 0.91, 0.96, 0.12)]);

    let json: serde_json::Value = serde_json::to_value(&report).expect("serialise report");
    assert_eq!(json["pack"], "app.kttl.subscription-audit");
    assert_eq!(json["pack_version"], "1.0.0");
    assert_eq!(json["model"]["quant"], "Q4_K_M");
    assert_eq!(json["machine"]["ram_gb"], 8);
    assert_eq!(json["verdict"], "pass");

    let fixture = &json["fixtures"][0];
    assert_eq!(fixture["fixture"], "statement-01.csv");
    // Scores are f32; widened to f64 for JSON they are not the literal.
    assert_eq!(
        fixture["step_scores"]["classify"]["score"],
        f64::from(0.91f32)
    );
    assert_eq!(fixture["step_scores"]["classify"]["n"], 100);
    assert!(
        fixture["step_scores"]["classify"]["wilson_95"]["low"].is_number(),
        "{fixture}"
    );
    assert!(
        fixture["step_scores"]["classify"].get("expected").is_none(),
        "n is the public statistical name: {fixture}"
    );
    assert_eq!(fixture["end_to_end"], f64::from(0.96f32));
    assert_eq!(fixture["needs_review_rate"], f64::from(0.12f32));
    assert_eq!(fixture["retries"], 0);
    assert!(
        fixture["perf"]["wall_ms"].is_number(),
        "a receipt carries telemetry: {fixture}"
    );
}

#[test]
fn tier_records_the_worst_fixture_not_the_mean() {
    let report = report_of(vec![
        fixture(0.95, 0.90, 0.96, 0.02),
        fixture(0.95, 1.00, 0.96, 0.02),
    ]);

    let tier = Tier::of(
        &report,
        1,
        "2026-07-28T09:30:00Z"
            .parse()
            .expect("a fixed measurement time"),
    );

    assert_eq!(tier.steps["classify"].score, 0.90);
    assert_eq!(tier.steps["classify"].n, Some(100));
}

#[test]
fn development_and_exam_are_distinct_tier_measurements() {
    let development = report_of(vec![fixture(1.0, 1.0, 1.0, 0.0)]);
    let mut exam = development.clone();
    exam.eval_set = runner::eval::fixture::EvalSelection::Exam;
    let measured_at = chrono::DateTime::parse_from_rfc3339("2026-07-29T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let development = Tier::of(&development, 1, measured_at);
    let exam = Tier::of(&exam, 1, measured_at);

    assert!(!development.same_measurement(&exam));
}

#[test]
fn different_pack_or_scoring_versions_are_distinct_tier_measurements() {
    let report = report_of(vec![fixture(1.0, 1.0, 1.0, 0.0)]);
    let measured_at = chrono::DateTime::parse_from_rfc3339("2026-07-29T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let current = Tier::of(&report, 1, measured_at);
    let mut older_pack = current.clone();
    older_pack.pack_version = "0.9.0".to_owned();
    let mut older_scoring = current.clone();
    older_scoring.scoring_version -= 1;

    assert!(!current.same_measurement(&older_pack));
    assert!(!current.same_measurement(&older_scoring));
}

/// #612: the sum is part of what an obligation *is*. A found obligation
/// that names the right party, the right day, the right way and the
/// wrong figure is a different assertion — confident-wrong, never a
/// near miss — because the person acts on the figure.
#[test]
fn an_obligation_with_a_different_sum_is_a_different_assertion() {
    use runner::eval::ExpectedObligation;
    let expected = |amount: &str| ExpectedObligation {
        kind: "payment".to_owned(),
        party: "Elmswood Lettings".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "the date of this letter".to_owned(),
        due: None,
        amount: amount.to_owned(),
    };
    let found = |amount: &str| runner::run::Obligation {
        kind: "payment".to_owned(),
        party: "Elmswood Lettings".to_owned(),
        ask: "Pay the arrears".to_owned(),
        deadline: "within 14 days".to_owned(),
        anchor: "the date of this letter".to_owned(),
        amount: amount.to_owned(),
        confidence: "high".to_owned(),
        due: None,
        evidence: Vec::new(),
        dated_by: None,
        priced_by: None,
        amount_from: None,
        deadline_from: None,
        shown: Default::default(),
        disputed: Vec::new(),
    };
    assert_eq!(expected("£84.00").identity(), found("£84.00").identity());
    assert_ne!(expected("£84.00").identity(), found("£48.00").identity());
    assert_ne!(expected("£84.00").identity(), found("no amount").identity());
    // A sum invented on an ask that names none is the same kind of wrong.
    assert_ne!(expected("no amount").identity(), found("£84.00").identity());
}
