//! Totals, annualisation and ordering (#29): turn a `RunOutcome` into
//! the `kettle/run-report@0` document the report and the app read.
//!
//! Every number here is `Decimal`. Money never floats (CLAUDE.md).
//!
//! CONTRACT: the signatures and the rules in these doc comments are
//! fixed — `results.rs` shapes and `fixtures/run-01/results.json` are
//! what the app already parses. Implementations may change freely.

use crate::fmt;
use crate::recurrence::Period;
use crate::results::{
    CheckYourself, Confidence, Evidence, Income, IntervalDays, NeedsReviewItem, PriceRiseOut,
    RecurringFinding, RegularSpend, RunInfo, RunReport, RunSummary, TransactionOut,
    RUN_REPORT_SCHEMA,
};
use crate::run::{Payload, RunOutcome};
use chrono::{Datelike, NaiveDate};
use rust_decimal::{Decimal, RoundingStrategy};

/// A subscription worth a second look purely because of what it costs,
/// however confident Kettle is about it (rule R3 below).
pub const WORTH_A_LOOK: Decimal = Decimal::ONE_HUNDRED;

/// What one series costs over a year at its cadence. Exact: 12.99
/// monthly is 155.88, not 155.88000000001.
///
/// Weekly ×52, monthly ×12, quarterly ×4, yearly ×1. A 52-week year is
/// the honest approximation — we say "a year", not "365.25 days".
pub fn annualise(amount: Decimal, period: Period) -> Decimal {
    let multiplier = match period {
        Period::Weekly => Decimal::from(52),
        Period::Monthly => Decimal::from(12),
        Period::Quarterly => Decimal::from(4),
        Period::Annual => Decimal::ONE,
    };
    amount * multiplier
}

/// Build the run's report document.
///
/// The caller owns `run` — the runner doesn't know its own run id,
/// model tier or timings; those come from whoever spawned the sidecar.
///
/// Rules, in order:
///
/// 1. **recurring** — one entry per finding, `annualised` from
///    `annualise(amount_current, period)`, sorted by `annualised`
///    descending, ties broken by merchant A–Z. `confidence` maps the
///    classifier's string; anything unrecognised is `Low`, never a
///    panic.
/// 2. **price_rise** — `month` is the rise's date as "YYYY-MM";
///    `extra_per_year` is `annualise(to - from, period)`.
/// 3. **evidence.reason** — one sentence, chosen by cadence:
///    - monthly: "{n} payments, one every month on or near the {5th}"
///      (median day of month, ordinal)
///    - weekly: "{n} payments, most weeks on or near {Tuesday}"
///      (median weekday)
///    - quarterly: "{n} payments, roughly 3 months apart"
///    - yearly: "{n} payments, roughly a year apart"
/// 4. **evidence.interval_days** — median and spread (max − min) of the
///    gaps between consecutive payments; `None` when there is only one
///    payment and therefore no gap to measure.
/// 5. **regular_spend** — `outcome.other` entries whose `kind` is not
///    income-shaped: `visits` is the transaction count, `total` their
///    sum, `typical_visit` the median, rounded half-up to 2dp.
/// 6. **income** — `outcome.income`, the regular money coming in.
///    Never totalled with spending, and never in `regular_spend`
///    (#79): a report that tells someone they spent their wages at
///    their employer is worse than one that says nothing.
/// 7. **summary** — `annualised_total` is the sum of `recurring`;
///    `monthly_equivalent` is that ÷ 12 rounded half-up to 2dp;
///    `price_rises` and `needs_review_count` are counts; `note` is
///    `review_note(needs_review_count)`.
/// 8. **needs_review** — carried straight through from the outcome,
///    each with `NOT_COUNTED_NOTE`.
/// 9. **check_yourself** — see `check_yourself` below.
pub fn build_report(outcome: &RunOutcome, run: RunInfo) -> Result<RunReport, String> {
    // `RunReport` is the Audit typology's report — every field below is
    // merchants, series and annualised money. The letter typology's
    // report is #243's; until it exists this says so plainly rather
    // than rendering a subscription report over a letter.
    let Payload::Audit(audit) = &outcome.payload else {
        return Err(
            "this pack's report needs a newer version of Kettle (letter reports)".to_owned(),
        );
    };

    let mut recurring: Vec<RecurringFinding> = audit
        .findings
        .iter()
        .map(|finding| {
            let annualised = annualise(finding.current_amount, finding.period);
            let transactions: Vec<TransactionOut> = finding
                .evidence
                .iter()
                .map(|e| TransactionOut {
                    date: e.date,
                    amount: e.amount,
                })
                .collect();
            let price_rise = finding.price_rise.as_ref().map(|rise| PriceRiseOut {
                from: rise.from,
                to: rise.to,
                month: fmt::iso_month(rise.when),
                extra_per_year: annualise(rise.to - rise.from, finding.period),
            });
            RecurringFinding {
                merchant: finding.merchant.clone(),
                raw_merchant: finding.raw_merchant.clone(),
                kind: finding.kind.clone(),
                category: finding.category.clone(),
                period: finding.period,
                amount_current: finding.current_amount,
                annualised,
                confidence: Confidence::parse(&finding.confidence),
                price_rise,
                evidence: evidence_for(finding.period, transactions),
            }
        })
        .collect();

    recurring.sort_by(|a, b| {
        b.annualised
            .cmp(&a.annualised)
            .then(a.merchant.cmp(&b.merchant))
    });

    let regular_spend: Vec<RegularSpend> = audit
        .other
        .iter()
        .map(|spend| {
            let mut amounts: Vec<Decimal> = spend.transactions.iter().map(|t| t.amount).collect();
            amounts.sort();
            RegularSpend {
                merchant: spend.merchant.clone(),
                raw_merchant: spend.raw_merchant.clone(),
                kind: spend.kind.clone(),
                category: spend.category.clone(),
                visits: spend.transactions.len(),
                total: amounts.iter().sum(),
                typical_visit: median_decimal(&amounts),
                confidence: Confidence::parse(&spend.confidence),
            }
        })
        .collect();

    let needs_review: Vec<NeedsReviewItem> = outcome
        .needs_review
        .iter()
        .map(|item| NeedsReviewItem {
            raw_merchant: item.subject.clone(),
            reason: item.reason.clone(),
            transactions: item
                .transactions
                .iter()
                .map(|e| TransactionOut {
                    date: e.date,
                    amount: e.amount,
                })
                .collect(),
            note: NOT_COUNTED_NOTE.to_owned(),
        })
        .collect();

    let annualised_total: Decimal = recurring.iter().map(|f| f.annualised).sum();
    let monthly_equivalent = (annualised_total / Decimal::from(12))
        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let price_rises = recurring.iter().filter(|f| f.price_rise.is_some()).count();
    let needs_review_count = needs_review.len();

    let summary = RunSummary {
        recurring_count: recurring.len(),
        annualised_total,
        monthly_equivalent,
        price_rises,
        needs_review_count,
        note: review_note(needs_review_count),
    };

    let check_list = check_yourself(&recurring);

    Ok(RunReport {
        schema: RUN_REPORT_SCHEMA.to_owned(),
        run,
        summary,
        recurring,
        regular_spend,
        income: audit
            .income
            .iter()
            .map(|earning| Income {
                merchant: earning.merchant.clone(),
                raw_merchant: earning.raw_merchant.clone(),
                period: earning.period,
                amount: earning.current_amount,
                confidence: Confidence::parse(&earning.confidence),
            })
            .collect(),
        needs_review,
        check_yourself: check_list,
    })
}

/// Weekday names in `chrono`'s Monday-first order, for "most weeks on
/// or near {Tuesday}".
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The evidence block for one finding: the reason sentence and the
/// interval spread, both derived from the transaction dates already
/// carried as evidence.
fn evidence_for(period: Period, transactions: Vec<TransactionOut>) -> Evidence {
    let n = transactions.len();
    let mut dates: Vec<NaiveDate> = transactions.iter().map(|t| t.date).collect();
    dates.sort();

    let reason = match period {
        Period::Monthly => {
            let days: Vec<i64> = dates.iter().map(|d| d.day() as i64).collect();
            let median_day = median_i64(&days);
            format!(
                "{n} payments, one every month on or near the {}",
                fmt::ordinal(median_day as u32)
            )
        }
        Period::Weekly => {
            let indices: Vec<i64> = dates
                .iter()
                .map(|d| d.weekday().num_days_from_monday() as i64)
                .collect();
            let median_weekday = median_i64(&indices);
            format!(
                "{n} payments, most weeks on or near {}",
                WEEKDAYS[median_weekday as usize]
            )
        }
        Period::Quarterly => format!("{n} payments, roughly 3 months apart"),
        Period::Annual => format!("{n} payments, roughly a year apart"),
    };

    let interval_days = if dates.len() < 2 {
        None
    } else {
        let gaps: Vec<i64> = dates.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
        let mut sorted_gaps = gaps.clone();
        sorted_gaps.sort();
        // Measured, not nominal. A calendar month is never exactly 30
        // days, so this jitters — but it sits beside a measured spread
        // under the heading "evidence", and a constant dressed as a
        // statistic is worse than an untidy true one. The cadence is
        // already said in words; this is the working shown.
        let median = median_i64(&sorted_gaps);
        let spread =
            sorted_gaps.last().copied().unwrap_or(0) - sorted_gaps.first().copied().unwrap_or(0);
        Some(IntervalDays { median, spread })
    };

    Evidence {
        reason,
        interval_days,
        transactions,
    }
}

/// The median of a sorted (or unsorted, we don't rely on order beyond
/// the two middle elements once sorted) list of integers. Even counts
/// average the two middle values.
fn median_i64(values: &[i64]) -> i64 {
    let mut sorted = values.to_vec();
    sorted.sort();
    let len = sorted.len();
    if len == 0 {
        return 0;
    }
    if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2
    }
}

/// The median of a sorted list of amounts, rounded half-up to 2dp.
fn median_decimal(sorted: &[Decimal]) -> Decimal {
    let len = sorted.len();
    let raw = if len == 0 {
        Decimal::ZERO
    } else if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / Decimal::from(2)
    };
    raw.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// The summary's honesty line: what the totals leave out.
pub fn review_note(needs_review_count: usize) -> String {
    match needs_review_count {
        0 => "Everything Kettle found is counted in these totals.".to_owned(),
        1 => {
            "One payment needs your eyes before it's counted — see 'Needs your review'.".to_owned()
        }
        n => {
            format!("{n} payments need your eyes before they're counted — see 'Needs your review'.")
        }
    }
}

/// What a person loses by leaving a review item alone: nothing.
pub const NOT_COUNTED_NOTE: &str = "Not counted in the totals until you confirm it.";

/// The honest checklist (#34's data; the report renders it).
///
/// Three rules, applied in this order, one entry per merchant — the
/// first rule that matches wins, so nothing is said twice:
///
/// - **R2 yearly**: any yearly finding. A once-a-year charge leaves one
///   mark in a twelve-month statement; that's thin evidence and we say
///   so, with the date the next one would land.
/// - **R1 uncertain**: `confidence` is not `High`.
/// - **R3 expensive habit**: at most one — the dearest subscription
///   still unclaimed by R1 or R2 that costs more than `WORTH_A_LOOK` a
///   year. Kettle knows the price, not whether they still use it, and
///   only they know that. One entry, not one per subscription: a
///   checklist that queries everything gets ignored wholesale.
///
/// The copy is authored, not generated: it reaches people, in British
/// English, and it must not sound like a fault report (CLAUDE.md).
pub fn check_yourself(recurring: &[RecurringFinding]) -> Vec<CheckYourself> {
    let mut matched = vec![false; recurring.len()];
    let mut entries: Vec<CheckYourself> = Vec::new();

    // R2 yearly.
    for (index, finding) in recurring.iter().enumerate() {
        if finding.period == Period::Annual {
            matched[index] = true;
            let last_charge = finding
                .evidence
                .transactions
                .iter()
                .map(|t| t.date)
                .max()
                .unwrap_or_default();
            let next_charge = last_charge
                .with_year(last_charge.year() + 1)
                .unwrap_or(last_charge);
            entries.push(CheckYourself {
                about: finding.merchant.clone(),
                why: why_yearly(&fmt::date(next_charge)),
            });
        }
    }

    // R1 uncertain.
    for (index, finding) in recurring.iter().enumerate() {
        if !matched[index] && finding.confidence != Confidence::High {
            matched[index] = true;
            entries.push(CheckYourself {
                about: finding.merchant.clone(),
                why: why_uncertain(finding.period),
            });
        }
    }

    // R3 expensive habit — the single biggest remaining subscription
    // over `WORTH_A_LOOK`. `recurring` is already sorted by annualised
    // descending, so the first eligible entry is the biggest.
    if let Some((_, finding)) = recurring.iter().enumerate().find(|(index, finding)| {
        !matched[*index] && finding.kind == "subscription" && finding.annualised > WORTH_A_LOOK
    }) {
        entries.push(CheckYourself {
            about: finding.merchant.clone(),
            why: why_expensive(&fmt::money(finding.annualised)),
        });
    }

    entries
}

/// R2's sentence. `next_charge` is the last payment plus a year,
/// formatted "14 March 2027".
pub fn why_yearly(next_charge: &str) -> String {
    format!(
        "Yearly costs only appear once in a 12-month statement, so there's less evidence to go \
         on. Check the renewal date — the next charge would land around {next_charge}."
    )
}

/// R1's sentence.
pub fn why_uncertain(period: Period) -> String {
    match period {
        Period::Quarterly => "Payments three months apart look quarterly, but some plans smooth \
                              payments differently across the year. Worth checking against your \
                              bill."
            .to_owned(),
        _ => "Kettle wasn't certain what this one is, so it's counted with less confidence than \
              the rest. Worth a look at your statement."
            .to_owned(),
    }
}

/// R3's sentence. `annualised` is formatted as "£299.88".
pub fn why_expensive(annualised: &str) -> String {
    format!(
        "Kettle can see what it costs ({annualised} a year) but not whether you still use it. \
         Only you know that."
    )
}
