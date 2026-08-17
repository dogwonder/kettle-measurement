//! Totals, annualisation and ordering (#29).
//!
//! RED BY DESIGN: `aggregate::annualise`, `build_report` and
//! `check_yourself` are `todo!()`. Making these pass is the task; the
//! assertions and the shapes they name are the contract.

use chrono::NaiveDate;
use runner::aggregate::{annualise, build_report, check_yourself, NOT_COUNTED_NOTE};
use runner::kinds::KindFrom;
use runner::recurrence::{Period, PriceRise};
use runner::results::{
    Confidence, DateRange, InputInfo, ModelInfo, PackInfo, RunInfo, RUN_REPORT_SCHEMA,
};
use runner::run::{
    AuditOutcome, Evidence, Finding, InputSeen, Payload, ReviewItem, RunOutcome, Spend,
};
use rust_decimal::Decimal;
use std::path::Path;
use std::str::FromStr;

fn decimal(text: &str) -> Decimal {
    Decimal::from_str(text).expect("test decimal parses")
}

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("test date parses")
}

/// The pack's own expectations for statement-01 — the same file the
/// eval harness scores against, so the two can never disagree about
/// what this statement costs.
fn expected() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit/fixtures/statement-01.expected.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture readable"))
        .expect("fixture is JSON")
}

/// Monthly payments on the 5th, rising in May — statement-01's Netflix.
fn netflix() -> Finding {
    let mut evidence: Vec<Evidence> = (1..=12)
        .map(|month| Evidence {
            date: day(&format!("2025-{month:02}-05")),
            amount: if month < 5 {
                decimal("10.99")
            } else {
                decimal("12.99")
            },
        })
        .collect();
    evidence.sort_by_key(|e| e.date);

    Finding {
        merchant: "Netflix".to_owned(),
        raw_merchant: "NETFLIX.COM".to_owned(),
        kind: "subscription".to_owned(),
        kind_from: KindFrom::CategoryMap,
        category: "streaming".to_owned(),
        confidence: "high".to_owned(),
        period: Period::Monthly,
        current_amount: decimal("12.99"),
        price_rise: Some(PriceRise {
            from: decimal("10.99"),
            to: decimal("12.99"),
            when: day("2025-05-05"),
        }),
        evidence,
    }
}

fn monthly(merchant: &str, raw: &str, amount: &str, on: u32, category: &str) -> Finding {
    Finding {
        merchant: merchant.to_owned(),
        raw_merchant: raw.to_owned(),
        kind: "subscription".to_owned(),
        kind_from: KindFrom::CategoryMap,
        category: category.to_owned(),
        confidence: "high".to_owned(),
        period: Period::Monthly,
        current_amount: decimal(amount),
        price_rise: None,
        evidence: (1..=12)
            .map(|month| Evidence {
                date: day(&format!("2025-{month:02}-{on:02}")),
                amount: decimal(amount),
            })
            .collect(),
    }
}

/// Quarterly, and only medium confidence — the "check this yourself"
/// case that isn't an error.
fn british_gas() -> Finding {
    Finding {
        merchant: "British Gas".to_owned(),
        raw_merchant: "BRITISH GAS".to_owned(),
        kind: "utility".to_owned(),
        kind_from: KindFrom::CategoryMap,
        category: "energy".to_owned(),
        confidence: "medium".to_owned(),
        period: Period::Quarterly,
        current_amount: decimal("88.00"),
        price_rise: None,
        evidence: ["2025-01-15", "2025-04-15", "2025-07-15", "2025-10-15"]
            .iter()
            .map(|date| Evidence {
                date: day(date),
                amount: decimal("88.00"),
            })
            .collect(),
    }
}

fn outcome() -> RunOutcome {
    RunOutcome {
        input: InputSeen {
            rows: 0,
            period: None,
        },
        // This fixture builds an outcome directly to test aggregation,
        // never having run an input through it.
        inputs: Vec::new(),
        claim_traces: Vec::new(),
        payload: Payload::Audit(AuditOutcome {
            income: Vec::new(),
            findings: vec![
                netflix(),
                monthly("Spotify", "SPOTIFY LTD", "11.99", 12, "streaming"),
                monthly("PureGym", "PUREGYM LTD", "24.99", 3, "fitness"),
                british_gas(),
            ],
            other: vec![Spend {
                merchant: "Tesco".to_owned(),
                raw_merchant: "TESCO STORES 3412".to_owned(),
                kind: "regular_spend".to_owned(),
                kind_from: KindFrom::Cadence,
                category: "food_drink".to_owned(),
                confidence: "high".to_owned(),
                transactions: vec![
                    Evidence {
                        date: day("2025-01-08"),
                        amount: decimal("41.20"),
                    },
                    Evidence {
                        date: day("2025-02-08"),
                        amount: decimal("43.75"),
                    },
                    Evidence {
                        date: day("2025-03-08"),
                        amount: decimal("52.05"),
                    },
                ],
            }],
        }),
        needs_review: vec![ReviewItem {
            subject: "AMZN DIGITAL*8H2Q".to_owned(),
            reason: "Kettle didn't get an answer for this one, so it needs your eyes.".to_owned(),
            transactions: vec![Evidence {
                date: day("2025-03-22"),
                amount: decimal("8.99"),
            }],
        }],
        warnings: Vec::new(),
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        id: "run-test".to_owned(),
        pack: PackInfo {
            id: "app.kttl.subscription-audit".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Subscription & recurring-spend audit".to_owned(),
        },
        input: InputInfo {
            file: "statement-01.csv".to_owned(),
            rows: 58,
            period: DateRange {
                from: day("2025-01-03"),
                to: day("2025-12-28"),
            },
            hash: "blake3:0000".to_owned(),
        },
        model: ModelInfo {
            tier: "Steady".to_owned(),
            id: "qwen2.5-7b-instruct-q4_k_m".to_owned(),
        },
        started: "2026-07-19T14:02:11Z".to_owned(),
        finished: "2026-07-19T14:06:38Z".to_owned(),
        currency: "GBP".to_owned(),
    }
}

#[test]
fn annualised_matches_expected_json() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");
    let expected = expected();
    let expected = expected["recurring"].as_array().expect("recurring block");

    for want in expected {
        let merchant = want["merchant"].as_str().expect("merchant");
        let finding = report
            .recurring
            .iter()
            .find(|f| f.merchant == merchant)
            .unwrap_or_else(|| panic!("{merchant} is missing from the report"));

        // Compared as numbers, not as text. `serde_json` holds the
        // fixture's 352.00 as a float and prints it back as "352.0",
        // so a string comparison would fail on a correct answer —
        // `Decimal`'s own equality ignores trailing-zero scale.
        assert_eq!(
            finding.annualised,
            decimal(&want["annualised"].to_string()),
            "{merchant} annualised"
        );
        assert_eq!(
            finding.period.as_wire(),
            want["period"].as_str().expect("period"),
            "{merchant} period"
        );
        assert_eq!(
            finding.price_rise.is_some(),
            want["price_rise"].as_bool().expect("price_rise"),
            "{merchant} price rise"
        );
    }
}

#[test]
fn annualise_multiplies_by_the_cadence_exactly() {
    assert_eq!(
        annualise(decimal("12.99"), Period::Monthly),
        decimal("155.88")
    );
    assert_eq!(
        annualise(decimal("88.00"), Period::Quarterly),
        decimal("352.00")
    );
    assert_eq!(
        annualise(decimal("2.50"), Period::Weekly),
        decimal("130.00")
    );
    assert_eq!(
        annualise(decimal("95.00"), Period::Annual),
        decimal("95.00")
    );
}

#[test]
fn recurring_is_sorted_by_what_it_costs_a_year() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");
    let order: Vec<&str> = report
        .recurring
        .iter()
        .map(|f| f.merchant.as_str())
        .collect();
    assert_eq!(order, ["British Gas", "PureGym", "Netflix", "Spotify"]);
}

#[test]
fn summary_totals_the_recurring_spend() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");

    assert_eq!(report.schema, RUN_REPORT_SCHEMA);
    assert_eq!(report.summary.recurring_count, 4);
    // 352.00 + 299.88 + 155.88 + 143.88
    assert_eq!(report.summary.annualised_total, decimal("951.64"));
    assert_eq!(report.summary.monthly_equivalent, decimal("79.30"));
    assert_eq!(report.summary.price_rises, 1);
    assert_eq!(report.summary.needs_review_count, 1);
    assert!(
        report.summary.note.contains("needs your eyes"),
        "the summary says what the totals leave out: {:?}",
        report.summary.note
    );
}

#[test]
fn price_rise_carries_what_it_costs_over_a_year() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");
    let netflix = report
        .recurring
        .iter()
        .find(|f| f.merchant == "Netflix")
        .expect("Netflix");
    let rise = netflix.price_rise.as_ref().expect("Netflix rose");

    assert_eq!(rise.from, decimal("10.99"));
    assert_eq!(rise.to, decimal("12.99"));
    assert_eq!(rise.month, "2025-05");
    assert_eq!(rise.extra_per_year, decimal("24.00"));
}

#[test]
fn evidence_says_why_in_a_sentence_a_person_can_check() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");
    let netflix = report
        .recurring
        .iter()
        .find(|f| f.merchant == "Netflix")
        .expect("Netflix");

    assert_eq!(
        netflix.evidence.reason,
        "12 payments, one every month on or near the 5th"
    );
    let intervals = netflix
        .evidence
        .interval_days
        .expect("twelve payments have gaps");
    // Measured, not the nominal 30: the real gaps between the 5th of
    // each month run 28–31 days. `interval_days` is evidence and sits
    // beside a measured spread, so it says what was actually seen.
    assert_eq!(intervals.median, 31);
    assert_eq!(intervals.spread, 3);
    assert_eq!(netflix.evidence.transactions.len(), 12);

    let gas = report
        .recurring
        .iter()
        .find(|f| f.merchant == "British Gas")
        .expect("British Gas");
    assert_eq!(gas.evidence.reason, "4 payments, roughly 3 months apart");
}

#[test]
fn regular_spending_is_counted_separately_from_subscriptions() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");

    assert_eq!(report.regular_spend.len(), 1);
    let tesco = &report.regular_spend[0];
    assert_eq!(tesco.merchant, "Tesco");
    assert_eq!(tesco.visits, 3);
    assert_eq!(tesco.total, decimal("137.00"));
    assert_eq!(tesco.typical_visit, decimal("43.75"));
    assert_eq!(tesco.confidence, Confidence::High);

    // Regular spending is not a subscription and must never inflate the
    // headline number.
    assert_eq!(report.summary.annualised_total, decimal("951.64"));
}

#[test]
fn review_items_travel_with_their_payments_and_a_reassurance() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");

    assert_eq!(report.needs_review.len(), 1);
    let item = &report.needs_review[0];
    assert_eq!(item.raw_merchant, "AMZN DIGITAL*8H2Q");
    assert_eq!(item.transactions.len(), 1);
    assert_eq!(item.note, NOT_COUNTED_NOTE);
}

#[test]
fn check_yourself_lists_the_uncertain_and_the_expensive_without_alarm() {
    let report = build_report(&outcome(), run_info()).expect("an audit run reports");
    let entries = check_yourself(&report.recurring);

    let about: Vec<&str> = entries.iter().map(|e| e.about.as_str()).collect();
    assert_eq!(
        about,
        ["British Gas", "PureGym"],
        "medium confidence, then the expensive habit"
    );

    for entry in &entries {
        for alarm in ["error", "failed", "warning", "invalid"] {
            assert!(
                !entry.why.to_lowercase().contains(alarm),
                "this is a checklist, not an error state: {:?}",
                entry.why
            );
        }
    }

    assert!(
        entries[1].why.contains("£299.88"),
        "say what it costs: {:?}",
        entries[1].why
    );
}

/// A salary: twelve credits, detected as income by `detect_income`.
fn salary() -> Finding {
    Finding {
        merchant: "Acme Payroll".to_owned(),
        raw_merchant: "ACME PAYROLL".to_owned(),
        kind: "income".to_owned(),
        kind_from: KindFrom::Income,
        category: "other".to_owned(),
        confidence: "high".to_owned(),
        period: Period::Monthly,
        current_amount: decimal("2450.00"),
        price_rise: None,
        evidence: (1..=12)
            .map(|month| Evidence {
                date: day(&format!("2025-{month:02}-28")),
                amount: decimal("2450.00"),
            })
            .collect(),
    }
}

#[test]
fn income_is_reported_as_income_and_never_as_spending() {
    let mut outcome = outcome();
    let Payload::Audit(audit) = &mut outcome.payload else {
        panic!("an audit pack produces the Audit payload");
    };
    audit.income = vec![salary()];
    let report = build_report(&outcome, run_info()).expect("an audit run reports");

    assert_eq!(report.income.len(), 1, "the salary is income");
    let wages = &report.income[0];
    assert_eq!(wages.merchant, "Acme Payroll");
    assert_eq!(wages.amount, decimal("2450.00"));
    assert_eq!(wages.period, Period::Monthly);

    // The bug this fixes: money coming in was reported as money going
    // out. A person was told they had spent £29,400 at their employer.
    assert!(
        !report
            .regular_spend
            .iter()
            .any(|s| s.merchant == "Acme Payroll"),
        "income must never appear in regular spending"
    );

    // And it is not spending, so it changes no total.
    assert_eq!(report.summary.annualised_total, decimal("951.64"));
}
