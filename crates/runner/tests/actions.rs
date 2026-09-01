//! Proposed actions (#30).
//!
//! RED BY DESIGN: `actions::propose_actions` and `actions::card` are
//! `todo!()`. `fixtures/run-01/results.json` is the input and the app's
//! review screen is the consumer — the shapes here are already being
//! rendered.

use chrono::NaiveDate;
use runner::actions::propose_actions;
use runner::results::{ActionKind, RunReport, ACTIONS_NOTE, PROPOSED_ACTIONS_SCHEMA};
use std::path::Path;

/// The completed run the design mocks and the app tests share.
fn report() -> RunReport {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/run-01/results.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture readable"))
        .expect("the fixture matches results.rs — if this fails, the contract moved")
}

/// Passed in, never read from the clock: a run's outputs must be
/// reproducible and tests must not drift into next month.
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 20).expect("valid date")
}

#[test]
fn price_rise_produces_review_action() {
    let actions = propose_actions(&report(), today());

    assert_eq!(actions.schema, PROPOSED_ACTIONS_SCHEMA);
    assert_eq!(actions.note, ACTIONS_NOTE);
    assert_eq!(actions.run_id, "run-01");

    let rise = actions
        .actions
        .iter()
        .find(|a| a.kind == ActionKind::ReviewPriceRise)
        .expect("Netflix rose in May and that is the headline action");

    assert_eq!(rise.id, "act-01", "the most useful card comes first");
    assert_eq!(
        rise.evidence.get("merchant").map(String::as_str),
        Some("Netflix")
    );
    assert_eq!(
        rise.evidence.get("before").map(String::as_str),
        Some("10.99")
    );
    assert_eq!(
        rise.evidence.get("after").map(String::as_str),
        Some("12.99")
    );
    assert_eq!(
        rise.evidence.get("month").map(String::as_str),
        Some("2025-05")
    );

    assert!(
        rise.title.contains("Netflix") && rise.title.contains("£12.99"),
        "the card says what happened: {:?}",
        rise.title
    );
    assert!(
        rise.detail.contains("£24.00"),
        "and what it costs over a year: {:?}",
        rise.detail
    );
}

#[test]
fn every_action_is_only_ever_proposed() {
    for action in propose_actions(&report(), today()).actions {
        assert_eq!(action.status, "proposed");
        assert!(
            !action.export.text.is_empty(),
            "{} has no copyable text",
            action.id
        );
        assert!(
            !action
                .export
                .ics
                .as_ref()
                .expect("audit actions are always dated")
                .summary
                .is_empty(),
            "{} has no summary",
            action.id
        );
    }
}

#[test]
fn calendar_dates_come_from_the_run_never_from_now() {
    let actions = propose_actions(&report(), today());
    let by_kind = |kind: ActionKind| {
        actions
            .actions
            .iter()
            .find(|a| a.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} action"))
            .clone()
    };

    // A monthly-bill decision belongs at the start of a month.
    assert_eq!(
        by_kind(ActionKind::ReviewPriceRise)
            .export
            .ics
            .as_ref()
            .expect("dated")
            .date,
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
    );
    // A week to think about it.
    assert_eq!(
        by_kind(ActionKind::ReviewSubscription)
            .export
            .ics
            .as_ref()
            .expect("dated")
            .date,
        NaiveDate::from_ymd_opt(2026, 7, 27).expect("valid date")
    );
    // Seven days before Amazon Prime's next yearly charge.
    assert_eq!(
        by_kind(ActionKind::CheckRenewal)
            .export
            .ics
            .as_ref()
            .expect("dated")
            .date,
        NaiveDate::from_ymd_opt(2027, 3, 7).expect("valid date")
    );
    // Two days before British Gas's next quarterly payment.
    assert_eq!(
        by_kind(ActionKind::CalendarReminder)
            .export
            .ics
            .as_ref()
            .expect("dated")
            .date,
        NaiveDate::from_ymd_opt(2026, 10, 13).expect("valid date")
    );
}

#[test]
fn ids_are_stable_and_in_emission_order() {
    let actions = propose_actions(&report(), today());
    let ids: Vec<&str> = actions.actions.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["act-01", "act-02", "act-03", "act-04"]);

    let kinds: Vec<ActionKind> = actions.actions.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            ActionKind::ReviewPriceRise,
            ActionKind::ReviewSubscription,
            ActionKind::CheckRenewal,
            ActionKind::CalendarReminder,
        ]
    );
}

#[test]
fn nothing_a_card_says_promises_to_act() {
    for action in propose_actions(&report(), today()).actions {
        let copy = format!("{} {}", action.title, action.detail).to_lowercase();
        for promise in [
            "cancelled",
            "we've booked",
            "automatically",
            "on your behalf",
        ] {
            assert!(
                !copy.contains(promise),
                "{} promises to act: {copy:?}",
                action.id
            );
        }
    }
}
