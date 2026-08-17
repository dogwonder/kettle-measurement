//! The diff step a renewal comparison needs (#350, for #66).
//!
//! Two documents, the same named terms, and the only question worth
//! asking: what moved. Every pairing here is an identity check on a
//! closed enum — never a string-similarity guess, which is the step
//! where every fabrication in this repo has landed (#348).

use runner::claim::Kind;
use runner::terms::{diff_terms, Term, TermChange, TermFamilies, ValueKind, ValueShape};
use rust_decimal::Decimal;
use std::str::FromStr;

/// A term read out of document `document`, quoting the whole of the
/// passage it came from; nothing here computes.
fn term(document: usize, name: &str, basis: &str, value: &str) -> Term {
    Term {
        term: name.to_owned(),
        basis: basis.to_owned(),
        value: value.to_owned(),
        quote: format!("{name}: {value}"),
        segment: format!("{name}: {value}"),
        document,
        confidence: "high".to_owned(),
    }
}

/// Find the one diff for a term, or fail loudly.
fn change_for<'a>(diffs: &'a [runner::terms::TermDiff], name: &str) -> &'a TermChange {
    let mut found = diffs.iter().filter(|d| d.term == name);
    let first = found.next().unwrap_or_else(|| panic!("no diff for {name}"));
    &first.change
}

/// The named test from #350. Two documents carrying the same term on
/// different bases must not pair: a £45 monthly instalment against a
/// £520 annual premium is not a 1000% rise, it is two different
/// measurements, and reporting it as a change is a fabrication the
/// reader has no way to catch.
#[test]
fn a_term_diff_pairs_across_roles_and_refuses_a_mismatched_basis() {
    let previous = [
        term(0, "compulsory_excess", "per_claim", "£250"),
        term(0, "premium", "monthly", "£45.00"),
    ];
    let renewal = [
        term(1, "compulsory_excess", "per_claim", "£500"),
        term(1, "premium", "annual", "£520.00"),
    ];
    let terms: Vec<Term> = previous.into_iter().chain(renewal).collect();

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default()).rows;

    // The excess shares its basis, so it pairs and it changed.
    assert_eq!(
        change_for(&diffs, "compulsory_excess"),
        &TermChange::Changed {
            from: "£250".to_owned(),
            to: "£500".to_owned(),
            delta: Some(Decimal::from(250)),
        },
    );

    // The premium does not pair. It is reported as one term gone and
    // one arrived — never as a change, and never with a delta.
    let premiums: Vec<&runner::terms::TermDiff> =
        diffs.iter().filter(|d| d.term == "premium").collect();
    assert_eq!(
        premiums.len(),
        2,
        "a mismatched basis must not pair: {diffs:#?}"
    );
    assert!(
        premiums.iter().any(|d| d.basis == "monthly"
            && d.change
                == TermChange::Removed {
                    value: "£45.00".to_owned()
                }),
        "the previous year's monthly premium is gone, not changed: {premiums:#?}"
    );
    assert!(
        premiums.iter().any(|d| d.basis == "annual"
            && d.change
                == TermChange::Added {
                    value: "£520.00".to_owned()
                }),
        "the renewal's annual premium is new, not changed: {premiums:#?}"
    );
    assert!(
        !diffs
            .iter()
            .any(|d| matches!(d.change, TermChange::Changed { .. }) && d.term == "premium"),
        "a mismatched basis must never be reported as a change: {diffs:#?}"
    );
}

/// The same value on both sides is said plainly. A renewal where
/// nothing moved is a useful answer, not an empty report.
#[test]
fn a_term_that_did_not_move_is_reported_unchanged() {
    let terms = vec![
        term(0, "compulsory_excess", "per_claim", "£250"),
        term(1, "compulsory_excess", "per_claim", "£250"),
    ];

    assert_eq!(
        change_for(
            &diff_terms(&terms, 0, 1, &TermFamilies::default()).rows,
            "compulsory_excess"
        ),
        &TermChange::Unchanged {
            value: "£250".to_owned()
        },
    );
}

/// The arithmetic is Rust's and it is `Decimal` (CLAUDE.md). Money
/// never floats, so a £0.10 rise is exactly 0.10.
#[test]
fn a_delta_is_decimal_and_exact() {
    let terms = vec![
        term(0, "premium", "annual", "£1,234.56"),
        term(1, "premium", "annual", "£1,234.66"),
    ];

    assert_eq!(
        change_for(
            &diff_terms(&terms, 0, 1, &TermFamilies::default()).rows,
            "premium"
        ),
        &TermChange::Changed {
            from: "£1,234.56".to_owned(),
            to: "£1,234.66".to_owned(),
            delta: Some(Decimal::from_str("0.10").expect("a decimal")),
        },
    );
}

/// Not every named value is money. A term whose value is a phrase can
/// still change, and it changes without a delta — an invented number
/// would be worse than none.
#[test]
fn a_value_that_is_not_an_amount_changes_without_a_delta() {
    let terms = vec![
        term(0, "cooling_off_period", "per_policy", "14 days"),
        term(1, "cooling_off_period", "per_policy", "21 days"),
    ];

    assert_eq!(
        change_for(
            &diff_terms(&terms, 0, 1, &TermFamilies::default()).rows,
            "cooling_off_period"
        ),
        &TermChange::Changed {
            from: "14 days".to_owned(),
            to: "21 days".to_owned(),
            delta: None,
        },
    );
}

/// A row states the kind of the claim it makes (#366, #367). Today a
/// subtracted amount and a phrase change render identically, and they
/// are not the same claim: "£250 more" is Rust's arithmetic and wrong
/// is a bug with a test, where "14 days became 21 days" is two passages
/// read off the page and nothing computed. The report cannot mark the
/// difference unless the diff carries it, and deriving it here is the
/// code saying what it did — a declared kind can be wrong.
#[test]
fn a_diff_row_states_the_kind_of_its_claim() {
    let terms = vec![
        term(0, "premium", "annual", "£500"),
        term(1, "premium", "annual", "£520"),
        term(0, "cooling_off_period", "per_policy", "14 days"),
        term(1, "cooling_off_period", "per_policy", "21 days"),
        term(0, "voluntary_excess", "per_claim", "£100"),
        term(1, "voluntary_excess", "per_claim", "£100"),
        term(1, "legal_cover", "per_policy", "£30"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default()).rows;
    let kind_for = |name: &str| {
        diffs
            .iter()
            .find(|d| d.term == name)
            .unwrap_or_else(|| panic!("no diff for {name}"))
            .kind
    };

    // Rust subtracted these two amounts, so the row asserts arithmetic.
    assert_eq!(kind_for("premium"), Kind::WorkedOut);
    // Both sides are passages off the page. Nothing was computed, and
    // saying "worked out" over this row would be the false assurance
    // #367 exists to prevent.
    assert_eq!(kind_for("cooling_off_period"), Kind::ReadAndVerified);
    // A value that did not move, and one that only the renewal names:
    // still read, never computed.
    assert_eq!(kind_for("voluntary_excess"), Kind::ReadAndVerified);
    assert_eq!(kind_for("legal_cover"), Kind::ReadAndVerified);
}

/// `other` exists so the model has an honest place for a term it
/// recognises and the pack does not model. It is a routing answer, not
/// a finding: it never pairs, and it never reaches the diff (#350).
#[test]
fn other_never_reaches_the_diff() {
    let terms = vec![
        term(0, "other", "per_policy", "£99"),
        term(1, "other", "per_policy", "£150"),
    ];

    assert!(
        diff_terms(&terms, 0, 1, &TermFamilies::default())
            .rows
            .is_empty(),
        "`other` is for a person to look at, never a compared finding"
    );
}

/// Order is the reader's, not the model's: a diff is read down the
/// page, so it has to be the same page every time. Terms sort by name
/// then basis, and a term appearing twice on different bases keeps a
/// stable order between its two halves.
#[test]
fn the_diff_is_ordered_deterministically() {
    let terms = vec![
        term(1, "premium", "annual", "£520"),
        term(0, "compulsory_excess", "per_claim", "£250"),
        term(1, "compulsory_excess", "per_claim", "£500"),
        term(0, "voluntary_excess", "per_claim", "£100"),
    ];

    let keys: Vec<(String, String)> = diff_terms(&terms, 0, 1, &TermFamilies::default())
        .rows
        .into_iter()
        .map(|d| (d.term, d.basis))
        .collect();

    assert_eq!(
        keys,
        vec![
            ("compulsory_excess".to_owned(), "per_claim".to_owned()),
            ("premium".to_owned(), "annual".to_owned()),
            ("voluntary_excess".to_owned(), "per_claim".to_owned()),
        ],
    );
}

// ── #377: a term stated twice in one document has no scope ──────────

/// The failure the first real comparison report made, in miniature.
///
/// A commercial schedule states three cover sections, each repeating
/// *Insurance amount*, *Excess*, *Annual premium*. One modelled term
/// then maps to three real values, and `(term, basis)` — sound where
/// each term occurs once — becomes arbitrary. The old rule was "first
/// reading wins", which is how one section's excess came to be
/// subtracted from another's, and one year's section premium from the
/// next year's total, both rendered **Worked out**.
///
/// The floor keys on the repetition itself, not on any attempt to
/// detect a heading: a `(term, basis)` read more than once in one
/// document does not pair at all.
#[test]
fn a_term_stated_twice_in_one_document_does_not_pair() {
    let terms = vec![
        // Last year, under two sections.
        term(0, "compulsory_excess", "per_claim", "£250"),
        term(0, "compulsory_excess", "per_claim", "£500"),
        term(1, "compulsory_excess", "per_claim", "£300"),
        term(1, "compulsory_excess", "per_claim", "£600"),
        // And one that is stated once on each side, which still pairs.
        term(0, "premium", "annual", "£480"),
        term(1, "premium", "annual", "£520"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default());

    assert!(
        !diffs.rows.iter().any(|row| row.term == "compulsory_excess"),
        "an arbitrary pairing is not a finding: {:#?}",
        diffs.rows
    );
    // The rest of the document is still answered. Refusing the whole
    // comparison would be honest and useless — the premium here is
    // stated once on each side and moved by £40.
    assert_eq!(
        change_for(&diffs.rows, "premium"),
        &TermChange::Changed {
            from: "£480".to_owned(),
            to: "£520".to_owned(),
            delta: Some(Decimal::from(40)),
        },
    );
}

/// Refused, not dropped. Every reading reaches a person with its quote,
/// and the report can say *why* it was not compared — the difference
/// between believing nothing changed and knowing Kettle declined to
/// say.
#[test]
fn every_refused_reading_reaches_a_person_with_its_quote() {
    let terms = vec![
        term(0, "compulsory_excess", "per_claim", "£250"),
        term(0, "compulsory_excess", "per_claim", "£500"),
        term(1, "compulsory_excess", "per_claim", "£300"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default());

    assert_eq!(diffs.not_compared.len(), 1, "{:#?}", diffs.not_compared);
    let refused = &diffs.not_compared[0];
    assert_eq!(refused.term, "compulsory_excess");
    assert_eq!(refused.basis, "per_claim");
    // The count is the document's, not the pair's: two readings in the
    // document that repeated it. That is the number the sentence to a
    // person quotes, and counting all three would overstate it.
    assert_eq!(refused.readings, 2);
    // Every reading's quote, from both documents — a person deciding
    // which section they meant needs all of them.
    assert_eq!(refused.quotes.len(), 3, "{:#?}", refused.quotes);
}

/// A repetition on either side is enough. Last year stating one excess
/// and this year stating three does not make this year's first reading
/// the right one to subtract from.
#[test]
fn a_repetition_on_one_side_alone_still_refuses() {
    let terms = vec![
        term(0, "cover_limit", "per_policy", "£1,000"),
        term(1, "cover_limit", "per_policy", "£1,000"),
        term(1, "cover_limit", "per_policy", "£2,000"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default());

    assert!(diffs.rows.is_empty(), "{:#?}", diffs.rows);
    assert_eq!(diffs.not_compared.len(), 1);
    assert_eq!(diffs.not_compared[0].readings, 2);
}

/// A term stated once in each document is untouched by any of this —
/// the case every fixture in the bed is, and the case that must not
/// become a referral.
#[test]
fn one_reading_each_side_is_unaffected() {
    let terms = vec![
        term(0, "premium", "annual", "£480"),
        term(1, "premium", "annual", "£520"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default());

    assert_eq!(diffs.rows.len(), 1);
    assert!(diffs.not_compared.is_empty());
}

// ── #380: a value whose shape the term cannot hold ──────────────────

/// The first real comparison report rendered a `cover_limit` whose
/// value was a policy period — "From <date> to <date> both days
/// inclusive" — paired against a monetary figure from the other
/// document, and it reached the page as a finding.
///
/// Nothing downstream could have caught it. `Changed { delta: None }`
/// is a legitimate state (the phrase change above), so a date range
/// where money belongs is indistinguishable from an honest phrase by
/// the time it gets here. The check has to be against what the term
/// says it can hold, and that is what a shape is.
#[test]
fn money_does_not_hold_a_date_range() {
    let money = ValueShape::of(&[ValueKind::Money]);

    assert!(money.holds("£1,234.56"));
    assert!(money.holds("£0"));
    assert!(
        !money.holds("From 1 September 2026 to 31 August 2027 both days inclusive"),
        "the row that prompted #380"
    );
    // Deliberately strict, for the same reason `delta` is: a parser
    // loose enough to read `14` out of "14 days" would subtract two
    // numbers about different things.
    assert!(!money.holds("14 days"));
    assert!(!money.holds("Included"));
    assert!(!money.holds(""));
}

/// Not every named value is money, and a shape that only knew money
/// would refer every honest phrase to a person.
#[test]
fn each_kind_holds_the_values_it_names() {
    let duration = ValueShape::of(&[ValueKind::Duration]);
    assert!(duration.holds("14 days"));
    assert!(duration.holds("12 months"));
    assert!(!duration.holds("£250"));
    assert!(!duration.holds("From 1 September 2026 to 31 August 2027"));

    let percentage = ValueShape::of(&[ValueKind::Percentage]);
    assert!(percentage.holds("65%"));
    assert!(percentage.holds("7.5 per cent"));
    assert!(!percentage.holds("£65"));

    // The escape hatch, and it has to be declared: a term whose value
    // really is free text says so, rather than every term defaulting to
    // a check that checks nothing.
    let text = ValueShape::of(&[ValueKind::Text]);
    assert!(text.holds("From 1 September 2026 to 31 August 2027"));
    assert!(!text.holds("   "), "even free text is not nothing");
}

/// A no-claims discount is written as money on one schedule and as a
/// percentage on the next, and both are correct. A term declares the
/// kinds it can hold, not the one it usually does.
#[test]
fn a_term_may_hold_more_than_one_kind() {
    let either = ValueShape::of(&[ValueKind::Money, ValueKind::Percentage]);

    assert!(either.holds("£150"));
    assert!(either.holds("65%"));
    assert!(!either.holds("From 1 September 2026 to 31 August 2027"));
}

/// The refusal reaches a person as a sentence, so the shape has to be
/// sayable in words. British English, and no jargon: "money" and "a
/// length of time" are what the person reading the report understands.
#[test]
fn a_shape_says_what_it_expected_in_words() {
    assert_eq!(ValueShape::of(&[ValueKind::Money]).in_words(), "an amount");
    assert_eq!(
        ValueShape::of(&[ValueKind::Money, ValueKind::Percentage]).in_words(),
        "an amount or a percentage"
    );
    assert_eq!(
        ValueShape::of(&[ValueKind::Duration]).in_words(),
        "a length of time"
    );
}

/// A document that is neither side of the comparison contributes
/// nothing. A run may carry more than two documents (#330), and a term
/// from a third one silently joining the diff would be a finding about
/// a document nobody asked to compare.
#[test]
fn a_document_outside_the_comparison_is_ignored() {
    let terms = vec![
        term(0, "premium", "annual", "£500"),
        term(1, "premium", "annual", "£520"),
        term(2, "premium", "annual", "£999"),
    ];

    let diffs = diff_terms(&terms, 0, 1, &TermFamilies::default()).rows;

    assert_eq!(diffs.len(), 1, "only the two named documents: {diffs:#?}");
    assert_eq!(
        change_for(&diffs, "premium"),
        &TermChange::Changed {
            from: "£500".to_owned(),
            to: "£520".to_owned(),
            delta: Some(Decimal::from(20)),
        },
    );
}
