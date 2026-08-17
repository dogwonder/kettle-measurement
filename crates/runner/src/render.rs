//! The report (#32, #33, #34): one self-contained HTML file a person
//! can read offline, print, or keep.
//!
//! Hard constraints, all testable:
//!
//! - **No external assets.** Inline CSS only; no `<script src>`, no
//!   `<link href>`, no remote images, no web fonts. The app shows this
//!   in a sandboxed iframe and the whole point is that it works with
//!   the network off.
//! - **Supported report subset.** Ordinary document markup is allowed.
//!   The only URL-bearing value allowed is an in-document `#fragment`
//!   (including SVG `href`). CSS may be inline or in `<style>`, but may
//!   not use `url()` or `@import`. Data URIs, active embedded documents,
//!   scripts, event handlers, forms and refresh redirects are refused.
//!   Kettle's own report needs none of them, and refusing data URIs
//!   avoids treating nested SVG references as inert.
//! - **Trust boundary.** This validation is defence-in-depth, not a
//!   substitute for #93's author/signature trust or the app's iframe
//!   sandbox. It also protects a standalone exported report, where the
//!   iframe sandbox is absent.
//! - **The report never depends on the model existing.** The prose
//!   summary step is optional; when it is skipped or fails, the
//!   deterministic `fallback_summary` fills the space and says nothing
//!   the numbers don't support (#33).
//! - **Low confidence is a finding, not an error.** It renders as an
//!   honest checklist, "Check these yourself", never as a warning
//!   banner (#34).
//!
//! `packs/app.kttl.subscription-audit/report.html.tera` is the report
//! source. `fixtures/run-01/report.html` is a rendered example kept in
//! sync through `kettle render` (#225), not a second template.
//!
//! CONTRACT: signatures fixed. The template's markup is not — matching
//! the fixture's look is the job.
//!
//! Everything a person reads is formatted here, in Rust, through
//! `crate::fmt`, and handed to the template as finished strings. The
//! template arranges; it never does arithmetic and never reformats an
//! amount.

use crate::fmt;
use crate::recurrence::Period;
use crate::results::{
    CheckYourself, Confidence, Income, NeedsReviewItem, PriceRiseOut, RecurringFinding,
    RegularSpend, RunReport, TransactionOut,
};
use chrono::{Datelike, NaiveDate};
use cssparser::{
    ParseError as CssParseError, Parser as CssParser, ParserInput as CssParserInput,
    Token as CssToken,
};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token as HtmlToken, TokenSink, TokenSinkResult, Tokenizer,
    TokenizerOpts,
};
use serde::Serialize;
use std::cell::{Cell, RefCell};
use tera::{Context, Tera};

#[derive(Debug)]
pub enum RenderError {
    /// The template didn't parse, or a field it names doesn't exist.
    Template(String),
    /// The rendered page reaches outside itself. Refused rather than
    /// written: a report that phones home is worse than no report.
    ExternalAsset(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Template(detail) => write!(f, "the report template is wrong: {detail}"),
            RenderError::ExternalAsset(detail) => {
                write!(f, "the report template is not self-contained: {detail}")
            }
        }
    }
}

impl std::error::Error for RenderError {}

/// Render `report` through `template` (the pack's `report.html.tera`).
///
/// `model_summary` is the optional prose step's output. `None` — the
/// step was skipped, failed, or the pack doesn't have one — means
/// `fallback_summary` is used instead. The reader is never told which
/// they got; both are true, and one of them is just cheaper.
///
/// The rendered page is checked for external references before it is
/// returned (`assert_self_contained`).
pub fn render_report(
    template: &str,
    report: &RunReport,
    model_summary: Option<&str>,
) -> Result<String, RenderError> {
    let prose = match model_summary.map(str::trim) {
        Some(prose) if !prose.is_empty() => prose.to_owned(),
        _ => fallback_summary(report),
    };

    let mut context = Context::new();
    context.insert("report", &ReportView::build(report, prose));

    let html = Tera::one_off(template, &context, true).map_err(template_error)?;
    assert_self_contained(&html)?;
    Ok(html)
}

/// The summary in words, generated from the numbers alone (#33).
///
/// Says, in this order and only where there is something to say: what
/// the recurring total is and its monthly equivalent; how many
/// subscriptions that covers; that a price rise was found, and whose;
/// that some payments need their eyes. Plain British English, no
/// superlatives, no advice — the actions do advice.
pub fn fallback_summary(report: &RunReport) -> String {
    let mut sentences: Vec<String> = Vec::new();
    let summary = &report.summary;

    if summary.recurring_count > 0 {
        sentences.push(format!(
            "Recurring payments come to {} a year, about {} a month, across {} {}.",
            fmt::money(summary.annualised_total),
            fmt::money(summary.monthly_equivalent),
            counted(summary.recurring_count),
            plural(summary.recurring_count, "payment", "payments"),
        ));
    }

    let risen: Vec<&RecurringFinding> = report
        .recurring
        .iter()
        .filter(|finding| finding.price_rise.is_some())
        .collect();
    if let Some(first) = risen.first() {
        let rise = first.price_rise.as_ref().expect("filtered on Some");
        if risen.len() == 1 {
            sentences.push(format!(
                "One price rise turned up: {} went from {} to {} in {}.",
                first.merchant,
                fmt::money(rise.from),
                fmt::money(rise.to),
                readable_month(&rise.month),
            ));
        } else {
            sentences.push(format!(
                "{} price rises turned up, including {}, which went from {} to {} in {}.",
                capitalised(counted(risen.len())),
                first.merchant,
                fmt::money(rise.from),
                fmt::money(rise.to),
                readable_month(&rise.month),
            ));
        }
    }

    if summary.needs_review_count > 0 {
        sentences.push(format!(
            "{} {} {} your review before {} counted.",
            capitalised(counted(summary.needs_review_count)),
            plural(summary.needs_review_count, "payment", "payments"),
            plural(summary.needs_review_count, "needs", "need"),
            plural(summary.needs_review_count, "it is", "they are"),
        ));
    }

    if sentences.is_empty() {
        return "No recurring payments turned up in this statement.".to_owned();
    }
    sentences.join(" ")
}

/// Refuse a page outside Kettle's supported, self-contained report subset.
///
/// `html5ever` supplies HTML tokenisation and entity decoding, so spelling
/// changes such as `src =`, `SrC` and `https&#58;` cannot evade the check.
/// CSS declarations and style sheets are tokenised separately.
pub fn assert_self_contained(html: &str) -> Result<(), RenderError> {
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));

    let tokenizer = Tokenizer::new(ReportTokenSink::default(), TokenizerOpts::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.finish()
}

/// A target is inside the file only when it is an in-document fragment.
/// Anything else — a host, path, scheme or embedded data — is a reach.
fn reject_if_external(target: &str) -> Result<(), RenderError> {
    let target = target.trim();
    if target.is_empty() || target.starts_with('#') {
        return Ok(());
    }
    Err(RenderError::ExternalAsset(target.to_owned()))
}

#[derive(Default)]
struct ReportTokenSink {
    violation: RefCell<Option<String>>,
    in_style: Cell<bool>,
    style_text: RefCell<String>,
}

impl ReportTokenSink {
    fn finish(&self) -> Result<(), RenderError> {
        if self.in_style.get() {
            self.check_style_text();
        }
        match self.violation.borrow().as_ref() {
            Some(detail) => Err(RenderError::ExternalAsset(detail.clone())),
            None => Ok(()),
        }
    }

    fn record(&self, detail: impl Into<String>) {
        let mut violation = self.violation.borrow_mut();
        if violation.is_none() {
            *violation = Some(detail.into());
        }
    }

    fn check_style_text(&self) {
        let css = self.style_text.replace(String::new());
        if let Err(detail) = assert_css_has_no_references(&css) {
            self.record(detail);
        }
    }

    fn inspect_tag(&self, tag: &Tag) {
        let element = tag.name.as_ref();

        if matches!(
            element,
            "script" | "iframe" | "object" | "embed" | "base" | "form"
        ) {
            self.record(format!("unsupported active <{element}> element"));
        }

        let is_refresh = element == "meta"
            && attribute(tag, "http-equiv")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("refresh"));
        if is_refresh {
            self.record(format!(
                "meta refresh: {}",
                attribute(tag, "content").unwrap_or("")
            ));
        }

        for attr in &tag.attrs {
            let name = attr.name.local.as_ref();
            let value = attr.value.as_ref();

            if name.starts_with("on") {
                self.record(format!("event handler attribute {name}"));
            } else if name == "style" {
                if let Err(detail) = assert_css_has_no_references(value) {
                    self.record(detail);
                }
            } else if name == "srcdoc" {
                if !value.trim().is_empty() {
                    self.record("embedded HTML in srcdoc");
                }
            } else if is_url_attribute(name) {
                if let Err(RenderError::ExternalAsset(target)) = reject_if_external(value) {
                    self.record(format!("{name}: {target}"));
                }
            }
        }
    }
}

impl TokenSink for ReportTokenSink {
    type Handle = ();

    fn process_token(&self, token: HtmlToken, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            HtmlToken::TagToken(tag)
                if tag.kind == TagKind::StartTag && tag.name.as_ref() == "style" =>
            {
                self.inspect_tag(&tag);
                self.style_text.borrow_mut().clear();
                self.in_style.set(true);
                return TokenSinkResult::RawData(RawKind::Rawtext);
            }
            HtmlToken::TagToken(tag)
                if tag.kind == TagKind::EndTag && tag.name.as_ref() == "style" =>
            {
                self.check_style_text();
                self.in_style.set(false);
            }
            HtmlToken::TagToken(tag) if tag.kind == TagKind::StartTag => {
                self.inspect_tag(&tag);
            }
            HtmlToken::CharacterTokens(text) if self.in_style.get() => {
                self.style_text.borrow_mut().push_str(text.as_ref());
            }
            HtmlToken::NullCharacterToken => {
                self.record("invalid null character in report HTML");
            }
            HtmlToken::ParseError(error) => {
                self.record(format!("invalid report HTML: {error}"));
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn attribute<'a>(tag: &'a Tag, wanted: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|attr| attr.name.local.as_ref() == wanted)
        .map(|attr| attr.value.as_ref())
}

fn is_url_attribute(name: &str) -> bool {
    matches!(
        name,
        "href"
            | "src"
            | "srcset"
            | "action"
            | "formaction"
            | "poster"
            | "data"
            | "cite"
            | "background"
            | "longdesc"
            | "usemap"
            | "manifest"
            | "ping"
            | "profile"
            | "codebase"
            | "archive"
            | "classid"
            | "icon"
    )
}

fn assert_css_has_no_references(css: &str) -> Result<(), String> {
    let mut input = CssParserInput::new(css);
    let mut parser = CssParser::new(&mut input);
    let mut reference = None;
    let parsed = inspect_css_tokens(&mut parser, &mut reference);

    if let Some(reference) = reference {
        return Err(reference);
    }
    if let Err(error) = parsed {
        return Err(format!("invalid CSS in report: {error:?}"));
    }
    Ok(())
}

fn inspect_css_tokens<'i, 't>(
    parser: &mut CssParser<'i, 't>,
    reference: &mut Option<String>,
) -> Result<(), CssParseError<'i, ()>> {
    while !parser.is_exhausted() {
        let token = parser.next_including_whitespace_and_comments()?.clone();
        match token {
            CssToken::AtKeyword(name) if name.eq_ignore_ascii_case("import") => {
                *reference = Some("CSS @import".to_owned());
                return Ok(());
            }
            CssToken::UnquotedUrl(target) => {
                *reference = Some(format!("CSS url(): {target}"));
                return Ok(());
            }
            CssToken::Function(name) if name.eq_ignore_ascii_case("url") => {
                *reference = Some("CSS url()".to_owned());
                return Ok(());
            }
            CssToken::BadUrl(target) => {
                *reference = Some(format!("invalid CSS url(): {target}"));
                return Ok(());
            }
            CssToken::Function(_)
            | CssToken::ParenthesisBlock
            | CssToken::SquareBracketBlock
            | CssToken::CurlyBracketBlock => {
                parser.parse_nested_block(|nested| inspect_css_tokens(nested, reference))?;
                if reference.is_some() {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn template_error(error: tera::Error) -> RenderError {
    let mut detail = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        detail.push_str(" — ");
        detail.push_str(&cause.to_string());
        source = cause.source();
    }
    RenderError::Template(detail)
}

// ---------------------------------------------------------------------------
// The view: the report as finished strings, ready to arrange.

#[derive(Serialize)]
struct ReportView {
    title: String,
    made_on: String,
    subtitle: String,
    prose: String,
    note: String,
    totals: TotalsView,
    recurring_heading: String,
    recurring: Vec<RecurringView>,
    regular_spend: Vec<RegularSpendView>,
    income: Vec<IncomeView>,
    needs_review_heading: String,
    needs_review: Vec<NeedsReviewView>,
    check_intro: String,
    check_yourself: Vec<CheckYourself>,
    footer: String,
}

#[derive(Serialize)]
struct TotalsView {
    annualised_total: String,
    monthly_equivalent: String,
    price_rises: String,
    price_rises_label: String,
    needs_review_count: String,
    needs_review_label: String,
}

#[derive(Serialize)]
struct ConfidenceView {
    /// "HIGH" — the pill's text.
    label: String,
    /// "high" — for prose that quotes it back.
    word: String,
    /// Tinted pill and a tick, rather than an outline and a wobble.
    settled: bool,
}

#[derive(Serialize)]
struct PaymentPill {
    label: String,
    /// The payment the price rise landed on.
    marked: bool,
}

#[derive(Serialize)]
struct RecurringView {
    merchant: String,
    raw_merchant: String,
    every: String,
    amount_current: String,
    annualised: String,
    confidence: ConfidenceView,
    /// Rows whose evidence changes a decision open on arrival.
    open: bool,
    /// "ROSE IN MAY".
    rise_tag: Option<String>,
    reason: String,
    price_rise: Option<String>,
    /// Why Kettle isn't certain, taken from the checklist entry about
    /// this merchant — the same words, where the reader already is.
    why_unsure: Option<String>,
    /// Every payment as a pill …
    payments: Vec<PaymentPill>,
    /// … unless they are all the same, when one line says it better.
    payments_line: Option<String>,
}

#[derive(Serialize)]
struct RegularSpendView {
    merchant: String,
    raw_merchant: String,
    visits: String,
    typical_visit: String,
    total: String,
    confidence: ConfidenceView,
}

#[derive(Serialize)]
struct IncomeView {
    merchant: String,
    raw_merchant: String,
    every: String,
    amount: String,
    annualised: String,
    confidence: ConfidenceView,
}

#[derive(Serialize)]
struct NeedsReviewView {
    raw_merchant: String,
    reason: String,
    note: String,
    payments: Vec<PaymentPill>,
}

impl ReportView {
    fn build(report: &RunReport, prose: String) -> ReportView {
        let run = &report.run;
        let summary = &report.summary;

        let mut recurring: Vec<&RecurringFinding> = report.recurring.iter().collect();
        // Sorted by what it costs over a year — the number the reader
        // came for. The sort is stable, so equal costs keep pack order.
        recurring.sort_by_key(|finding| std::cmp::Reverse(finding.annualised));

        // The evidence is what makes a figure checkable, so it is open
        // wherever it changes a decision: a price rise to notice, or a
        // confidence short of settled that the reader should judge for
        // themselves. Rows that are simply true and unremarkable stay
        // shut, so a long statement is still a table you can read
        // across rather than a wall.
        let recurring: Vec<RecurringView> = recurring
            .into_iter()
            .map(|finding| {
                let material =
                    finding.price_rise.is_some() || !matches!(finding.confidence, Confidence::High);
                RecurringView::build(finding, material, &report.check_yourself)
            })
            .collect();

        ReportView {
            title: format!("Subscriptions and regular spending in {}", run.input.file),
            made_on: timestamp_day(&run.finished),
            subtitle: format!(
                "{} payments, {} · read with the {} reading tool",
                run.input.rows,
                span(run.input.period.from, run.input.period.to),
                run.model.tier,
            ),
            prose,
            note: linked_note(&summary.note),
            totals: TotalsView {
                annualised_total: fmt::money(summary.annualised_total),
                monthly_equivalent: fmt::money(summary.monthly_equivalent),
                price_rises: summary.price_rises.to_string(),
                price_rises_label: format!(
                    "price {} found",
                    plural(summary.price_rises, "rise", "rises")
                ),
                needs_review_count: summary.needs_review_count.to_string(),
                needs_review_label: format!(
                    "{} need your review",
                    plural(summary.needs_review_count, "payment", "payments")
                ),
            },
            recurring_heading: format!(
                "Recurring payments — {} a year",
                fmt::money(summary.annualised_total)
            ),
            recurring,
            regular_spend: report
                .regular_spend
                .iter()
                .map(RegularSpendView::build)
                .collect(),
            income: report.income.iter().map(IncomeView::build).collect(),
            needs_review_heading: format!(
                "Needs your review — {} {}",
                summary.needs_review_count,
                plural(summary.needs_review_count, "payment", "payments")
            ),
            needs_review: report
                .needs_review
                .iter()
                .map(NeedsReviewView::build)
                .collect(),
            check_intro: check_intro(report.check_yourself.len()),
            check_yourself: report.check_yourself.clone(),
            footer: footer(report),
        }
    }
}

impl RecurringView {
    fn build(finding: &RecurringFinding, open: bool, checklist: &[CheckYourself]) -> RecurringView {
        let settled = matches!(finding.confidence, Confidence::High);
        let why_unsure = if settled {
            None
        } else {
            checklist
                .iter()
                .find(|entry| entry.about == finding.merchant)
                .map(|entry| entry.why.clone())
        };

        RecurringView {
            merchant: finding.merchant.clone(),
            raw_merchant: finding.raw_merchant.clone(),
            every: every(finding.period).to_owned(),
            amount_current: fmt::money(finding.amount_current),
            annualised: fmt::money(finding.annualised),
            confidence: ConfidenceView::build(finding.confidence),
            open,
            rise_tag: finding
                .price_rise
                .as_ref()
                .map(|rise| format!("ROSE IN {}", bare_month(&rise.month).to_uppercase())),
            reason: sentence(&finding.evidence.reason),
            price_rise: finding.price_rise.as_ref().map(price_rise_line),
            why_unsure,
            payments: pills(&finding.evidence.transactions, finding.price_rise.as_ref()),
            payments_line: payments_line(finding),
        }
    }
}

impl RegularSpendView {
    fn build(spend: &RegularSpend) -> RegularSpendView {
        RegularSpendView {
            merchant: spend.merchant.clone(),
            raw_merchant: spend.raw_merchant.clone(),
            // One payment is an observation, not a distribution (#219).
            // "Typical" needs something to be typical of, and a total
            // that equals the only amount states it twice — so a
            // single payment says its amount once and leaves the
            // total column empty.
            visits: if spend.visits == 1 {
                "1 payment".to_owned()
            } else {
                format!("{} visits", spend.visits)
            },
            typical_visit: if spend.visits == 1 {
                fmt::money(spend.typical_visit)
            } else {
                format!("{} typical", fmt::money(spend.typical_visit))
            },
            total: if spend.visits == 1 {
                "—".to_owned()
            } else {
                format!("{} total", fmt::money(spend.total))
            },
            confidence: ConfidenceView::build(spend.confidence),
        }
    }
}

impl IncomeView {
    fn build(income: &Income) -> IncomeView {
        IncomeView {
            merchant: income.merchant.clone(),
            raw_merchant: income.raw_merchant.clone(),
            every: every(income.period).to_owned(),
            amount: fmt::money(income.amount),
            annualised: fmt::money(crate::aggregate::annualise(income.amount, income.period)),
            confidence: ConfidenceView::build(income.confidence),
        }
    }
}

impl NeedsReviewView {
    fn build(item: &NeedsReviewItem) -> NeedsReviewView {
        NeedsReviewView {
            raw_merchant: item.raw_merchant.clone(),
            reason: item.reason.clone(),
            note: item.note.clone(),
            payments: pills(&item.transactions, None),
        }
    }
}

impl ConfidenceView {
    fn build(confidence: Confidence) -> ConfidenceView {
        let word = match confidence {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        };
        ConfidenceView {
            label: word.to_uppercase(),
            word: word.to_owned(),
            settled: matches!(confidence, Confidence::High),
        }
    }
}

// ---------------------------------------------------------------------------
// Small pieces of English and arithmetic, all in one place.

fn every(period: Period) -> &'static str {
    match period {
        Period::Weekly => "week",
        Period::Monthly => "month",
        Period::Quarterly => "quarter",
        Period::Annual => "year",
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

/// "one", "two" … up to ten, then digits — how a person would write it.
fn counted(count: usize) -> String {
    const WORDS: [&str; 11] = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    WORDS
        .get(count)
        .map(|word| (*word).to_owned())
        .unwrap_or_else(|| count.to_string())
}

fn capitalised(text: String) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => text,
    }
}

fn check_intro(count: usize) -> String {
    let things = if count == 1 {
        "this one thing".to_owned()
    } else {
        format!("these {} things", counted(count))
    };
    format!("A careful reader would double-check {things}. Kettle can't, so it's telling you.")
}

/// Evidence arrives as a clause; it reads as a sentence here.
fn sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.ends_with(['.', '!', '?']) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.")
    }
}

/// "January" — the month on its own, without the year `fmt::month` adds.
fn month_name(day: NaiveDate) -> String {
    fmt::month(day)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// "Jan" — for a payment pill, where space is short.
fn short_month(day: NaiveDate) -> String {
    month_name(day).chars().take(3).collect()
}

/// "2025-05" as a person reads it: "May 2025". Left alone if it isn't
/// the shape we expect — a wrong-looking string is better than a lie.
fn readable_month(wire: &str) -> String {
    match first_of_month(wire) {
        Some(day) => fmt::month(day),
        None => wire.to_owned(),
    }
}

/// "2025-05" as "May" — the tag has no room for the year, and the row
/// it sits on is already inside the statement's window.
fn bare_month(wire: &str) -> String {
    match first_of_month(wire) {
        Some(day) => month_name(day),
        None => wire.to_owned(),
    }
}

fn first_of_month(wire: &str) -> Option<NaiveDate> {
    let (year, month) = wire.split_once('-')?;
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)
}

/// "3 January to 28 December 2025" — the year said once when it can be.
fn span(from: NaiveDate, to: NaiveDate) -> String {
    if from.year() == to.year() {
        format!("{} {} to {}", from.day(), month_name(from), fmt::date(to))
    } else {
        format!("{} to {}", fmt::date(from), fmt::date(to))
    }
}

fn price_rise_line(rise: &PriceRiseOut) -> String {
    format!(
        "{} → {} from {} — an extra {} a year.",
        fmt::money(rise.from),
        fmt::money(rise.to),
        readable_month(&rise.month),
        fmt::money(rise.extra_per_year),
    )
}

fn pills(transactions: &[TransactionOut], rise: Option<&PriceRiseOut>) -> Vec<PaymentPill> {
    // The payment the rise landed on: the first at the new amount in the
    // month the rise is dated.
    let marked_at = rise.and_then(|rise| {
        transactions.iter().position(|transaction| {
            transaction.amount == rise.to && fmt::iso_month(transaction.date) == rise.month
        })
    });

    transactions
        .iter()
        .enumerate()
        .map(|(index, transaction)| PaymentPill {
            label: format!(
                "{} {} {}",
                transaction.date.day(),
                short_month(transaction.date),
                fmt::money(transaction.amount)
            ),
            marked: Some(index) == marked_at,
        })
        .collect()
}

/// A steady monthly payment doesn't need twelve pills saying the same
/// thing; one sentence carries it. Anything that varies gets the pills.
fn payments_line(finding: &RecurringFinding) -> Option<String> {
    let transactions = &finding.evidence.transactions;
    if finding.period != Period::Monthly || transactions.len() < 3 {
        return None;
    }
    let first = transactions.first()?;
    let last = transactions.last()?;
    if transactions
        .iter()
        .any(|transaction| transaction.amount != first.amount)
    {
        return None;
    }

    let months = if first.date.year() == last.date.year() {
        format!(
            "{} to {} {}",
            month_name(first.date),
            month_name(last.date),
            last.date.year()
        )
    } else {
        format!("{} to {}", fmt::month(first.date), fmt::month(last.date))
    };

    Some(format!(
        "{} on the {} of every month, {}.",
        fmt::money(first.amount),
        fmt::ordinal(first.date.day()),
        months
    ))
}

/// The run's own line: what was read, from where, and when it finished.
fn footer(report: &RunReport) -> String {
    let run = &report.run;
    format!(
        "{} · {} · {} · finished {} · made with Kettle · kttl.app",
        run.input.file,
        shortened_hash(&run.input.hash),
        run.id,
        timestamp_minute(&run.finished),
    )
}

/// "blake3:9f4c2a71…b9d1f" — enough to compare, short enough to read.
fn shortened_hash(hash: &str) -> String {
    let Some((algorithm, digest)) = hash.split_once(':') else {
        return hash.to_owned();
    };
    if digest.len() <= 13 {
        return hash.to_owned();
    }
    format!(
        "{algorithm}:{}…{}",
        &digest[..8],
        &digest[digest.len() - 5..]
    )
}

/// "19 July 2026" from an RFC 3339 stamp; the stamp itself if it isn't
/// one.
fn timestamp_day(stamp: &str) -> String {
    match timestamp_parts(stamp) {
        Some((day, _)) => fmt::date(day),
        None => stamp.to_owned(),
    }
}

/// "19 July 2026, 14:06".
fn timestamp_minute(stamp: &str) -> String {
    match timestamp_parts(stamp) {
        Some((day, Some(time))) => format!("{}, {}", fmt::date(day), time),
        Some((day, None)) => fmt::date(day),
        None => stamp.to_owned(),
    }
}

fn timestamp_parts(stamp: &str) -> Option<(NaiveDate, Option<String>)> {
    let (date, rest) = stamp.split_once('T').unwrap_or((stamp, ""));
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let time = rest
        .get(..5)
        .filter(|clock| clock.len() == 5 && clock.as_bytes()[2] == b':')
        .map(str::to_owned);
    Some((day, time))
}

/// The summary's note points at a section of the report, so it points
/// at it properly. Escaped here because it arrives as HTML.
fn linked_note(note: &str) -> String {
    const QUOTED: &str = "'Needs your review'";
    match note.split_once(QUOTED) {
        Some((before, after)) => format!(
            "{}<a href=\"#needs-review\">Needs your review</a>{}",
            escape(before),
            escape(after)
        ),
        None => escape(note),
    }
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Render a letter run's report (#243).
///
/// A separate entry point from [`render_report`] for the same reason
/// the document is separate: the two typologies hand a template
/// different facts, and a shared context would let a letter template
/// reach for `annualised_total` and quietly render nothing.
///
/// The self-contained check is the same one, unchanged — inline CSS,
/// no external assets, printable offline, whatever the pack.
pub fn render_letter_report(
    template: &str,
    report: &crate::results::LetterReport,
) -> Result<String, RenderError> {
    let mut context = Context::new();
    context.insert("run", &report.run);
    context.insert("summary", &report.summary);
    context.insert("obligations", &report.obligations);
    context.insert("needs_review", &report.needs_review);

    let html = Tera::one_off(template, &context, true).map_err(template_error)?;
    assert_self_contained(&html)?;
    Ok(html)
}

/// Render a comparison run's report (#66).
///
/// A third entry point, for the reason there is a second: each
/// typology hands a template different facts, and a shared context
/// would let one pack's template reach for another's and quietly
/// render nothing.
pub fn render_comparison_report(
    template: &str,
    report: &crate::results::ComparisonReport,
) -> Result<String, RenderError> {
    let mut context = Context::new();
    context.insert("run", &report.run);
    context.insert("summary", &report.summary);
    context.insert("changes", &report.changes);
    context.insert("needs_review", &report.needs_review);

    let html = Tera::one_off(template, &context, true).map_err(template_error)?;
    assert_self_contained(&html)?;
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fragment_is_inside_the_file() {
        assert!(reject_if_external("#needs-review").is_ok());
        assert!(reject_if_external("").is_ok());
        assert!(reject_if_external("data:image/svg+xml,%3Csvg%2F%3E").is_err());
    }

    #[test]
    fn a_host_is_a_reach_however_it_is_spelled() {
        for reach in [
            "https://fonts.example/x.css",
            "http://cdn.example/a.png",
            "//cdn.example/a.png",
            "logo.png",
        ] {
            assert!(matches!(
                reject_if_external(reach),
                Err(RenderError::ExternalAsset(_))
            ));
        }
    }

    #[test]
    fn a_bare_attribute_value_is_still_read() {
        let html = "<img src=logo.png alt=x>";
        assert!(assert_self_contained(html).is_err());
    }

    #[test]
    fn a_hash_shortens_to_both_ends() {
        assert_eq!(
            shortened_hash(
                "blake3:9f4c2a71e8b06d3f5c1a9e2b7d4f8a0c6e3b5d7f9a1c3e5b7d9f1a3c5e7b9d1f"
            ),
            "blake3:9f4c2a71…b9d1f"
        );
        assert_eq!(shortened_hash("short"), "short");
    }

    #[test]
    fn a_run_stamp_reads_as_a_day_and_a_time() {
        assert_eq!(timestamp_day("2026-07-19T14:06:38Z"), "19 July 2026");
        assert_eq!(
            timestamp_minute("2026-07-19T14:06:38Z"),
            "19 July 2026, 14:06"
        );
        assert_eq!(timestamp_minute("not a stamp"), "not a stamp");
    }

    #[test]
    fn the_note_links_the_section_it_names() {
        let linked = linked_note("Two payments need your eyes — see 'Needs your review'.");
        assert!(linked.contains("<a href=\"#needs-review\">Needs your review</a>"));
    }

    #[test]
    fn a_merchant_cannot_smuggle_markup_into_the_page() {
        assert_eq!(
            escape("<b>&'\"</b>"),
            "&lt;b&gt;&amp;&#x27;&quot;&lt;/b&gt;"
        );
    }
}
