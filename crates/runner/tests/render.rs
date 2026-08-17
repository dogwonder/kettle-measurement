//! The report (#32), its deterministic summary (#33) and its honest
//! checklist (#34).
//!
//! `packs/app.kttl.subscription-audit/report.html.tera` is the source;
//! `fixtures/run-01/report.html` is its committed rendered example
//! (#225).

use runner::render::{assert_self_contained, fallback_summary, render_report, RenderError};
use runner::results::{Confidence, RegularSpend, RunReport};
use rust_decimal::Decimal;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn report() -> RunReport {
    serde_json::from_str(&std::fs::read_to_string(repo("fixtures/run-01/results.json")).unwrap())
        .expect("the fixture matches results.rs — if this fails, the contract moved")
}

fn decimal(value: &str) -> Decimal {
    Decimal::from_str(value).expect("a decimal literal")
}

fn template() -> String {
    std::fs::read_to_string(repo("packs/app.kttl.subscription-audit/report.html.tera"))
        .expect("the pack's template")
}

fn rendered() -> String {
    render_report(&template(), &report(), None).expect("the report renders")
}

#[test]
fn renders_from_fixture_run() {
    let html = rendered();

    // The headline number, as a person reads it.
    assert!(
        html.contains("£1,046.64"),
        "the annualised total is the headline"
    );
    assert!(html.contains("£87.22"), "and what that is a month");

    // Every finding appears, with its evidence.
    for merchant in [
        "Netflix",
        "Spotify",
        "PureGym",
        "British Gas",
        "Amazon Prime",
    ] {
        assert!(
            html.contains(merchant),
            "{merchant} is missing from the report"
        );
    }
    assert!(html.contains("£155.88"), "Netflix's yearly cost");
    assert!(
        html.contains("12 payments, one every month on or near the 5th"),
        "the evidence for a finding travels with it"
    );

    // Regular spending and income, which are not subscriptions.
    assert!(html.contains("Tesco"));
    assert!(html.contains("Acme Payroll"));

    assert!(html.starts_with("<!doctype html>") || html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("lang=\"en-GB\""), "British English, declared");
}

#[test]
fn contains_no_external_urls() {
    let html = rendered();
    assert!(
        assert_self_contained(&html).is_ok(),
        "the fixture report reaches outside itself"
    );

    // The crude sweep below is belt-and-braces behind `assert_self_contained`,
    // which parses the document properly. It runs over the markup with the
    // <style> blocks lifted out, because CSS can *name* a scheme without
    // fetching anything: govuk's print styles carry the attribute selector
    // `[href^="http://"].govuk-link::after`, which is a rule about links the
    // page already has, not a reference the page goes out and resolves.
    // Stylesheet content gets its own, sharper check underneath.
    let (markup, styles) = split_out_style_blocks(&html);

    for reach in [
        "http://",
        "https://",
        "//cdn",
        "<script src",
        "<link rel=\"stylesheet\"",
    ] {
        assert!(
            !markup.contains(reach),
            "the report must work with the network off, but contains {reach:?}"
        );
    }

    // The only two ways CSS reaches the network. Both are already refused by
    // `assert_self_contained`; asserting them here keeps the guarantee
    // legible at the place someone will read it.
    for reach in ["@import", "url(http", "url('http", "url(\"http", "url(//"] {
        assert!(
            !styles.contains(reach),
            "the report's stylesheet must fetch nothing, but contains {reach:?}"
        );
    }
}

/// Returns the document with every `<style>` block removed, and the contents
/// of those blocks concatenated — so a check can be aimed at markup or at CSS
/// without one's syntax being read as the other's.
fn split_out_style_blocks(html: &str) -> (String, String) {
    let mut markup = String::with_capacity(html.len());
    let mut styles = String::new();
    let mut rest = html;

    while let Some(open) = rest.find("<style>") {
        markup.push_str(&rest[..open]);
        let after_open = &rest[open + "<style>".len()..];
        let Some(close) = after_open.find("</style>") else {
            // an unclosed <style> is not something to quietly tolerate
            panic!("the report has a <style> that is never closed");
        };
        styles.push_str(&after_open[..close]);
        rest = &after_open[close + "</style>".len()..];
    }
    markup.push_str(rest);

    (markup, styles)
}

#[test]
fn a_page_that_phones_home_is_refused() {
    let html = r#"<!doctype html><html><head>
        <link rel="stylesheet" href="https://fonts.example/x.css"></head><body></body></html>"#;
    assert!(matches!(
        assert_self_contained(html),
        Err(RenderError::ExternalAsset(_))
    ));
}

#[test]
fn render_refuses_an_external_reference_before_returning_the_report() {
    let unsafe_template = r#"<!doctype html><html><body>{{ report.title }}<img src = "https://example.test/x"></body></html>"#;
    assert!(matches!(
        render_report(unsafe_template, &report(), None),
        Err(RenderError::ExternalAsset(_))
    ));
}

#[test]
fn self_contained_report_rejects_whitespace_src_and_css_import() {
    for html in [
        r#"<img src = "https://example.test/x">"#,
        r#"<img srcset="data:image/png;base64,eA== 1x, https://example.test/x 2x">"#,
        r#"<style>@import url(https://example.test/x.css)</style>"#,
        r#"<meta http-equiv="refresh" content="0; URL=https://example.test/x">"#,
        r#"<form action="https://example.test/collect"></form>"#,
        r#"<IMG SrC = "https&#58;//example.test/x">"#,
        r#"<p style="background: URL( 'https://example.test/x.png' )">x</p>"#,
    ] {
        assert!(
            matches!(
                assert_self_contained(html),
                Err(RenderError::ExternalAsset(_))
            ),
            "the report must refuse {html:?}"
        );
    }
}

#[test]
fn local_fragments_are_still_self_contained() {
    let html = r##"<!doctype html><html><body><a href="#check-yourself">Check these yourself</a></body></html>"##;
    assert!(assert_self_contained(html).is_ok());
}

#[test]
fn data_uris_are_refused_in_the_supported_report_subset() {
    // Raster data could be made safe, but Kettle's reports do not need
    // embedded images today. Refusing every data URI also avoids treating
    // nested SVG references as inert when they are not.
    let html = r#"<img src="data:image/png;base64,eA==">"#;
    assert!(matches!(
        assert_self_contained(html),
        Err(RenderError::ExternalAsset(_))
    ));
}

#[test]
fn active_content_is_not_part_of_the_supported_report_subset() {
    for html in [
        r#"<script>fetch("https://example.test/report")</script>"#,
        r#"<body onload="location='https://example.test/'"></body>"#,
        r#"<iframe srcdoc="<img src='https://example.test/x'>"></iframe>"#,
    ] {
        assert!(
            matches!(
                assert_self_contained(html),
                Err(RenderError::ExternalAsset(_))
            ),
            "the report must refuse {html:?}"
        );
    }
}

#[test]
fn fallback_summary_when_summary_step_absent() {
    // The optional prose step was skipped, and the report is still whole.
    let html =
        render_report(&template(), &report(), None).expect("renders without a model summary");
    let summary = fallback_summary(&report());

    assert!(
        html.contains(&summary),
        "the deterministic summary fills the space"
    );
    assert!(
        summary.contains("£1,046.64"),
        "it says the total: {summary:?}"
    );
    assert!(
        summary.contains("Netflix"),
        "and names the price rise: {summary:?}"
    );
    assert!(
        !summary.to_lowercase().contains("could not")
            && !summary.to_lowercase().contains("unavailable"),
        "it never apologises for the model: {summary:?}"
    );
}

#[test]
fn a_model_summary_is_used_when_there_is_one() {
    let prose = "Four subscriptions, one of which went up in May.";
    let html = render_report(&template(), &report(), Some(prose)).expect("renders");

    assert!(html.contains(prose));
    assert!(
        !html.contains(&fallback_summary(&report())),
        "one summary or the other, never both"
    );
}

#[test]
fn low_confidence_items_listed_in_review_section() {
    let html = rendered();

    assert!(html.contains("Check these yourself"));
    for about in ["Amazon Prime", "British Gas", "PureGym"] {
        assert!(html.contains(about), "{about} should be on the checklist");
    }

    // Needs-review items appear with their plain reason and their
    // payments, and are visibly not counted.
    assert!(html.contains("AMZN DIGITAL*8H2Q"));
    assert!(html.contains("WWW-GIFFGAFF-COM"));
    assert!(html.contains("Not counted in the totals until you confirm it."));

    // An honest checklist, not a failure notice.
    for alarm in ["Error", "Failed", "Warning", "Invalid"] {
        assert!(
            !html.contains(alarm),
            "the report shows {alarm:?} at the reader"
        );
    }
}

/// The evidence is Kettle's trust mechanism, so it opens wherever it
/// changes a decision — a price rise, or a confidence below settled —
/// and stays shut on rows that are simply true and unremarkable.
#[test]
fn evidence_opens_on_the_rows_that_earn_it() {
    let html = rendered();

    // Each disclosure names its merchant, so the open state of one row
    // can be read from the markup between its <details> and that name.
    let open_for = |merchant: &str| {
        let summary = format!("The payments behind {merchant}");
        let at = html
            .find(&summary)
            .unwrap_or_else(|| panic!("{merchant} has no disclosure"));
        let details = html[..at]
            .rfind("<details")
            .expect("the summary sits inside a details");
        html[details..at].contains(r#"open="open""#)
    };

    // Netflix rose in May; British Gas and Amazon Prime are only medium.
    for material in ["Netflix", "British Gas", "Amazon Prime"] {
        assert!(
            open_for(material),
            "{material} carries something the reader must see, so its \
             evidence should be open"
        );
    }

    // Spotify and PureGym are settled and unremarkable: shut, so the
    // ledger stays a table you can read across.
    for quiet in ["Spotify", "PureGym"] {
        assert!(
            !open_for(quiet),
            "{quiet} is settled and unremarkable, so its evidence should \
             stay behind the disclosure"
        );
    }
}

#[test]
fn a_merchant_seen_once_is_not_dressed_as_a_pattern() {
    // #219. The section holds everything classified that isn't a
    // subscription, one-offs included — which is worth showing. What it
    // must not do is describe a single observation as a distribution:
    // "typical" needs more than one payment to be typical of, and a
    // "total" that equals the only amount says nothing twice.
    let mut report = report();
    report.regular_spend.push(RegularSpend {
        merchant: "Hackney Fishmonger".to_owned(),
        raw_merchant: "HACKNEY FISH CO".to_owned(),
        kind: "spend".to_owned(),
        category: "groceries".to_owned(),
        visits: 1,
        total: decimal("24.99"),
        typical_visit: decimal("24.99"),
        confidence: Confidence::High,
    });

    let html = render_report(&template(), &report, None).expect("the report renders");
    let row = row_containing(&html, "HACKNEY FISH CO");

    assert!(
        row.contains("1 payment"),
        "a single observation is a payment, not a visit count: {row}"
    );
    assert!(
        !row.contains("typical"),
        "nothing is typical of one payment: {row}"
    );
    assert_eq!(
        row.matches("£24.99").count(),
        1,
        "the one amount is stated once, not as a typical/total pair: {row}"
    );

    // The heading over the table claims a narrower thing than the table
    // selects, which is where the over-claim actually lives.
    assert!(
        !html.contains("Regular spending and income"),
        "the heading promises regularity the section does not check"
    );
    assert!(
        html.contains("Everything else Kettle found"),
        "the heading should say what the table holds"
    );
}

/// The `<tr>` a merchant's raw name falls inside.
fn row_containing<'a>(html: &'a str, needle: &str) -> &'a str {
    let at = html
        .find(needle)
        .unwrap_or_else(|| panic!("{needle} is not in the report"));
    let start = html[..at].rfind("<tr").expect("inside a row");
    let end = at + html[at..].find("</tr>").expect("a row that closes");
    &html[start..end]
}
