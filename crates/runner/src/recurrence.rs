//! Recurrence detection: which of a merchant's payments form a
//! repeating series, at what cadence, at what current price (#27, #28).
//! Deterministic by contract — below 100% on fixtures is a Rust bug.
//!
//! The shape that makes the hard cases fall out: cluster a merchant's
//! payments by exact amount *first*, then look for cadence per cluster.
//! Concurrent subscriptions at different prices (two Amazon subs)
//! separate naturally instead of poisoning each other's intervals.

use crate::parse::{Direction, Transaction};
use chrono::NaiveDate;
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Weekly,
    Monthly,
    Quarterly,
    Annual,
}

/// A detected price change within one series (#28) — kept as from/to/when
/// so the report and actions can say "rose £7.99 → £10.99 in November".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceRise {
    pub from: Decimal,
    pub to: Decimal,
    pub when: NaiveDate,
}

/// One recurring series for a merchant. A merchant can have several
/// (concurrent subscriptions), which is why detection returns a Vec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurring {
    pub merchant: String,
    pub period: Period,
    pub current_amount: Decimal,
    pub price_rise: Option<PriceRise>,
}

/// Median inter-payment interval in days → cadence, using the bands from
/// brief §3. Two payments prove nothing at monthly cadence — but two a
/// year apart are how an annual subscription looks in any normal
/// statement window, and those are the highest-value findings.
///
/// The median picks the candidate; a majority of the intervals then has
/// to agree with it. Median alone let intervals of 3 and 365 days — a
/// refund and its re-charge, plus one payment the year before — band as
/// an annual subscription (#261). A cadence that half the evidence
/// contradicts is not a cadence, and declining costs a review item
/// where inventing costs a deadline nobody has.
fn period_of(dates: &[NaiveDate]) -> Option<Period> {
    let mut intervals: Vec<i64> = dates.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
    if intervals.is_empty() {
        return None;
    }
    intervals.sort_unstable();
    let period = band_of(intervals[intervals.len() / 2])?;
    // Fewer than three payments only counts at annual cadence.
    if dates.len() < 3 && period != Period::Annual {
        return None;
    }
    let agreeing = intervals
        .iter()
        .filter(|interval| band_of(**interval) == Some(period))
        .count();
    (agreeing * 2 > intervals.len()).then_some(period)
}

/// The cadence one interval reads as on its own, or none if it reads as
/// no cadence at all.
fn band_of(days: i64) -> Option<Period> {
    match days {
        6..=8 => Some(Period::Weekly),
        27..=34 => Some(Period::Monthly),
        84..=98 => Some(Period::Quarterly),
        350..=380 => Some(Period::Annual),
        _ => None,
    }
}

/// Days a period nominally spans, for judging whether a later cluster
/// continues an earlier one's cadence.
fn nominal_days(period: Period) -> i64 {
    match period {
        Period::Weekly => 7,
        Period::Monthly => 30,
        Period::Quarterly => 91,
        Period::Annual => 365,
    }
}

/// An amount cluster being grown into a series by merging.
struct Series {
    amount: Decimal,
    dates: Vec<NaiveDate>,
    rise: Option<PriceRise>,
}

/// Would `dates` followed by `next` read as one uninterrupted series?
/// Yes when they don't overlap, the combined cadence still bands, and
/// the hand-over gap is about one period — a price change is billed on
/// the usual day, so the gap is ~1 period; anything much longer is a
/// cancellation followed by something new.
fn continues(dates: &[NaiveDate], next: &[NaiveDate]) -> Option<Period> {
    let (last, first) = (dates.last()?, next.first()?);
    if first <= last {
        return None; // interleaved: concurrent series, never one
    }
    let combined: Vec<NaiveDate> = dates.iter().chain(next).copied().collect();
    let period = period_of(&combined)?;
    let gap = (*first - *last).num_days();
    (gap <= nominal_days(period) * 8 / 5).then_some(period)
}

/// Do this merchant's payments *look* periodic, whatever the amounts
/// do (#271)?
///
/// Detection clusters by exact amount first, which is what stops a
/// coffee habit becoming a subscription — and what makes a rent that
/// creeps a penny a month invisible to it. Declining that rent is
/// right: there is no cadence whose price we could state. But the
/// pipeline has to be able to say *why* it declined, because "these
/// payments have no pattern" and "these payments are obviously monthly
/// and we could not pin one down" are different facts about our own
/// certainty — and only the second belongs in front of a person.
///
/// Dates only, debits only. Dates, because amounts are exactly what
/// detection already failed on. Debits, because a salary is periodic
/// and `detect_income` owns it; flagging it here would put every wage
/// packet in the review list.
pub fn looks_periodic(txns: &[Transaction]) -> bool {
    let mut dates: Vec<NaiveDate> = txns
        .iter()
        .filter(|t| t.direction == Direction::Debit)
        .map(|t| t.date)
        .collect();
    dates.sort_unstable();
    dates.dedup();
    period_of(&dates).is_some()
}

/// Find the recurring series in one merchant's transactions.
///
/// Money going out only. Credits are refunds or transfers, never part
/// of a billing series — money coming in regularly is income, and
/// `detect_income` is where it goes.
pub fn detect_recurring(merchant: &str, txns: &[Transaction]) -> Vec<Recurring> {
    detect_series(merchant, txns, Direction::Debit)
}

/// Find the regular income in one merchant's transactions: the same
/// cadence machinery, pointed at money coming in (#79).
///
/// Wages, a pension, rent received. Before this existed, a salary's
/// credits matched nothing and fell through to "regular spending" — a
/// person was told they had spent their year's wages at their
/// employer.
pub fn detect_income(merchant: &str, txns: &[Transaction]) -> Vec<Recurring> {
    detect_series(merchant, txns, Direction::Credit)
}

/// How long after a payment its reversal can arrive and still read as
/// bookkeeping for that payment rather than as money in its own right.
/// About a statement cycle: the fixtures' slowest is a 7-day
/// chargeback, and a refund landing a month later still nets.
const REVERSAL_WINDOW_DAYS: i64 = 45;

/// The payments that stood: one direction's transactions, minus any
/// that a matching opposite-direction transaction reversed (#253).
///
/// A refunded charge is not a payment. Counting it as one is how the
/// season ticket — renewed yearly, refunded and re-bought once — read
/// as intervals of 3 and 365 days, a cadence #261's majority rule
/// rightly declines. Netting removes the bookkeeping pair before
/// clustering; whatever cadence the surviving payments carry is real
/// evidence, judged by the same rules as ever. A reversal pairs with
/// the latest same-amount payment at or before it within the window;
/// one that matches nothing cancels nothing.
fn net_of_reversals(txns: &[Transaction], direction: Direction) -> Vec<&Transaction> {
    let mut payments: Vec<&Transaction> =
        txns.iter().filter(|t| t.direction == direction).collect();
    payments.sort_by_key(|t| t.date);
    let mut reversals: Vec<&Transaction> =
        txns.iter().filter(|t| t.direction != direction).collect();
    reversals.sort_by_key(|t| t.date);

    let mut cancelled = vec![false; payments.len()];
    for reversal in reversals {
        let matched = payments
            .iter()
            .enumerate()
            .rev()
            .find(|(index, payment)| {
                !cancelled[*index]
                    && payment.amount == reversal.amount
                    && payment.date <= reversal.date
                    && (reversal.date - payment.date).num_days() <= REVERSAL_WINDOW_DAYS
            })
            .map(|(index, _)| index);
        if let Some(index) = matched {
            cancelled[index] = true;
        }
    }
    payments
        .into_iter()
        .zip(cancelled)
        .filter(|(_, cancelled)| !cancelled)
        .map(|(payment, _)| payment)
        .collect()
}

/// One direction's repeating series. Both callers want identical
/// cadence and price-change logic; only the direction differs, and
/// keeping one implementation is what stops the two drifting.
fn detect_series(merchant: &str, txns: &[Transaction], direction: Direction) -> Vec<Recurring> {
    let moving = net_of_reversals(txns, direction);

    // Cluster by exact amount, keeping clusters in first-seen order.
    let mut clusters: Vec<(Decimal, Vec<NaiveDate>)> = Vec::new();
    for txn in &moving {
        match clusters
            .iter_mut()
            .find(|(amount, _)| *amount == txn.amount)
        {
            Some((_, dates)) => dates.push(txn.date),
            None => clusters.push((txn.amount, vec![txn.date])),
        }
    }

    // A price change splits one subscription across two amount
    // clusters (#28). Rejoin: a cluster that continues an earlier
    // series' cadence merges into it — at the new price, flagged as a
    // rise when it went up. Most recent series first so a chain of
    // changes folds left to right.
    let mut series: Vec<Series> = Vec::new();
    for (amount, dates) in clusters {
        let joined = series
            .iter_mut()
            .rev()
            .find(|earlier| continues(&earlier.dates, &dates).is_some());
        match joined {
            Some(earlier) => {
                let when = dates[0];
                if amount > earlier.amount {
                    earlier.rise = Some(PriceRise {
                        from: earlier.amount,
                        to: amount,
                        when,
                    });
                }
                earlier.amount = amount;
                earlier.dates.extend(dates);
            }
            None => series.push(Series {
                amount,
                dates,
                rise: None,
            }),
        }
    }

    series
        .into_iter()
        .filter_map(|s| {
            period_of(&s.dates).map(|period| Recurring {
                merchant: merchant.to_owned(),
                period,
                current_amount: s.amount,
                price_rise: s.rise,
            })
        })
        .collect()
}
