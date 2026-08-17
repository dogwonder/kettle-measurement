//! #66: the Comparison typology's report document.
//!
//! The pack has been measurable and unreadable since #350 — `outputs`
//! was empty because nothing rendered a comparison. `build_report` is
//! the Audit typology's and `letter_report` is the Extraction
//! typology's, and a third question needs its own document for the
//! reason the first two do: a shared shape with half its fields empty
//! makes every reader guess which half meant anything (#238).
//!
//! Nothing here reads. `terms::diff_terms` has already paired on the
//! closed enum and done every subtraction (#350); this arranges what it
//! decided, and the template reformats nothing.

use runner::claim::Kind;
use runner::comparison_report::build_comparison_report;
use runner::results::{
    ChangeState, ComparedDocument, ComparisonReport, ComparisonRunInfo, Direction, TermChangeOut,
    TermSideOut,
};
use runner::run::ComparisonOutcome;
use runner::terms::{diff_terms, Term, TermFamilies};

fn term(document: usize, name: &str, basis: &str, value: &str) -> Term {
    Term {
        term: name.to_owned(),
        basis: basis.to_owned(),
        value: value.to_owned(),
        quote: format!("The {} is {value}.", name.replace('_', " ")),
        segment: format!("The {} is {value}.", name.replace('_', " ")),
        document,
        confidence: "high".to_owned(),
    }
}

/// One renewal's worth of movement: a premium that rose, an excess that
/// did not move, a phrase that changed, a term only the renewal names
/// and one only last year's policy did.
fn outcome() -> ComparisonOutcome {
    let terms = vec![
        term(0, "premium", "annual", "£500.00"),
        term(1, "premium", "annual", "£565.50"),
        term(0, "compulsory_excess", "per_claim", "£250.00"),
        term(1, "compulsory_excess", "per_claim", "£250.00"),
        term(0, "cooling_off_period", "per_policy", "14 days"),
        term(1, "cooling_off_period", "per_policy", "21 days"),
        term(1, "no_claims_discount", "annual", "£60.00"),
        term(0, "cover_limit", "per_policy", "£1,000,000.00"),
    ];
    ComparisonOutcome {
        diff: diff_terms(&terms, 0, 1, &TermFamilies::default()).rows,
        not_compared: Vec::new(),
        terms,
    }
}

fn run() -> ComparisonRunInfo {
    ComparisonRunInfo {
        id: "renewal-01".to_owned(),
        pack: "app.kttl.renewal-diff".to_owned(),
        pack_version: "0.1.0".to_owned(),
        documents: vec![
            ComparedDocument {
                role: "previous".to_owned(),
                label: "Last year's policy".to_owned(),
                file: "policy-2025.pdf".to_owned(),
            },
            ComparedDocument {
                role: "renewal".to_owned(),
                label: "This year's renewal".to_owned(),
                file: "renewal-2026.pdf".to_owned(),
            },
        ],
        passages: 24,
        started: "2026-08-04T09:00:00Z".to_owned(),
        finished: "2026-08-04T09:01:40Z".to_owned(),
    }
}

/// Rows are looked up by term, never by position. Within a group the
/// diff's own alphabetical order survives, and deliberately: ranking
/// changed rows by how much money moved would be Kettle deciding which
/// of a person's terms matters most, which is not a claim it can check.
fn row<'a>(report: &'a ComparisonReport, term: &str) -> &'a TermChangeOut {
    report
        .changes
        .iter()
        .find(|row| row.term == term)
        .unwrap_or_else(|| panic!("no row for {term}: {:#?}", report.changes))
}

/// What moved comes first, and a row says how it can be wrong.
///
/// `diff_terms` orders by term name, which is the order that makes two
/// runs of the same documents comparable — not the order a person came
/// for. They came to find what changed, so the document decides that
/// once, here, rather than leaving a screen and a report to each pick
/// their own and disagree (#241's ordering rule, applied to a diff).
#[test]
fn a_comparison_report_leads_with_what_moved() {
    let report = build_comparison_report(&outcome(), &[], run());

    let states: Vec<ChangeState> = report.changes.iter().map(|row| row.state).collect();
    assert_eq!(
        states,
        vec![
            ChangeState::Changed,
            ChangeState::Changed,
            ChangeState::Added,
            ChangeState::Removed,
            ChangeState::Unchanged,
        ],
        "changed, then added, then removed, then what stayed put: {:#?}",
        report.changes
    );

    assert_eq!(report.summary.changed_count, 2);
    assert_eq!(report.summary.added_count, 1);
    assert_eq!(report.summary.removed_count, 1);
    assert_eq!(report.summary.unchanged_count, 1);
    assert_eq!(report.summary.terms_count, 5);
}

/// A row is readable without the schema that produced it.
///
/// `compulsory_excess` is the pack's vocabulary, not a person's, and a
/// report that printed the enum would be asking them to learn it. The
/// label is derived from the term rather than declared beside it, for
/// #367's reason: a declared label can disagree with the value it names.
#[test]
fn a_row_names_its_term_in_words_and_says_what_it_is_measured_against() {
    let report = build_comparison_report(&outcome(), &[], run());

    let premium = row(&report, "premium");
    assert_eq!(premium.term, "premium");
    assert_eq!(premium.label, "Premium");
    assert_eq!(premium.basis_label, "a year");

    let excess = row(&report, "compulsory_excess");
    assert_eq!(excess.label, "Compulsory excess");
    assert_eq!(excess.basis_label, "per claim");
}

/// The money is worked out; the phrase is read. Both are shown as such.
///
/// A premium rising by £65.50 is Rust's subtraction and can only be
/// checked against its arithmetic. A cooling-off period going from "14
/// days" to "21 days" is two passages with nothing computed, and #366
/// refuses to render them in one voice. The size and the direction are
/// separate because "£65.50 more" is what a person reads — the template
/// reformats nothing (its own rule).
#[test]
fn a_moved_amount_carries_its_size_its_direction_and_the_kind_of_claim_it_is() {
    let report = build_comparison_report(&outcome(), &[], run());

    let premium = row(&report, "premium");
    assert_eq!(premium.from.as_deref(), Some("£500.00"));
    assert_eq!(premium.to.as_deref(), Some("£565.50"));
    assert_eq!(premium.delta.as_deref(), Some("£65.50"));
    assert_eq!(premium.direction, Some(Direction::Up));
    assert_eq!(premium.kind, Kind::WorkedOut);

    let cooling_off = row(&report, "cooling_off_period");
    assert_eq!(cooling_off.from.as_deref(), Some("14 days"));
    assert_eq!(cooling_off.to.as_deref(), Some("21 days"));
    assert_eq!(
        cooling_off.delta, None,
        "no number was computed, so none is shown"
    );
    assert_eq!(cooling_off.direction, None);
    assert_eq!(cooling_off.kind, Kind::ReadAndVerified);
}

/// Every row carries the passages behind it, because the pack's whole
/// claim is that each one can be checked locally (its README).
///
/// Stated side by side rather than as a count (#379): a row with two
/// passages and one value would satisfy "has evidence" while leaving a
/// value unevidenced, which is the shape #460 refuses. So the property
/// is per side — a value on the page has words behind it, and a side
/// with nothing to say quotes nothing.
#[test]
fn every_row_carries_the_words_it_came_from() {
    let report = build_comparison_report(&outcome(), &[], run());
    for row in &report.changes {
        for side in &row.sides {
            assert_eq!(
                side.value.is_some(),
                side.quote.is_some(),
                "a value with no passage, or a passage with no value: {side:#?} in {row:#?}"
            );
        }
    }
}

/// A renewal where nothing moved is an answer, not an empty page.
#[test]
fn a_renewal_that_did_not_move_says_so_in_words() {
    let terms = vec![
        term(0, "premium", "annual", "£500.00"),
        term(1, "premium", "annual", "£500.00"),
    ];
    let outcome = ComparisonOutcome {
        diff: diff_terms(&terms, 0, 1, &TermFamilies::default()).rows,
        not_compared: Vec::new(),
        terms,
    };

    let report = build_comparison_report(&outcome, &[], run());
    assert_eq!(report.summary.changed_count, 0);
    assert!(
        report.summary.note.contains("nothing"),
        "the note says nothing moved rather than leaving a person to infer it: {}",
        report.summary.note
    );
}

/// The document says what it is, and reads back as what it was.
#[test]
fn a_comparison_report_is_a_document_that_says_what_it_is() {
    let report = build_comparison_report(&outcome(), &[], run());
    assert_eq!(report.schema, runner::results::COMPARISON_REPORT_SCHEMA);

    let json = serde_json::to_string(&report).expect("a report is plain data");
    let round_tripped: runner::results::ComparisonReport =
        serde_json::from_str(&json).expect("it reads back");
    assert_eq!(round_tripped, report);
}

/// The rendered page: both documents named, every kind marked, and the
/// audit's vocabulary nowhere near it.
#[test]
fn the_rendered_page_names_both_documents_and_marks_every_claim() {
    let report = build_comparison_report(&outcome(), &[], run());
    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.renewal-diff/report.html.tera"),
    )
    .expect("the renewal pack's template");

    let html = runner::render::render_comparison_report(&template, &report)
        .expect("the comparison report renders");
    runner::render::assert_self_contained(&html).expect("no external references");

    // Escaping is Tera's job and it does it right; the assertions below
    // are about the words a person reads, so the apostrophe entity is
    // put back before matching rather than spelled out in every string.
    let body = html
        .split_once("</style>")
        .map(|(_, rest)| rest)
        .unwrap_or(&html)
        .replace("&#39;", "'");
    let body = body.as_str();

    // Which document is which, by the words the pack calls them and the
    // names of the files themselves: a comparison that does not say
    // which side is this year's can be read exactly backwards.
    assert!(body.contains("Last year's policy"), "{body}");
    assert!(body.contains("This year's renewal"));
    assert!(body.contains("policy-2025.pdf"));
    assert!(body.contains("renewal-2026.pdf"));

    assert!(body.contains("Premium"), "the term in words");
    assert!(body.contains("£65.50 more"), "the size and the direction");
    assert!(body.contains("Worked out"), "a subtracted amount is marked");
    assert!(
        body.contains("Read from the documents"),
        "a phrase change is marked: {body}"
    );
    assert!(
        body.contains("The premium is £565.50."),
        "the passage behind the row"
    );

    for word in ["subscription", "merchant", "annualised", "obligation"] {
        assert!(
            !body.contains(word),
            "{word:?} belongs to another typology's report"
        );
    }
}

/// #377: a term Kettle declined to compare says so, by name and count.
///
/// Not silence, and not a bare passage under "check these yourself". A
/// reader who is only told a passage needs their eyes cannot tell
/// "Kettle looked and found nothing" from "Kettle refused to guess",
/// and those are opposite facts about their policy. The count is in the
/// sentence because it is the reason: three excesses under three
/// sections is *why* no single one could be compared.
#[test]
fn a_term_that_was_not_compared_says_why() {
    let terms = vec![
        term(0, "compulsory_excess", "per_claim", "£250.00"),
        term(0, "compulsory_excess", "per_claim", "£500.00"),
        term(0, "compulsory_excess", "per_claim", "£750.00"),
        term(1, "compulsory_excess", "per_claim", "£300.00"),
    ];
    let decided = diff_terms(&terms, 0, 1, &TermFamilies::default());
    let outcome = ComparisonOutcome {
        diff: decided.rows,
        not_compared: decided.not_compared,
        terms,
    };

    let report = build_comparison_report(&outcome, &[], run());

    assert!(
        report.changes.is_empty(),
        "nothing was compared: {:?}",
        report.changes
    );
    // Every reading is on the page, each carrying the same reason.
    assert_eq!(report.needs_review.len(), 4);
    assert!(
        report.needs_review.iter().all(|passage| passage.reason
            == "Compulsory excess appears 3 times in this document, so Kettle hasn't \
                    compared it."),
        "{:?}",
        report.needs_review
    );
    // The term in a person's words, never the enum's.
    assert!(!report.needs_review[0].reason.contains("compulsory_excess"));
    // And the summary counts them, so the page does not open by saying
    // nothing moved while four passages below say otherwise.
    assert_eq!(report.summary.needs_review_count, 4);
    assert!(
        report.summary.note.contains("need your eyes"),
        "{}",
        report.summary.note
    );
}

/// #379: a row claiming a value moved shows two passages, and a reader
/// verifying the delta must know which is last year's. Each quote keeps
/// the document it was read from, and the report shows the pack's own
/// label for that document beside the passage — never the role key,
/// never a bare pair of blockquotes.
#[test]
fn every_quoted_passage_says_which_document_it_came_from() {
    let report = build_comparison_report(&outcome(), &[], run());

    let premium = row(&report, "premium");
    let labels: Vec<&str> = premium
        .sides
        .iter()
        .map(|side| side.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["Last year's policy", "This year's renewal"],
        "the earlier document's side first, each attributed: {:#?}",
        premium.sides
    );

    let body = rendered(&report);

    // The label sits with the passage it attributes, in the same
    // element — a label a reader has to pair up by eye is not an
    // attribution.
    let attributed = body.split("govuk-summary-list__row").any(|block| {
        block.contains("The premium is £500.00.") && block.contains("Last year's policy")
    });
    assert!(
        attributed,
        "the passage renders beside the label of the document it came from: {body}"
    );
}

/// #379, the presentation half: attribution said *which* document a
/// passage came from, and that is not yet enough to check a delta.
///
/// A reader verifying "£65.50 more" has to pair three facts — last
/// year's value, this year's, and the passage behind each — and a flat
/// `quotes` list leaves that pairing to whoever renders it. Two
/// passages in document order only *look* paired: an added term has one
/// passage and two sides, so position stops meaning anything the moment
/// a document is silent. So the runner pairs them and the template
/// arranges what it is given (#361 — return the engine's answer, don't
/// let a screen re-derive its rule).
///
/// One side per compared document, in document order, always both —
/// "this document does not state it" is a finding, not an absence to
/// drop. The difference is not a side: no document says it, and it
/// already renders from `delta`, `direction`, `state` and `kind`.
#[test]
fn a_side_pairs_each_document_value_with_the_passage_behind_it() {
    let report = build_comparison_report(&outcome(), &[], run());

    assert_eq!(
        row(&report, "premium").sides,
        vec![
            TermSideOut {
                label: "Last year's policy".to_owned(),
                value: Some("£500.00".to_owned()),
                quote: Some("The premium is £500.00.".to_owned()),
            },
            TermSideOut {
                label: "This year's renewal".to_owned(),
                value: Some("£565.50".to_owned()),
                quote: Some("The premium is £565.50.".to_owned()),
            },
        ],
        "each side carries its own document's value and the passage evidencing it",
    );
}

/// The case the pairing exists for: one passage, two sides.
///
/// A no claims discount only this year's renewal names has a single
/// quote, and a renderer walking `quotes` by position would put this
/// year's words in last year's row — the exact misreading #379 was
/// filed about, arrived at from the other direction. Last year's side
/// keeps its place with nothing in it, because "last year's policy does
/// not mention this" is the finding; dropping the side would make an
/// added term indistinguishable from one nobody looked for.
#[test]
fn a_term_only_one_document_names_leaves_the_other_side_empty() {
    let report = build_comparison_report(&outcome(), &[], run());

    assert_eq!(
        row(&report, "no_claims_discount").sides,
        vec![
            TermSideOut {
                label: "Last year's policy".to_owned(),
                value: None,
                quote: None,
            },
            TermSideOut {
                label: "This year's renewal".to_owned(),
                value: Some("£60.00".to_owned()),
                quote: Some("The no claims discount is £60.00.".to_owned()),
            },
        ],
        "the silent document keeps its side, and the passage stays with the document that said it",
    );

    // And the mirror, so neither side is special-cased: a cover limit
    // only last year's policy names.
    assert_eq!(
        row(&report, "cover_limit").sides,
        vec![
            TermSideOut {
                label: "Last year's policy".to_owned(),
                value: Some("£1,000,000.00".to_owned()),
                quote: Some("The cover limit is £1,000,000.00.".to_owned()),
            },
            TermSideOut {
                label: "This year's renewal".to_owned(),
                value: None,
                quote: None,
            },
        ],
    );
}

/// Render the renewal pack's report and hand back its body.
fn rendered(report: &ComparisonReport) -> String {
    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.renewal-diff/report.html.tera"),
    )
    .expect("the renewal pack's template");
    let html =
        runner::render::render_comparison_report(&template, report).expect("the report renders");
    html.split_once("</style>")
        .map(|(_, rest)| rest.to_owned())
        .unwrap_or(html)
        .replace("&#39;", "'")
}

/// #379: the evidence belongs beneath the claim it evidences, not in a
/// section at the foot of the page.
///
/// A reader checking "£65.50 more" had to scroll to a second list,
/// find the right heading and pair two passages by eye — and the pack's
/// promise is that a claim can be checked *locally*. So each row keeps
/// its own passages in a disclosure directly beneath it, which is the
/// pattern the audit report's ledger already ships (`k-ledger__evidence`
/// wrapping a `govuk-details`). Reusing the class rather than growing a
/// sibling is what keeps the print rules — a closed disclosure has to
/// open on paper — true for both reports at once.
#[test]
fn the_passages_behind_a_row_sit_in_a_disclosure_beneath_it() {
    let body = rendered(&build_comparison_report(&outcome(), &[], run()));

    // The detached section is gone, not merely relocated.
    assert!(
        !body.contains("Where each of these comes from"),
        "the foot-of-page evidence section is gone: {body}"
    );

    // The premium's own evidence row carries both passages, each keyed
    // by the document that said it.
    let evidence = body
        .split("k-ledger__evidence")
        .find(|block| block.contains("The premium is £500.00."))
        .unwrap_or_else(|| panic!("no evidence row holding the premium's passages: {body}"));
    assert!(
        evidence.contains("govuk-details"),
        "the passages are behind a disclosure: {evidence}"
    );
    assert!(
        evidence.contains("Last year's policy")
            && evidence.contains("This year's renewal")
            && evidence.contains("The premium is £565.50."),
        "both documents' passages, each labelled: {evidence}"
    );

    // Every disclosure arrives shut, including the rows that moved.
    //
    // The audit ledger opens a row where the evidence changes a
    // decision — a price rise, an unsettled confidence — and those are
    // the exceptions in a statement, so the table stays readable across.
    // A diff has no such exception: what changed *is* the subject, so
    // the same rule would open most of the page and cost the table the
    // scannability it was kept for. Nothing is lost on paper, where the
    // print rules open every disclosure regardless.
    assert!(
        !body.contains("open="),
        "no row arrives expanded, so the table can still be read across: {body}"
    );
}
