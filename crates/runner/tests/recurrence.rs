//! Recurrence detection (#27) and price-rise detection (#28). All
//! deterministic Rust: `recurring` below 100% on fixtures is a Rust
//! bug, not a model problem (CLAUDE.md).

use chrono::NaiveDate;
use runner::parse::{Direction, Transaction};
use runner::recurrence::{detect_income, detect_recurring, Period};
use rust_decimal::Decimal;
use std::str::FromStr;

fn debit(date: &str, amount: &str) -> Transaction {
    Transaction {
        date: NaiveDate::from_str(date).expect("test date"),
        raw_merchant: String::new(),
        amount: Decimal::from_str(amount).expect("test amount"),
        direction: Direction::Debit,
    }
}

/// n monthly debits of `amount` starting January 2025, billed on the 8th.
fn monthly(n: u32, amount: &str) -> Vec<Transaction> {
    (0..n)
        .map(|i| debit(&format!("{}-{:02}-08", 2025 + (i / 12), 1 + i % 12), amount))
        .collect()
}

#[test]
fn netflix_monthly_detected() {
    let series = detect_recurring("Netflix", &monthly(12, "10.99"));

    assert_eq!(series.len(), 1, "one subscription, one series");
    let netflix = &series[0];
    assert_eq!(netflix.merchant, "Netflix");
    assert_eq!(netflix.period, Period::Monthly);
    assert_eq!(netflix.current_amount, Decimal::from_str("10.99").unwrap());
    assert!(netflix.price_rise.is_none());
}

#[test]
fn two_txns_not_recurring() {
    // Two monthly-spaced payments prove nothing — below the floor.
    let two = vec![debit("2025-01-08", "10.99"), debit("2025-02-08", "10.99")];
    assert!(detect_recurring("Netflix", &two).is_empty());
}

#[test]
fn mid_year_rise_flagged() {
    // Eight months at £7.99, then four at £10.99: one subscription
    // whose price rose, not two subscriptions.
    let mut series = monthly(8, "7.99");
    series.extend((0..4).map(|i| debit(&format!("2025-{:02}-08", 9 + i), "10.99")));

    let detected = detect_recurring("Disney+", &series);

    assert_eq!(
        detected.len(),
        1,
        "a price change is one series, not two: {detected:?}"
    );
    let disney = &detected[0];
    assert_eq!(disney.period, Period::Monthly);
    assert_eq!(
        disney.current_amount,
        Decimal::from_str("10.99").unwrap(),
        "current price is the new one"
    );
    let rise = disney.price_rise.as_ref().expect("the rise is flagged");
    assert_eq!(rise.from, Decimal::from_str("7.99").unwrap());
    assert_eq!(rise.to, Decimal::from_str("10.99").unwrap());
    assert_eq!(rise.when, NaiveDate::from_str("2025-09-08").unwrap());
}

#[test]
fn flat_series_not_flagged() {
    let detected = detect_recurring("Netflix", &monthly(12, "10.99"));
    assert!(detected[0].price_rise.is_none(), "no change, no flag");
}

// ── the §4a traps ───────────────────────────────────────────────────

#[test]
fn annual_two_occurrences_detected() {
    // An annual sub shows up once or twice in a normal statement
    // window. Two, a year apart, is the highest-value finding in the
    // report — it must not fall to a three-payment floor.
    let two = vec![debit("2024-03-14", "95.00"), debit("2025-03-14", "95.00")];

    let detected = detect_recurring("Amazon Prime", &two);
    assert_eq!(detected.len(), 1, "{detected:?}");
    assert_eq!(detected[0].period, Period::Annual);
}

#[test]
fn concurrent_subscriptions_split_not_merged() {
    // One merchant, two subscriptions running at once (music monthly,
    // storage annual). Interleaved series must come out separately —
    // never as one series with imagined price changes.
    let mut txns = monthly(12, "9.99");
    txns.push(debit("2025-03-20", "79.00"));
    txns.push(debit("2026-03-20", "79.00"));

    let mut detected = detect_recurring("Amazon", &txns);
    detected.sort_by_key(|r| r.current_amount);

    assert_eq!(detected.len(), 2, "{detected:?}");
    assert_eq!(detected[0].period, Period::Monthly);
    assert!(detected[0].price_rise.is_none());
    assert_eq!(detected[1].period, Period::Annual);
}

#[test]
fn cancelled_and_restarted_still_monthly() {
    // Eight months, a five-month gap, four more months at the same
    // price: the median shrugs off the gap.
    let mut txns = monthly(8, "12.99");
    txns.extend((0..4).map(|i| debit(&format!("2026-{:02}-08", 2 + i), "12.99")));

    let detected = detect_recurring("PureGym", &txns);
    assert_eq!(detected.len(), 1, "{detected:?}");
    assert_eq!(detected[0].period, Period::Monthly);
}

#[test]
fn price_drop_merges_without_a_rise_flag() {
    // A decrease is still one series — current price updates, but
    // "price rise" means rise.
    let mut txns = monthly(8, "12.99");
    txns.extend((0..4).map(|i| debit(&format!("2025-{:02}-08", 9 + i), "6.99")));

    let detected = detect_recurring("Sky", &txns);
    assert_eq!(detected.len(), 1, "one series: {detected:?}");
    assert_eq!(
        detected[0].current_amount,
        Decimal::from_str("6.99").unwrap()
    );
    assert!(detected[0].price_rise.is_none());
}

#[test]
fn irregular_spend_not_recurring() {
    // A coffee habit: similar-but-varying amounts at irregular
    // intervals. Habitual is not recurring.
    let txns = vec![
        debit("2025-01-06", "3.40"),
        debit("2025-01-21", "4.10"),
        debit("2025-02-03", "3.20"),
        debit("2025-02-19", "4.80"),
        debit("2025-03-10", "3.40"),
        debit("2025-03-27", "3.90"),
    ];
    assert!(detect_recurring("Kaffa Coffee", &txns).is_empty());
}

#[test]
fn credits_never_join_a_series() {
    // Eleven debits and a refund: the refund is not a twelfth payment.
    let mut txns = monthly(11, "10.99");
    txns.push(Transaction {
        direction: Direction::Credit,
        ..debit("2025-12-08", "10.99")
    });

    let detected = detect_recurring("Netflix", &txns);
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].period, Period::Monthly);
}

#[test]
fn statement_02_messy_recurring_set_is_exact() {
    // The contract test: `recurring` below 100% on fixtures is a Rust
    // bug, not a model problem (CLAUDE.md). Real parse, real cleanup,
    // deterministic grouping — no model anywhere.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit/fixtures/statement-02-messy.csv");
    let parsed = runner::parse::parse_statement_file(&path).expect("fixture parses");
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);

    let mut by_merchant: Vec<(String, Vec<Transaction>)> = Vec::new();
    for txn in parsed.transactions {
        let name = runner::cleanup::clean_merchant(&txn.raw_merchant);
        match by_merchant.iter_mut().find(|(m, _)| *m == name) {
            Some((_, txns)) => txns.push(txn),
            None => by_merchant.push((name, vec![txn])),
        }
    }

    let mut recurring: Vec<_> = by_merchant
        .iter()
        .flat_map(|(m, txns)| detect_recurring(m, txns))
        .collect();
    recurring.sort_by(|a, b| a.merchant.cmp(&b.merchant));

    // Exactly Disney+ (monthly, risen) and Amazon Prime (annual) — the
    // coffee shops and Amazon Marketplace must not appear.
    assert_eq!(
        recurring
            .iter()
            .map(|r| r.merchant.as_str())
            .collect::<Vec<_>>(),
        vec!["AMAZON PRIME", "DISNEYPLUS"],
        "{recurring:?}"
    );

    let prime = &recurring[0];
    assert_eq!(prime.period, Period::Annual);
    assert_eq!(prime.current_amount, Decimal::from_str("95.00").unwrap());
    assert!(prime.price_rise.is_none());

    let disney = &recurring[1];
    assert_eq!(disney.period, Period::Monthly);
    assert_eq!(disney.current_amount, Decimal::from_str("10.99").unwrap());
    let rise = disney
        .price_rise
        .as_ref()
        .expect("the fixture's mid-window rise");
    assert_eq!(rise.from, Decimal::from_str("7.99").unwrap());
    assert_eq!(rise.to, Decimal::from_str("10.99").unwrap());
    assert_eq!(rise.when, NaiveDate::from_str("2024-11-08").unwrap());
}

fn credit(date: &str, amount: &str) -> Transaction {
    Transaction {
        date: NaiveDate::from_str(date).expect("test date"),
        raw_merchant: String::new(),
        amount: Decimal::from_str(amount).expect("test amount"),
        direction: Direction::Credit,
    }
}

/// A salary: money coming in, monthly, on the 28th.
fn monthly_credits(n: u32, amount: &str) -> Vec<Transaction> {
    (0..n)
        .map(|i| credit(&format!("{}-{:02}-28", 2025 + (i / 12), 1 + i % 12), amount))
        .collect()
}

#[test]
fn monthly_salary_is_income_not_a_subscription() {
    let txns = monthly_credits(12, "2450.00");

    // It is emphatically not a billing series — that rule stands (#27).
    assert!(
        detect_recurring("Acme Payroll", &txns).is_empty(),
        "credits are never a subscription"
    );

    // But it is a series, and the report has a place for it. Before
    // this, twelve credits fell through to `outcome.other` and were
    // reported as regular *spending* — a person was told they spent
    // £29,400 at their employer (#79).
    let income = detect_income("Acme Payroll", &txns);
    assert_eq!(income.len(), 1, "one salary, one series");
    assert_eq!(income[0].merchant, "Acme Payroll");
    assert_eq!(income[0].period, Period::Monthly);
    assert_eq!(
        income[0].current_amount,
        Decimal::from_str("2450.00").unwrap()
    );
}

#[test]
fn a_merchant_split_across_processors_is_one_merchant_with_one_series() {
    // #261, from the bed's own rows: one rent, twelve monthly payments,
    // cycling three processors with the pennies drifting. It came out as
    // five series at Annual and Weekly — cadences nobody pays — because
    // `STRIPE* ` survived cleaning and the survivor then out-scored four
    // unrelated ALDER* merchants into one group.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../packs/app.kttl.subscription-audit/fixtures/\
         generated-development-ambiguous-categories-no-subscriptions-alder.csv",
    );
    let parsed = runner::parse::parse_statement_file(&path).expect("fixture parses");

    let rent: Vec<_> = runner::cleanup::group_transactions(&parsed.transactions)
        .into_iter()
        .filter(|group| group.cleaned.contains("HARTWELLLETTINGS"))
        .collect();

    assert_eq!(rent.len(), 1, "one merchant, one group: {rent:?}");
    let rent = &rent[0];
    assert_eq!(rent.cleaned, "HARTWELLLETTINGS");
    assert_eq!(
        rent.txns.len(),
        14,
        "every processor's rows, and only those"
    );

    // The bed's `expected.json` names one series in this fixture, and it
    // is not this one: penny-drifting payments have no exact cadence to
    // find, and the honest answer is to surface the merchant for review
    // rather than invent one. An invented series is an invented deadline.
    assert!(
        detect_recurring(&rent.cleaned, &rent.txns).is_empty(),
        "{:?}",
        detect_recurring(&rent.cleaned, &rent.txns)
    );
}

#[test]
fn generated_eval_bed_recurring_sets_are_exact() {
    let pack = runner::packs::load_pack(std::path::Path::new(
        "../../packs/app.kttl.subscription-audit",
    ))
    .expect("pack loads");
    let fixtures = runner::eval::fixture::fixtures_in(&pack).expect("fixtures load");

    for fixture in fixtures
        .iter()
        .filter(|fixture| fixture.expected.fixture_id.contains("-generated-"))
    {
        let parsed = runner::parse::parse_statement_file(&fixture.path)
            .unwrap_or_else(|error| panic!("{} parses: {error}", fixture.name));
        assert!(
            parsed.warnings.is_empty(),
            "{}: {:?}",
            fixture.name,
            parsed.warnings
        );

        // The pipeline's own grouping, not a second one that happens to
        // live in a test: grouping by `clean_merchant` alone checked a
        // pipeline the runner does not run, which is why #261's split
        // merchants sat in the baseline unread for a whole stage.
        let groups = runner::cleanup::group_transactions(&parsed.transactions);
        let mut got = groups
            .iter()
            .flat_map(|group| {
                detect_recurring(&group.cleaned, &group.txns)
                    .into_iter()
                    .map(|series| format!("{}/{}", group.raw_first, series.period.as_wire()))
            })
            .collect::<Vec<_>>();
        got.sort();

        // Both sides are keyed by the group a descriptor lands in, so a
        // merchant reached through three processors is one expectation
        // rather than three near-misses.
        let mut want = fixture
            .expected
            .recurring
            .iter()
            .map(|series| {
                let raw = fixture
                    .expected
                    .normalise
                    .iter()
                    .find(|expectation| expectation.name == series.merchant)
                    .expect("recurring merchant has a raw descriptor");
                let group = groups
                    .iter()
                    .find(|group| group.txns.iter().any(|txn| txn.raw_merchant == raw.raw))
                    .unwrap_or_else(|| panic!("{}: no group holds {}", fixture.name, raw.raw));
                format!("{}/{}", group.raw_first, series.period)
            })
            .collect::<Vec<_>>();
        want.sort();

        assert_eq!(got, want, "{}", fixture.name);
    }
}

// ── #253: a refunded charge is not a payment ────────────────────────

#[test]
fn a_refunded_and_rebought_annual_renewal_is_one_yearly_series() {
    // The bed's season ticket: £510, then a year on £540 charged,
    // refunded two days later, and re-bought the day after. The refund
    // pair is bookkeeping, not payments — what stood is £510 and £540 a
    // year apart, an annual renewal with a rise. Counting the pair as
    // evidence is what read this as intervals of 3 and 365 days, the
    // cadence #261's majority rule rightly declines.
    let txns = vec![
        debit("2024-08-20", "510.00"),
        debit("2025-08-20", "540.00"),
        credit("2025-08-22", "540.00"),
        debit("2025-08-23", "540.00"),
    ];

    let detected = detect_recurring("Trainline", &txns);
    assert_eq!(detected.len(), 1, "{detected:?}");
    let season = &detected[0];
    assert_eq!(season.period, Period::Annual);
    assert_eq!(season.current_amount, Decimal::from_str("540.00").unwrap());
    let rise = season.price_rise.as_ref().expect("£510 → £540 is a rise");
    assert_eq!(rise.from, Decimal::from_str("510.00").unwrap());
    assert_eq!(rise.to, Decimal::from_str("540.00").unwrap());
}

#[test]
fn a_recharge_with_no_refund_is_still_declined() {
    // The #261 guard stands where the extra charge is real: two charges
    // three days apart with nothing refunded are two payments, and a
    // cadence half the evidence contradicts is still not a cadence.
    let txns = vec![
        debit("2024-08-20", "510.00"),
        debit("2025-08-20", "540.00"),
        debit("2025-08-23", "540.00"),
    ];
    assert!(detect_recurring("Trainline", &txns).is_empty());
}

#[test]
fn a_reversal_only_cancels_a_payment_it_could_be_refunding() {
    // A credit with no earlier same-amount debit in reach cancels
    // nothing: money back that matches nothing recent is its own event,
    // not bookkeeping for the series.
    let mut txns = monthly(12, "10.99");
    txns.push(credit("2024-12-20", "10.99")); // before the series starts

    let detected = detect_recurring("Netflix", &txns);
    assert_eq!(detected.len(), 1, "{detected:?}");
    assert_eq!(detected[0].period, Period::Monthly);
}

#[test]
fn spending_is_never_mistaken_for_income() {
    assert!(
        detect_income("Netflix", &monthly(12, "10.99")).is_empty(),
        "debits are not income, however regular"
    );
}

// ── #271: declining a cadence is a fact about our certainty ─────────

#[test]
fn payments_that_look_periodic_are_flagged_even_when_no_series_is_certified() {
    // The 7B run's confident-wrong cell was 91% items like this: a real
    // subscription whose amounts drift by a penny, so exact-amount
    // clustering finds nothing, and the pipeline then asserts "regular
    // spending" carrying the model's confidence about a different
    // question entirely. Declining a cadence is a fact about *our*
    // certainty, and it has to be visible as one.
    let drifting: Vec<Transaction> = (0..12)
        .map(|i| {
            debit(
                &format!("2024-{:02}-03", i + 1),
                &format!("125.{:02}", 10 + i),
            )
        })
        .collect();

    assert!(
        detect_recurring("Alder Rent", &drifting).is_empty(),
        "no exact-amount series to certify — that part is #261 and stays"
    );
    assert!(
        runner::recurrence::looks_periodic(&drifting),
        "twelve payments a month apart are periodic, whatever the pennies do"
    );
}

#[test]
fn an_irregular_habit_is_not_flagged_as_periodic() {
    // The other half, and the one that decides whether this floods the
    // review list: a coffee habit is genuinely not recurring, so
    // declining a cadence for it is certain, not uncertain. It must
    // stay asserted.
    let coffee = vec![
        debit("2025-01-06", "3.40"),
        debit("2025-01-21", "4.10"),
        debit("2025-02-03", "3.20"),
        debit("2025-02-19", "4.80"),
        debit("2025-03-10", "3.40"),
        debit("2025-03-27", "3.90"),
    ];
    assert!(detect_recurring("Kaffa Coffee", &coffee).is_empty());
    assert!(
        !runner::recurrence::looks_periodic(&coffee),
        "irregular intervals are not a declined cadence, they are no cadence"
    );
}

#[test]
fn a_one_off_purchase_is_not_periodic() {
    let once = vec![debit("2025-03-04", "480.00")];
    assert!(!runner::recurrence::looks_periodic(&once));
}

#[test]
fn credits_alone_are_not_a_declined_spending_cadence() {
    // A salary is periodic, but it is income and `detect_income` owns
    // it. Flagging it here would surface every wage packet for review.
    assert!(!runner::recurrence::looks_periodic(&monthly_credits(
        12, "2450.00"
    )));
}
