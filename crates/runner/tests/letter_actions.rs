//! #243: an obligation becomes something a person can put in their
//! diary, and a report they can check it against.
//!
//! The read-only rule holds exactly as it does for the audit: Kettle
//! never writes to a calendar. It proposes, exports .ics or copyable
//! text, and a person decides (CLAUDE.md).

use chrono::NaiveDate;
use runner::actions::propose_letter_actions;
use runner::claim::Kind;
use runner::document::Segment;
use runner::letter_report::build_letter_report;
use runner::ocr::Disagreement;
use runner::results::{ActionKind, LetterRunInfo, STATUS_PROPOSED};
use runner::run::{ExtractionOutcome, Obligation};
use runner::timeline::Resolved;
use std::str::FromStr;

fn date(iso: &str) -> NaiveDate {
    NaiveDate::from_str(iso).expect("test date")
}

fn segment(ordinal: usize, text: &str) -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal,
        text: text.to_owned(),
        rows: Vec::new(),
    }
}

/// Two obligations from one invented letter: a dated payment and an
/// undated request.
fn outcome() -> ExtractionOutcome {
    ExtractionOutcome {
        date_disputes: vec![],
        obligations: vec![
            Obligation {
                kind: "payment".to_owned(),
                party: runner::reading::Reading::new(1, "Harborne Parking Services".to_owned()),
                ask: "Pay £120.00".to_owned(),
                deadline: runner::reading::Reading::new(1, "within 14 days".to_owned()),
                anchor: "the date of this letter".to_owned(),
                amount: runner::reading::Reading::absent(1),
                refused: Vec::new(),
                confidence: "high".to_owned(),
                // Counted from the letter's date, not written on it.
                due: Some(Resolved {
                    date: date("2026-03-17"),
                    kind: Kind::WorkedOut,
                }),
                evidence: vec![segment(
                    1,
                    "Please pay £120.00 within 14 days of the date of this letter.",
                )],
                dated_by: None,
                priced_by: None,
                shown: Default::default(),
                disputed: vec![],
            },
            Obligation {
                kind: "response".to_owned(),
                party: runner::reading::Reading::new(3, "Harborne Parking Services".to_owned()),
                ask: "Send a meter reading".to_owned(),
                deadline: runner::reading::Reading::new(
                    3,
                    "at your earliest convenience".to_owned(),
                ),
                anchor: "no particular date".to_owned(),
                amount: runner::reading::Reading::absent(3),
                refused: Vec::new(),
                confidence: "high".to_owned(),
                due: None,
                evidence: vec![segment(
                    3,
                    "Please send a reading at your earliest convenience.",
                )],
                dated_by: None,
                priced_by: None,
                shown: Default::default(),
                disputed: vec![],
            },
        ],
    }
}

fn run_info() -> LetterRunInfo {
    LetterRunInfo {
        id: "letter-01".to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        pack_version: "0.1.0".to_owned(),
        file: "letter-01.txt".to_owned(),
        passages: 5,
        started: "2026-03-03T09:00:00Z".to_owned(),
        finished: "2026-03-03T09:00:12Z".to_owned(),
    }
}

#[test]
fn a_dated_obligation_becomes_one_approvable_calendar_action() {
    let actions = propose_letter_actions(&outcome(), "run-01");

    // One card per obligation — including the undated one, which a
    // person still has to deal with.
    assert_eq!(actions.actions.len(), 2, "{actions:#?}");

    let pay = &actions.actions[0];
    assert_eq!(pay.kind, ActionKind::CalendarReminder);
    assert_eq!(pay.status, STATUS_PROPOSED, "the runner only ever proposes");
    assert!(
        pay.title.contains("Pay £120.00"),
        "the card says what to do: {}",
        pay.title
    );
    assert!(
        pay.detail.contains("Harborne Parking Services"),
        "and who asked: {}",
        pay.detail
    );
    // The evidence a person checks it against: the passage itself.
    assert!(
        pay.evidence
            .values()
            .any(|value| value.contains("within 14 days")),
        "{:?}",
        pay.evidence
    );

    // The .ics sits on the day it falls due, not on today.
    let ics = pay
        .export
        .ics
        .as_ref()
        .expect("a dated obligation exports to a calendar");
    assert_eq!(ics.date, date("2026-03-17"));
    assert!(!ics.summary.is_empty());
}

#[test]
fn a_disputed_deadline_reaches_its_approvable_action() {
    let mut disputed = outcome();
    disputed.obligations[0].disputed = vec![Disagreement {
        page: 1,
        top: 0.60,
        read: "Please pay £120.00 by 28 April 2026.".to_owned(),
        also_read: "Please pay £120.00 by 28 April 2028.".to_owned(),
    }];

    let actions = propose_letter_actions(&disputed, "run-01");
    let shown = &actions.actions[0].disputed;

    assert_eq!(
        shown.len(),
        1,
        "the action carries the runner-owned dispute"
    );
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/contracts/disputed-out.json"
    ))
    .expect("the shared wire fixture");
    assert_eq!(
        serde_json::to_value(&shown[0]).expect("the dispute serializes"),
        expected,
        "Rust and TypeScript share one DisputedOut contract"
    );
}

#[test]
fn a_line_the_second_reading_missed_says_that_it_was_not_confirmed() {
    let mut disputed = outcome();
    disputed.obligations[0].disputed = vec![Disagreement {
        page: 1,
        top: 0.60,
        read: "Please pay £120.00 by 28 April 2026.".to_owned(),
        also_read: String::new(),
    }];

    let actions = propose_letter_actions(&disputed, "run-01");
    let shown = &actions.actions[0].disputed[0];

    assert_eq!(
        shown.message,
        "Reading the page a second time did not find this line at all."
    );
    assert_eq!(
        shown.instruction,
        "Nothing confirmed it, so check it against the letter before you rely on the date."
    );
}

#[test]
fn an_undated_obligation_is_offered_without_a_date_being_invented() {
    let actions = propose_letter_actions(&outcome(), "run-01");
    let reading = &actions.actions[1];

    // It is visible — never dropped for being awkward.
    assert!(reading.title.contains("Send a meter reading"));
    // And it carries the letter's own words rather than a date nobody
    // wrote: today's date here would be an invented deadline.
    assert!(
        reading.detail.contains("at your earliest convenience"),
        "{}",
        reading.detail
    );
    assert!(
        !reading.detail.contains("2026-03-03"),
        "an undated obligation must not be dated by default: {}",
        reading.detail
    );
    // Nor may the export invent one. A calendar event needs a day and
    // the letter gave none, so there is no calendar event — only text.
    assert!(
        reading.export.ics.is_none(),
        "an undated obligation has no calendar export: {:?}",
        reading.export
    );
    assert!(reading.export.text.contains("at your earliest convenience"));
    assert!(
        actions.actions[0].export.ics.is_some(),
        "the dated one still exports to a calendar"
    );
}

#[test]
fn the_letter_report_says_what_needs_doing_and_never_mentions_subscriptions() {
    let report = build_letter_report(&outcome(), run_info());
    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.letter-to-actions/report.html.tera"),
    )
    .expect("the letter pack's template");

    let html = runner::render::render_letter_report(&template, &report)
        .expect("the letter report renders");

    // Self-contained, exactly as the audit's is: inline CSS, no
    // external assets, printable offline (CLAUDE.md).
    runner::render::assert_self_contained(&html).expect("no external references");

    assert!(html.contains("Pay £120.00"), "the ask reaches the page");
    assert!(html.contains("Harborne Parking Services"));
    // The undated one is shown as something to check, not as a deadline.
    assert!(html.contains("at your earliest convenience"));

    // Vocabulary is judged on what a person reads, so the inlined
    // stylesheet is excluded: `.k-merchant` is a class name nobody
    // sees, and the styles are shared on purpose so a letter report
    // looks like the same Kettle (CLAUDE.md, one source). Dead audit
    // rules riding along are worth tidying, but they are not the
    // report speaking the wrong language.
    let body = html
        .split_once("</style>")
        .map(|(_, rest)| rest)
        .unwrap_or(&html);
    for word in ["subscription", "Subscription", "merchant", "annualised"] {
        assert!(
            !body.contains(word),
            "{word:?} is the audit's vocabulary and has no place in a letter report"
        );
    }
}

/// The report says how each date was arrived at (#366, #367).
///
/// Two dates sit in the same column: "17 March 2026", which Rust
/// counted from the letter's own date, and one a letter wrote out in
/// full. Rendered identically they make the same promise, and only one
/// of them can be checked against the page. The kind is derived in the
/// runner (`timeline::Resolved`), so the template's only job is to show
/// it — a template that decided this for itself is the copy-layer claim
/// #367 refuses.
#[test]
fn a_rendered_date_says_whether_it_was_read_or_worked_out() {
    let mut outcome = outcome();
    outcome.obligations[1].deadline.value = "by 12 August 2026".to_owned();
    outcome.obligations[1].due = Some(Resolved {
        date: date("2026-08-12"),
        kind: Kind::ReadAndVerified,
    });

    let report = build_letter_report(&outcome, run_info());
    assert_eq!(
        report.obligations[0].due.as_ref().map(|due| due.kind),
        Some(Kind::WorkedOut),
        "the report document carries the kind, so nothing downstream re-guesses it"
    );

    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.letter-to-actions/report.html.tera"),
    )
    .expect("the letter pack's template");
    let html = runner::render::render_letter_report(&template, &report)
        .expect("the letter report renders");
    let body = html
        .split_once("</style>")
        .map(|(_, rest)| rest)
        .unwrap_or(&html);

    assert!(
        body.contains("Worked out"),
        "a counted date is marked as arithmetic: {body}"
    );
    assert!(
        body.contains("Read from the letter"),
        "a date the letter wrote out is marked as read: {body}"
    );
}

/// One run states one deadline one way (#405). The action card already
/// says "17 March 2026" through `fmt::date`; the report showed the
/// serialised wire form beside it, so the same deadline read two
/// different ways in the same run.
#[test]
fn the_report_states_a_deadline_the_way_the_action_card_does() {
    let report = build_letter_report(&outcome(), run_info());
    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.letter-to-actions/report.html.tera"),
    )
    .expect("the letter pack's template");
    let html = runner::render::render_letter_report(&template, &report)
        .expect("the letter report renders");
    let body = html
        .split_once("</style>")
        .map(|(_, rest)| rest)
        .unwrap_or(&html);

    assert!(
        body.contains("17 March 2026"),
        "the report reads the deadline the way the action card does: {body}"
    );
    assert!(
        !body.contains("2026-03-17"),
        "the wire form is not how a person reads a date: {body}"
    );
}

#[test]
fn the_actions_keep_the_order_the_timeline_decided() {
    // Cards follow the payload's order rather than re-deriving it.
    // builtin:timeline-sort (#241) already put these soonest-first
    // with undated last, and a second sort here could disagree with
    // it — two orderings of the same list is how a screen and a
    // report end up telling a person different things.
    let actions = propose_letter_actions(&outcome(), "run-01");
    let titles: Vec<&str> = actions.actions.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Pay £120.00", "Send a meter reading"]);

    let mut reversed = outcome();
    reversed.obligations.reverse();
    let reversed_actions = propose_letter_actions(&reversed, "run-01");
    let reversed_titles: Vec<&str> = reversed_actions
        .actions
        .iter()
        .map(|a| a.title.as_str())
        .collect();
    assert_eq!(
        reversed_titles,
        vec!["Send a meter reading", "Pay £120.00"],
        "the payload's order is the one a person sees"
    );
}

#[test]
fn a_letter_report_is_a_document_that_says_what_it_is() {
    // The schema field is how any reader tells the two typologies
    // apart without guessing from a filename or a pack id. The CLI
    // renderer dispatches on it; so can the app, and so can anyone
    // reading a run directory later.
    let report = build_letter_report(&outcome(), run_info());
    assert_eq!(report.schema, runner::results::LETTER_REPORT_SCHEMA);

    let json = serde_json::to_string(&report).expect("a report is plain data");
    let round_tripped: runner::results::LetterReport =
        serde_json::from_str(&json).expect("it reads back");
    assert_eq!(round_tripped, report);

    // The counts a person is shown, and the sentence derived from them.
    assert_eq!(report.summary.obligations_count, 2);
    assert_eq!(report.summary.dated_count, 1);
    assert_eq!(report.summary.undated_count, 1);
    assert!(
        report
            .summary
            .note
            .contains("no date Kettle could work out"),
        "the summary is honest about the undated one: {}",
        report.summary.note
    );
}

#[test]
fn proposed_letter_actions_carry_the_run_they_belong_to() {
    // #389: the letter emitter wrote `run_id: ""` while the audit's
    // clones it from its report — and restart hydration, which rightly
    // refuses an actions document whose run_id disagrees with the
    // marker, dropped every completed letter run because of it. An
    // actions document that cannot say which run proposed it is not a
    // complete document.
    let actions = propose_letter_actions(&outcome(), "run-07");
    assert_eq!(actions.run_id, "run-07");
}

/// A deadline the letter never wrote is not shown as the letter's words.
///
/// Found on real post, 3 September 2026. The report showed
/// *Not stated — the letter says "no particular date"* for an ask that
/// carried no deadline, and the letter says no such thing:
/// `prompts/obligations.md` tells the model to **write** "no particular
/// date" when no *anchor* is given, and says nothing at all about what
/// `deadline` should be when the letter states none. So the model
/// borrowed the neighbouring field's sentinel, and
/// `report.html.tera` — whose slot is unconditional — attributed it to
/// the page.
///
/// The rule is #460's, applied one field further along: what the report
/// puts in the letter's mouth has to be in the letter. `ObligationOut`
/// documents `deadline` as "the letter's own words for when", so a
/// phrase that appears in none of the passages the claim rests on is
/// not that, whatever it says. Blanked here rather than repaired,
/// because the runner cannot know which words the letter would have
/// used — and "Not stated" alone is true.
///
/// This matters more than its size. The page's promise is that
/// everything on it quotes the letter so a person can check it, and a
/// reader who goes looking for this phrase will not find it.
#[test]
fn a_deadline_the_letter_never_wrote_is_not_shown_as_its_words() {
    let mut outcome = outcome();
    let ask = &mut outcome.obligations[1];
    // Exactly what the real letter produced: the anchor's sentinel in
    // the deadline field, on a passage that contains neither.
    ask.deadline.value = "no particular date".to_owned();
    ask.evidence = vec![segment(
        1,
        "If you have recently paid, please complete our form.",
    )];

    let report = build_letter_report(&outcome, run_info());
    let shown = &report.obligations[1];

    assert!(
        shown.deadline.is_empty(),
        "a phrase in none of the claim's passages is not the letter's \
         words, so the report must not offer it as a quotation: {:?}",
        shown.deadline,
    );
    assert!(
        shown.due.is_none(),
        "and nothing was resolved from it either",
    );
}

/// The other side of the same rule: a deadline the letter *did* write
/// survives untouched. A guard that blanked real phrases would be worse
/// than the defect it fixes.
#[test]
fn a_deadline_the_letter_wrote_is_kept_exactly() {
    let report = build_letter_report(&outcome(), run_info());

    assert_eq!(
        report.obligations[0].deadline, "within 14 days",
        "the phrase is in the passage, so it is the letter's own words"
    );
    assert_eq!(
        report.obligations[1].deadline, "at your earliest convenience",
        "so is this one, and it resolves to no date — which is not the \
         same as the letter saying nothing"
    );
}

/// #612: the sum reaches the report only in the letter's own words.
/// A figure the passage prints is shown; the sentinel is not a figure;
/// and a figure the model copied from somewhere the claim's own
/// passages do not contain is blanked rather than repaired — #460's
/// rule one, on the field the photo route is likeliest to misread
/// (OCR turns `£` into `€` and drops it).
#[test]
fn the_sum_is_reported_only_in_the_letters_own_words() {
    let with = |amount: &str| {
        let mut outcome = outcome();
        outcome.obligations.truncate(1);
        outcome.obligations[0].amount.value = amount.to_owned();
        build_letter_report(&outcome, run_info()).obligations[0]
            .amount
            .clone()
    };
    assert_eq!(with("£120.00"), "£120.00");
    assert_eq!(with("no amount"), "");
    assert_eq!(with("£12.00"), "", "a figure the passage does not print");
    assert_eq!(
        with("€120.00"),
        "",
        "the reader's currency sign, not the page's"
    );
}

/// #612, second half: a sum read off the letter's own row reaches the
/// report with the row beside it, and passes #460's rule one against
/// that row rather than against the ask's passage.
#[test]
fn a_sum_read_off_a_row_is_reported_with_the_row() {
    let mut outcome = outcome();
    outcome.obligations.truncate(1);
    let row = segment(9, "Amount Due 41.21 GBP 009422");
    outcome.obligations[0].amount.value = "41.21 GBP".to_owned();
    outcome.obligations[0].priced_by = Some(row);
    let report = build_letter_report(&outcome, run_info());
    assert_eq!(report.obligations[0].amount, "41.21 GBP");
    assert_eq!(
        report.obligations[0]
            .priced_by
            .as_ref()
            .map(|p| p.text.as_str()),
        Some("Amount Due 41.21 GBP 009422")
    );
}

/// Two invoices to one payee by one date reach the report as two rows
/// and the diary as two reminders, each with its own sum (review of
/// #626, Task 2). The old four-string merge folded them into one and
/// kept the first sum.
#[test]
fn two_invoices_by_one_date_are_two_rows_and_two_reminders() {
    let invoice = |ordinal: usize, ask: &str, amount: &str| Obligation {
        kind: "payment".to_owned(),
        party: runner::reading::Reading::new(ordinal, "Harborne Parking Services".to_owned()),
        ask: ask.to_owned(),
        deadline: runner::reading::Reading::new(ordinal, "by 30 April 2026".to_owned()),
        anchor: "30 April 2026".to_owned(),
        amount: runner::reading::Reading::new(ordinal, amount.to_owned()),
        refused: Vec::new(),
        confidence: "high".to_owned(),
        due: None,
        evidence: vec![segment(
            ordinal,
            &format!("{ask}: {amount} by 30 April 2026."),
        )],
        dated_by: None,
        priced_by: None,
        shown: Default::default(),
        disputed: vec![],
    };
    let segments: Vec<Segment> = vec![
        segment(0, "3 March 2026"),
        segment(1, "Pay invoice A: £80.00 by 30 April 2026."),
        segment(2, "Pay invoice B: £120.00 by 30 April 2026."),
    ];
    let obligations = runner::timeline::sort_timeline(
        vec![
            invoice(1, "Pay invoice A", "£80.00"),
            invoice(2, "Pay invoice B", "£120.00"),
        ],
        &segments,
    );
    let outcome = ExtractionOutcome {
        date_disputes: vec![],
        obligations,
    };

    let report = build_letter_report(&outcome, run_info());
    let amounts: Vec<&str> = report
        .obligations
        .iter()
        .map(|row| row.amount.as_str())
        .collect();
    assert_eq!(amounts, vec!["£80.00", "£120.00"], "{report:#?}");

    let actions = propose_letter_actions(&outcome, "run-01");
    assert_eq!(actions.actions.len(), 2, "{actions:#?}");
    let titles: Vec<&str> = actions
        .actions
        .iter()
        .map(|action| action.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Pay invoice A", "Pay invoice B"]);
}
