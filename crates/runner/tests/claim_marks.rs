//! #367's last item: a pack cannot render a claim without saying what
//! kind of claim it is.
//!
//! The issue asked for a load-time refusal — a pack whose report shape
//! can produce an unmarked claim, refused with the same shape as the
//! closed-set refusal. Writing the second template showed that check
//! cannot be honest: at load there is only template *source*, and a
//! template that mentions `kind` in a comment, or in a branch nothing
//! reaches, would pass it. It would be a guardrail that reads for the
//! word rather than for the behaviour.
//!
//! What it does instead is render. Each pack's own template is given
//! the same document twice — every claim worked out, then every claim
//! read — and the two renderings must differ. That is the property in
//! its plainest form: **the kind changes what a person sees.** A
//! template that ignores the field renders identically both ways, and
//! that is exactly the regression, caught without dictating anybody's
//! wording.
//!
//! Discovery, not a list. `packs/*` is read off disk, so pack three is
//! covered the day it exists rather than the day somebody remembers
//! this file — the lesson from the tokens sync, where naming one pack
//! let the second drift unnoticed for a fortnight.

use runner::claim::Kind;
use runner::comparison_report::build_comparison_report;
use runner::document::Segment;
use runner::letter_report::build_letter_report;
use runner::packs::{load_pack, PipelineStep};
use runner::results::{ComparedDocument, ComparisonRunInfo, LetterRunInfo};
use runner::run::{ComparisonOutcome, ExtractionOutcome, Obligation};
use runner::terms::{TermChange, TermDiff};
use runner::timeline::Resolved;
use std::path::{Path, PathBuf};

/// Packs whose report deliberately does not mark its claims yet, each
/// with a reason and a date — the convention the styling guardrails
/// use (`STAGED_GOVUK_COMPONENTS`). Silence is what this test exists to
/// prevent, so an exception is written down or it does not exist.
const STAGED: &[(&str, &str, &str)] = &[(
    "app.kttl.subscription-audit",
    "Sidelined by #348: its central claim rests on inferred merchant \
     identity, which the reader cannot audit, and #366 says explicitly \
     not to retrofit the marks onto it. Marking rows of a report whose \
     grouping is the unsound step would dress the wrong claim in the \
     language of a checked one.",
    "2026-08-04",
)];

/// Every pack directory carrying a report template.
fn packs_with_reports() -> Vec<PathBuf> {
    let packs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&packs)
        .expect("the packs directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("report.html.tera").is_file())
        .collect();
    found.sort();
    found
}

fn pack_id(dir: &Path) -> String {
    dir.file_name()
        .expect("a pack directory has a name")
        .to_string_lossy()
        .into_owned()
}

/// Which typology a pack is, from the step that decides its answers.
/// An unknown one is a hard failure rather than a skip: a pack this
/// test cannot render is a pack it cannot vouch for, and passing
/// quietly is the failure mode it was written against.
fn aggregate_builtin(dir: &Path) -> String {
    let pack =
        load_pack(dir).unwrap_or_else(|error| panic!("{} does not load: {error:?}", pack_id(dir)));
    pack.manifest
        .pipeline
        .iter()
        .find_map(|step| match step {
            PipelineStep::Aggregate { implementation } => Some(implementation.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{} renders a report with no aggregate step", pack_id(dir)))
}

fn segment(text: &str) -> Segment {
    Segment {
        document: 0,
        page: 1,
        ordinal: 1,
        text: text.to_owned(),
        rows: Vec::new(),
    }
}

/// A letter report whose one dated obligation carries `kind`.
fn letter_html(template: &str, kind: Kind) -> String {
    let outcome = ExtractionOutcome {
        date_disputes: vec![],
        obligations: vec![Obligation {
            kind: "payment".to_owned(),
            party: "Harborne Parking Services".to_owned(),
            ask: "Pay £120.00".to_owned(),
            deadline: "within 14 days".to_owned(),
            anchor: "the date of this letter".to_owned(),
            amount: "no amount".to_owned(),
            confidence: "high".to_owned(),
            due: Some(Resolved {
                date: "2026-03-17".parse().expect("a real day"),
                kind,
            }),
            evidence: vec![segment("Please pay £120.00 within 14 days.")],
            dated_by: None,
            priced_by: None,
            disputed: vec![],
        }],
    };
    let run = LetterRunInfo {
        id: "letter-01".to_owned(),
        pack: "app.kttl.letter-to-actions".to_owned(),
        pack_version: "0.1.0".to_owned(),
        file: "letter-01.txt".to_owned(),
        passages: 3,
        started: "2026-03-03T09:00:00Z".to_owned(),
        finished: "2026-03-03T09:00:12Z".to_owned(),
    };
    runner::render::render_letter_report(template, &build_letter_report(&outcome, run))
        .expect("the letter report renders")
}

/// A comparison report whose one changed row carries `kind`.
///
/// The `TermDiff` is built directly rather than through `diff_terms`,
/// because there the kind is derived from whether a delta exists — and
/// deriving it is the point. This asks a narrower question: given a row
/// of each kind, does the template show the difference.
fn comparison_html(template: &str, kind: Kind) -> String {
    let outcome = ComparisonOutcome {
        not_compared: Vec::new(),
        terms: Vec::new(),
        diff: vec![TermDiff {
            term: "premium".to_owned(),
            basis: "annual".to_owned(),
            change: TermChange::Changed {
                from: "£500.00".to_owned(),
                to: "£565.50".to_owned(),
                delta: Some("65.50".parse().expect("a real amount")),
            },
            kind,
            quotes: vec![runner::terms::Quote {
                document: 0,
                text: "The premium is £565.50.".to_owned(),
            }],
        }],
    };
    let run = ComparisonRunInfo {
        id: "renewal-01".to_owned(),
        pack: "app.kttl.renewal-diff".to_owned(),
        pack_version: "0.1.0".to_owned(),
        documents: vec![ComparedDocument {
            role: "previous".to_owned(),
            label: "Last year's policy".to_owned(),
            file: "policy-2025.pdf".to_owned(),
        }],
        passages: 4,
        started: "2026-08-04T09:00:00Z".to_owned(),
        finished: "2026-08-04T09:00:20Z".to_owned(),
    };
    runner::render::render_comparison_report(template, &build_comparison_report(&outcome, &[], run))
        .expect("the comparison report renders")
}

/// The page below the stylesheet — what a person actually reads.
fn body(html: &str) -> String {
    html.split_once("</style>")
        .map(|(_, rest)| rest.to_owned())
        .unwrap_or_else(|| html.to_owned())
}

#[test]
fn every_pack_that_renders_a_report_shows_the_kind_of_the_claims_it_makes() {
    let packs = packs_with_reports();
    assert!(
        packs.len() > 1,
        "discovery found {} pack(s); a search that matches nothing passes every \
         assertion it never runs",
        packs.len()
    );

    for dir in packs {
        let id = pack_id(&dir);
        if let Some((_, reason, date)) = STAGED.iter().find(|(staged, _, _)| *staged == id) {
            println!("staged: {id} — {reason} ({date})");
            continue;
        }

        let template = std::fs::read_to_string(dir.join("report.html.tera")).expect("the template");
        let builtin = aggregate_builtin(&dir);
        // Every kind, not the two somebody remembered. `Kind::ALL` is
        // held to the enum by the compiler, so a kind added without its
        // copy fails here instead of rendering in another kind's words.
        let rendered: Vec<(Kind, String)> = Kind::ALL
            .iter()
            .map(|kind| {
                let html = match builtin.as_str() {
                    "builtin:timeline-sort" => letter_html(&template, *kind),
                    "builtin:term-diff" => comparison_html(&template, *kind),
                    other => panic!(
                        "{id} renders a report through {other}, which this test cannot build a \
                         document for. Teach it that typology, or stage the pack with a reason \
                         and a date — a pack it cannot render is a pack it cannot vouch for."
                    ),
                };
                (*kind, body(&html))
            })
            .collect();

        for (one, (kind, html)) in rendered.iter().enumerate() {
            // Each kind says its own words, in the vocabulary a person
            // can act on (#365) — not merely differing somewhere.
            let expected = match kind {
                Kind::WorkedOut => "Worked out",
                Kind::ReadAndVerified => "Read from",
                Kind::Yours => "you gave",
            };
            assert!(
                html.contains(expected),
                "{id} does not say a {kind:?} claim in its own words \
                 (expected {expected:?}): {html}"
            );
            for (other, html_other) in rendered.iter().skip(one + 1) {
                assert_ne!(
                    html, html_other,
                    "{id} renders {kind:?} and {other:?} identically, so the kind reaches \
                     the runner and stops there — the copy-layer claim #367 refuses"
                );
            }
        }
    }
}

/// A staged exception that outlives its pack is a comment pretending to
/// be a rule.
#[test]
fn every_staged_pack_still_exists() {
    let packs: Vec<String> = packs_with_reports()
        .iter()
        .map(|dir| pack_id(dir))
        .collect();
    for (staged, _, _) in STAGED {
        assert!(
            packs.iter().any(|id| id == staged),
            "{staged} is staged out of the claim-mark guard but renders no report; \
             remove the entry"
        );
    }
}
