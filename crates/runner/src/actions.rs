//! Proposed actions (#30): what a person might want to do about what
//! the run found, as reviewable cards — never as anything that happens.
//!
//! Kettle's capabilities are read-only (brief §1). Approving an action
//! only prepares it: a calendar file they add themselves, or text they
//! can copy. Every string here reaches a person, so it is plain British
//! English, and the copy is authored rather than generated — a card
//! that reads like a log line is a bug.
//!
//! CONTRACT: signatures, the copy constants and the rules below are
//! fixed. `fixtures/run-01/actions.json` shows the shape; the app's
//! review screen already parses it.

use crate::aggregate::WORTH_A_LOOK;
use crate::fmt;
use crate::recurrence::Period;
use crate::results::{
    ActionExport, ActionKind, IcsExport, ProposedAction, ProposedActions, RunReport, ACTIONS_NOTE,
    PROPOSED_ACTIONS_SCHEMA, STATUS_PROPOSED,
};
use chrono::{Datelike, Months, NaiveDate};
use std::collections::BTreeMap;

/// Propose actions for a completed run.
///
/// `today` is passed in rather than read from the clock: a run's
/// outputs must be reproducible, and tests must not drift into next
/// month.
///
/// One action per finding that earns one, in this order — the order is
/// the order they appear on screen, so it is the order of usefulness:
///
/// 1. **ReviewPriceRise** — every finding with a `price_rise`.
/// 2. **ReviewSubscription** — at most one: the single most expensive
///    finding whose `kind` is "subscription", whose cadence is not
///    yearly, and which costs more than `aggregate::WORTH_A_LOOK` a
///    year (ties broken by merchant A–Z). One card, not one per
///    subscription — "the biggest easy saving" is a selection, and a
///    card for every subscription over £100 would nag.
/// 3. **CheckRenewal** — every yearly finding.
/// 4. **CalendarReminder** — every finding whose `kind` is "utility":
///    a quarterly bill that surprises the month's budget is the whole
///    complaint these packs exist to answer.
///
/// Ids are "act-01", "act-02", … in emission order. `status` is always
/// `STATUS_PROPOSED`; `note` is always `ACTIONS_NOTE`.
///
/// Calendar dates, all derived from `today` or from the last payment —
/// never "now":
///
/// - price rise → the first of next month (a monthly-bill decision
///   belongs at the start of a month)
/// - subscription → `today` + 7 days
/// - renewal → 7 days before the next expected charge (last payment
///   plus a year)
/// - reminder → 2 days before the next expected payment (last payment
///   plus one period, repeatedly, until it is after `today`)
pub fn propose_actions(report: &RunReport, today: NaiveDate) -> ProposedActions {
    let mut next_id = 1u32;
    let mut actions = Vec::new();

    // 1. ReviewPriceRise — every finding with a price rise.
    for finding in &report.recurring {
        let Some(rise) = &finding.price_rise else {
            continue;
        };
        let month_said_plainly = iso_month_to_date(&rise.month).map(fmt::month);
        let facts = CardFacts {
            id: next_action_id(&mut next_id),
            merchant: finding.merchant.clone(),
            before: Some(fmt::money(rise.from)),
            amount: fmt::money(rise.to),
            annualised: fmt::money(finding.annualised),
            extra_per_year: Some(fmt::money(rise.extra_per_year)),
            period: finding.period,
            month: month_said_plainly,
            last_charged: None,
            ics_date: first_of_next_month(today),
            next_charge: None,
            since: None,
        };
        let mut action = card(ActionKind::ReviewPriceRise, &facts);
        let mut evidence = BTreeMap::new();
        evidence.insert("merchant".to_owned(), finding.merchant.clone());
        evidence.insert("before".to_owned(), plain(rise.from));
        evidence.insert("after".to_owned(), plain(rise.to));
        evidence.insert("month".to_owned(), rise.month.clone());
        action.evidence = evidence;
        actions.push(action);
    }

    // 2. ReviewSubscription — the biggest easy saving: the qualifying
    // subscription (not yearly, costs more than WORTH_A_LOOK a year)
    // with the largest annualised cost.
    let subscription = report
        .recurring
        .iter()
        .filter(|f| {
            f.kind == "subscription" && f.period != Period::Annual && f.annualised > WORTH_A_LOOK
        })
        .max_by(|a, b| {
            a.annualised
                .cmp(&b.annualised)
                .then_with(|| b.merchant.cmp(&a.merchant))
        });
    if let Some(finding) = subscription {
        let since_date = finding
            .evidence
            .transactions
            .first()
            .map(|t| t.date)
            .unwrap_or(today);
        let facts = CardFacts {
            id: next_action_id(&mut next_id),
            merchant: finding.merchant.clone(),
            before: None,
            amount: fmt::money(finding.amount_current),
            annualised: fmt::money(finding.annualised),
            extra_per_year: None,
            period: finding.period,
            month: None,
            last_charged: None,
            ics_date: today + chrono::Duration::days(7),
            next_charge: None,
            since: Some(fmt::month(since_date)),
        };
        let mut action = card(ActionKind::ReviewSubscription, &facts);
        let mut evidence = BTreeMap::new();
        evidence.insert("merchant".to_owned(), finding.merchant.clone());
        evidence.insert("amount".to_owned(), plain(finding.amount_current));
        evidence.insert("period".to_owned(), finding.period.as_wire().to_owned());
        evidence.insert("annualised".to_owned(), plain(finding.annualised));
        action.evidence = evidence;
        actions.push(action);
    }

    // 3. CheckRenewal — every yearly finding.
    for finding in &report.recurring {
        if finding.period != Period::Annual {
            continue;
        }
        let Some(last_charged) = finding.evidence.transactions.last().map(|t| t.date) else {
            continue;
        };
        let next_charge = next_occurrence(last_charged, finding.period, today);
        let facts = CardFacts {
            id: next_action_id(&mut next_id),
            merchant: finding.merchant.clone(),
            before: None,
            amount: fmt::money(finding.amount_current),
            annualised: fmt::money(finding.annualised),
            extra_per_year: None,
            period: finding.period,
            month: None,
            last_charged: Some(last_charged),
            ics_date: next_charge - chrono::Duration::days(7),
            next_charge: Some(fmt::date(next_charge)),
            since: None,
        };
        let mut action = card(ActionKind::CheckRenewal, &facts);
        let mut evidence = BTreeMap::new();
        evidence.insert("merchant".to_owned(), finding.merchant.clone());
        evidence.insert("amount".to_owned(), plain(finding.amount_current));
        evidence.insert("period".to_owned(), finding.period.as_wire().to_owned());
        evidence.insert("last_charged".to_owned(), last_charged.to_string());
        action.evidence = evidence;
        actions.push(action);
    }

    // 4. CalendarReminder — every utility finding.
    for finding in &report.recurring {
        if finding.kind != "utility" {
            continue;
        }
        let Some(last_charged) = finding.evidence.transactions.last().map(|t| t.date) else {
            continue;
        };
        let next_charge = next_occurrence(last_charged, finding.period, today);
        let facts = CardFacts {
            id: next_action_id(&mut next_id),
            merchant: finding.merchant.clone(),
            before: None,
            amount: fmt::money(finding.amount_current),
            annualised: fmt::money(finding.annualised),
            extra_per_year: None,
            period: finding.period,
            month: None,
            last_charged: Some(last_charged),
            ics_date: next_charge - chrono::Duration::days(2),
            next_charge: Some(fmt::date(next_charge)),
            since: None,
        };
        let mut action = card(ActionKind::CalendarReminder, &facts);
        let mut evidence = BTreeMap::new();
        evidence.insert("merchant".to_owned(), finding.merchant.clone());
        evidence.insert("amount".to_owned(), plain(finding.amount_current));
        evidence.insert("period".to_owned(), finding.period.as_wire().to_owned());
        evidence.insert("last_charged".to_owned(), last_charged.to_string());
        action.evidence = evidence;
        actions.push(action);
    }

    ProposedActions {
        schema: PROPOSED_ACTIONS_SCHEMA.to_owned(),
        run_id: report.run.id.clone(),
        note: ACTIONS_NOTE.to_owned(),
        actions,
    }
}

fn next_action_id(counter: &mut u32) -> String {
    let id = format!("act-{:02}", counter);
    *counter += 1;
    id
}

/// "10.99", "299.88" — the plain two-decimal-place number a person's
/// bank statement shows, no currency sign, no thousands separator. For
/// evidence fields, which downstream code (and other packs' tooling)
/// may parse.
fn plain(amount: rust_decimal::Decimal) -> String {
    format!("{:.2}", amount.round_dp(2))
}

/// "2025-05" -> the first of that month, so `fmt::month`/`fmt::date`
/// helpers (which take a `NaiveDate`) can format it for people.
///
/// `None` rather than a panic on anything else: a `RunReport` is read
/// back from `results.json` on disk, and a file someone has edited by
/// hand must not take the run down with it. The card then simply says
/// less (CLAUDE.md: never crash a run on one bad input).
fn iso_month_to_date(iso_month: &str) -> Option<NaiveDate> {
    let (year, month) = iso_month.split_once('-')?;
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)
}

/// The first of the month after `today` — a monthly-bill decision
/// belongs at the start of a month.
fn first_of_next_month(today: NaiveDate) -> NaiveDate {
    let first_this_month =
        NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("the 1st always exists");
    first_this_month
        .checked_add_months(Months::new(1))
        .expect("a month after a valid date is a valid date")
}

/// `last` plus one `period`, repeatedly, until the result is after
/// `today` — the next time a recurring charge is expected to land.
fn next_occurrence(last: NaiveDate, period: Period, today: NaiveDate) -> NaiveDate {
    let mut next = last;
    loop {
        next = add_period(next, period);
        if next > today {
            return next;
        }
    }
}

fn add_period(date: NaiveDate, period: Period) -> NaiveDate {
    match period {
        Period::Weekly => date + chrono::Duration::days(7),
        Period::Monthly => date.checked_add_months(Months::new(1)).expect("valid date"),
        Period::Quarterly => date.checked_add_months(Months::new(3)).expect("valid date"),
        Period::Annual => date
            .checked_add_months(Months::new(12))
            .expect("valid date"),
    }
}

/// One proposed action's card copy. Split out so the tests can name a
/// single card's wording without building a whole report.
pub fn card(kind: ActionKind, facts: &CardFacts) -> ProposedAction {
    let (title, detail) = match kind {
        ActionKind::ReviewPriceRise => {
            let before = facts
                .before
                .as_deref()
                .expect("price rise card needs `before`");
            // A month we couldn't read is said vaguely rather than
            // wrongly — the rise itself is still worth telling them.
            let month = facts.month.as_deref().unwrap_or("an earlier month");
            let extra_per_year = facts
                .extra_per_year
                .as_deref()
                .expect("price rise card needs `extra_per_year`");
            (
                price_rise_title(&facts.merchant, before, &facts.amount, month),
                price_rise_detail(extra_per_year),
            )
        }
        ActionKind::ReviewSubscription => {
            let since = facts
                .since
                .as_deref()
                .expect("subscription card needs `since`");
            (
                subscription_title(&facts.merchant, &facts.annualised),
                subscription_detail(&facts.amount, cadence_phrase(facts.period), since),
            )
        }
        ActionKind::CheckRenewal => {
            let next_charge = facts
                .next_charge
                .as_deref()
                .expect("renewal card needs `next_charge`");
            (
                renewal_title(&facts.merchant, next_charge, &facts.amount),
                RENEWAL_DETAIL.to_owned(),
            )
        }
        ActionKind::CalendarReminder => {
            let when = facts
                .next_charge
                .as_deref()
                .expect("reminder card needs `next_charge`");
            (
                reminder_title(&facts.merchant, when),
                reminder_detail(&facts.amount, cadence_phrase(facts.period)),
            )
        }
    };

    let (summary, text) = export_copy(kind, facts);

    ProposedAction {
        id: facts.id.clone(),
        kind,
        title,
        detail,
        evidence: BTreeMap::new(),
        disputed: Vec::new(),
        export: ActionExport {
            ics: Some(IcsExport {
                summary,
                date: facts.ics_date,
            }),
            text,
        },
        status: STATUS_PROPOSED.to_owned(),
    }
}

/// The calendar summary and copyable text for one card. Not part of the
/// authored copy constants below — those are the on-screen title and
/// detail — but held to the same rules: plain British English, and
/// never a promise that Kettle has done or will do anything itself.
fn export_copy(kind: ActionKind, facts: &CardFacts) -> (String, String) {
    match kind {
        ActionKind::ReviewPriceRise => {
            let before = facts.before.as_deref().unwrap_or_default();
            let month = facts.month.as_deref().unwrap_or_default();
            let extra_per_year = facts.extra_per_year.as_deref().unwrap_or_default();
            (
                format!(
                    "Review {} subscription (rose to {})",
                    facts.merchant, facts.amount
                ),
                // "+£24.00/year", not "£24.00/year extra" — the design
                // fixture's wording, which a person wrote. The runner
                // is the source of truth for these documents now, so
                // the authored copy has to live here.
                format!(
                    "Review {}: price rose from {before} to {} in {month} (+{extra_per_year}/year).",
                    facts.merchant, facts.amount
                ),
            )
        }
        ActionKind::ReviewSubscription => (
            format!(
                "Decide: keep or cancel {} ({}/{})",
                facts.merchant,
                facts.amount,
                cadence_word(facts.period)
            ),
            format!(
                "{}: {}/{}, {}/year. Decide whether to keep it.",
                facts.merchant,
                facts.amount,
                cadence_word(facts.period),
                facts.annualised
            ),
        ),
        ActionKind::CheckRenewal => {
            let next_charge = facts.next_charge.as_deref().unwrap_or_default();
            (
                format!(
                    "{} renews soon — {}. Keep it?",
                    facts.merchant, facts.amount
                ),
                format!(
                    "{} renews around {next_charge} ({}/year). Decide the week before.",
                    facts.merchant, facts.amount
                ),
            )
        }
        ActionKind::CalendarReminder => {
            let next_charge = facts.next_charge.as_deref().unwrap_or_default();
            (
                format!(
                    "{} {} payment due (~{})",
                    facts.merchant,
                    cadence_word(facts.period),
                    facts.amount
                ),
                format!(
                    "{} {} payment (~{}) due around {next_charge}.",
                    facts.merchant,
                    cadence_word(facts.period),
                    facts.amount
                ),
            )
        }
    }
}

/// "month", "quarter" — the export copy's shorter cadence word.
fn cadence_word(period: Period) -> &'static str {
    match period {
        Period::Weekly => "week",
        Period::Monthly => "month",
        Period::Quarterly => "quarter",
        Period::Annual => "year",
    }
}

/// Everything the copy templates need about one finding, already
/// formatted for people: money as "£12.99", dates as "14 March 2027",
/// months as "May 2025".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFacts {
    pub id: String,
    pub merchant: String,
    /// Present on price-rise cards.
    pub before: Option<String>,
    pub amount: String,
    pub annualised: String,
    /// The rise's cost over a year, e.g. "£24.00".
    pub extra_per_year: Option<String>,
    pub period: Period,
    /// "May 2025" — when the rise landed.
    pub month: Option<String>,
    pub last_charged: Option<NaiveDate>,
    /// The date the .ics event should sit on.
    pub ics_date: NaiveDate,
    /// The next expected charge, "14 March 2027", for renewal cards.
    pub next_charge: Option<String>,
    /// The first month a series was seen, "January 2025".
    pub since: Option<String>,
}

// ---------------------------------------------------------------------------
// Authored copy. British English; no jargon; never blames the person.

pub fn price_rise_title(merchant: &str, before: &str, after: &str, month: &str) -> String {
    format!("{merchant} rose {before} → {after} in {month}")
}

pub fn price_rise_detail(extra_per_year: &str) -> String {
    format!(
        "That's an extra {extra_per_year} a year. If you'd rather not pay it, a cheaper plan or \
         a pause might suit — worth a look."
    )
}

pub fn subscription_title(merchant: &str, annualised: &str) -> String {
    format!("{merchant} costs {annualised} a year — still using it?")
}

pub fn subscription_detail(amount: &str, cadence: &str, since: &str) -> String {
    format!(
        "{amount} {cadence} since at least {since}. If you're using it, ignore this. If not, \
         it's one of the easier savings in this statement."
    )
}

pub fn renewal_title(merchant: &str, next_charge: &str, amount: &str) -> String {
    format!("{merchant} renews around {next_charge} ({amount})")
}

pub const RENEWAL_DETAIL: &str =
    "Yearly plans are easy to forget. A reminder a week before gives you time to decide.";

pub fn reminder_title(merchant: &str, when: &str) -> String {
    format!("Next {merchant} payment lands around {when}")
}

pub fn reminder_detail(amount: &str, cadence: &str) -> String {
    format!("{amount} {cadence} so far. A reminder means it never surprises the month's budget.")
}

/// "every month", "each quarter" — how a cadence reads mid-sentence.
pub fn cadence_phrase(period: Period) -> &'static str {
    match period {
        Period::Weekly => "every week",
        Period::Monthly => "every month",
        Period::Quarterly => "each quarter",
        Period::Annual => "each year",
    }
}

// ── The Extraction typology's actions (#243) ────────────────────────

/// Turn a letter's obligations into cards a person can approve.
///
/// One card per obligation, in the order [`crate::timeline`] left them
/// — soonest first, undated last — because that is the order somebody
/// can act in. Every obligation gets a card, including the undated
/// ones: an ask Kettle could not date is still an ask, and dropping it
/// for being awkward is how a deadline goes unmet.
///
/// The read-only rule is unchanged (CLAUDE.md). Kettle never writes to
/// a calendar; it exports .ics or copyable text and a person decides.
/// Unlike [`propose_actions`], this takes no `today`: every date on a
/// letter card comes from the letter, and an undated ask exports as
/// text with no event, so there is nothing for a clock to fill in and
/// no way for one to (#399). `run_id` is taken because the extraction
/// outcome does not know which run produced it, and an actions
/// document that cannot say is one restart hydration rightly refuses
/// (#389).
pub fn propose_letter_actions(
    outcome: &crate::run::ExtractionOutcome,
    run_id: &str,
) -> ProposedActions {
    let actions = outcome
        .obligations
        .iter()
        .enumerate()
        .map(|(index, obligation)| {
            let mut evidence = std::collections::BTreeMap::new();
            evidence.insert("asked_by".to_owned(), obligation.party.clone());
            evidence.insert("in_the_letter".to_owned(), obligation.deadline.clone());
            for (n, passage) in obligation.evidence.iter().enumerate() {
                evidence.insert(format!("passage_{}", n + 1), passage.text.clone());
            }

            // An undated obligation is offered without a date being
            // invented for it. The .ics still has to sit somewhere, so
            // it sits today — but the card says plainly that the date
            // is the person's to set, and never presents today as the
            // deadline.
            let detail = match obligation.due {
                Some(due) => format!(
                    "{} asked for this by {}. The letter says \"{}\".",
                    obligation.party,
                    crate::fmt::date(due.date),
                    obligation.deadline
                ),
                None => format!(
                    "{} asked for this, but the letter does not give a date Kettle \
                     could work out — it says \"{}\". Choose a date that suits you.",
                    obligation.party, obligation.deadline
                ),
            };

            ProposedAction {
                id: format!("act-{:02}", index + 1),
                kind: ActionKind::CalendarReminder,
                title: obligation.ask.clone(),
                detail,
                evidence,
                disputed: obligation.disputed.iter().map(Into::into).collect(),
                export: ActionExport {
                    // No due date, no event: a calendar entry dated today
                    // would be a deadline the letter never set.
                    ics: obligation.due.map(|due| IcsExport {
                        summary: format!("{} — {}", obligation.ask, obligation.party),
                        date: due.date,
                    }),
                    text: format!(
                        "{} ({}) — {}",
                        obligation.ask, obligation.party, obligation.deadline
                    ),
                },
                status: STATUS_PROPOSED.to_owned(),
            }
        })
        .collect();

    ProposedActions {
        schema: crate::results::PROPOSED_ACTIONS_SCHEMA.to_owned(),
        run_id: run_id.to_owned(),
        note: ACTIONS_NOTE.to_owned(),
        actions,
    }
}
