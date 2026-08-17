//! Named values compared across two documents (#350, for #66).
//!
//! A renewal diff asks one question of two documents: what moved. The
//! model's part is small and closed — read a named term off the page,
//! verbatim, with the passage it came from. Every number below is
//! `Decimal` arithmetic Rust does (CLAUDE.md); the model neither
//! computes nor compares, because a delta it invented is a price rise
//! nobody can check.
//!
//! Pairing is an **identity check on a closed enum**, never a
//! string-similarity guess. That is the whole difference between this
//! and the subscription audit, whose central claim rests on inferred
//! merchant identity and cannot be audited by the person reading it
//! (#348). Here every claim carries its own quote, so verification is
//! per-claim and local.
//!
//! Generic, not renewal-specific: any pack extracting named values from
//! two documents diffs the same way (#67's payslips, a tariff
//! comparison), which is why the builtin is `term-diff` rather than
//! `renewal-diff`.

use crate::claim::Kind;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// The model's honest place for a term it recognises and the pack does
/// not model. It is a routing answer — the passage goes to a person —
/// so it never pairs and never reaches the diff.
pub const OTHER: &str = "other";

/// Labels that name the same kind of value, declared by the pack
/// (#461): family name → the terms in it.
///
/// Pack data for exactly `value_kinds`' reason. "A compulsory excess and
/// a total excess are both excesses" is pack policy, and a runner that
/// knew it would be pack-specific runner code in a module whose whole
/// claim is that it is generic — any pack extracting named values from
/// two documents diffs the same way.
///
/// A family is only ever read to *refuse* a comparison, never to make
/// one. Two terms being siblings does not make them equivalent: nothing
/// here ever pairs a compulsory excess with a total one, and a family
/// declared wrongly can cost a comparison but cannot invent a number.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct TermFamilies(BTreeMap<String, Vec<String>>);

impl TermFamilies {
    /// The family this term belongs to, or `None` if the pack put it in
    /// none — which is the ordinary case and means "this label has no
    /// siblings to be confused with".
    pub fn of(&self, term: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, members)| members.iter().any(|member| member == term))
            .map(|(name, _)| name.as_str())
    }

    /// Every term the pack put in a family, for validation.
    pub fn members(&self) -> impl Iterator<Item = &str> {
        self.0.values().flatten().map(String::as_str)
    }
}

/// One named value read out of one document.
///
/// `value` is verbatim as written ("£1,234.56", "14 days"). Parsing it
/// is Rust's, below: a misparse is then a bug with a test, not a model
/// error arriving with a confidence attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The closed enum from the pack's schema. This is the identity
    /// half of the pairing key.
    pub term: String,
    /// What the value is measured against — "per_claim", "monthly",
    /// "annual". The other half of the key, and load-bearing: without
    /// it a £45 monthly instalment pairs with a £520 annual premium and
    /// reads as a 1000% rise. Deterministic once declared, undetectable
    /// if not.
    pub basis: String,
    /// The value exactly as the document writes it.
    pub value: String,
    /// The words the value was read from, for the per-claim
    /// verification #258 requires of anything the model says about a
    /// source. Evidence that the value is on the page — not an
    /// identifier of *where*, which is what [`Term::segment`] is for:
    /// a schedule's three sections each say `Excess`, and a quote of
    /// that word is verbatim in all three (#457).
    pub quote: String,
    /// The passage this was read from, verbatim, as the run segmented
    /// the document.
    ///
    /// Rust holds this already — the model is asked one passage at a
    /// time — so nothing downstream needs to reconstruct it from the
    /// quote. It used to, and a weak quote then rebound a correct
    /// reading onto a neighbouring section's expectation and scored it
    /// as a confident wrong number (#457, #361).
    pub segment: String,
    /// Which input document it came from, indexing the run's bound
    /// inputs (#330, #332). The model is never asked this: role is a
    /// fact Rust already holds, and asking would invite an answer it
    /// cannot reliably give.
    pub document: usize,
    /// The model's confidence in its reading of this passage.
    pub confidence: String,
}

/// What kind of thing a term's value is (#380).
///
/// The model may say what it read; Rust checks that what it read is the
/// kind of thing the question was about. That is #258's quote
/// verification one field along — the quote proves the words are on the
/// page, and this proves the words answer the question asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueKind {
    /// An amount of money, and only that: the same strict parser the
    /// arithmetic uses, so a value that passes here is a value the diff
    /// can subtract.
    Money,
    /// A proportion — "65%", "7.5 per cent".
    Percentage,
    /// A length of time — "14 days", "12 months".
    Duration,
    /// Free text, for a term whose value really is a phrase. The escape
    /// hatch, and it has to be declared: a pack that means "don't check
    /// this" says so, rather than every unchecked term looking guarded.
    Text,
}

impl ValueKind {
    /// The manifest's spelling, or nothing. Deliberately not a serde
    /// derive: an unknown kind is a pack author's typo, and it should
    /// arrive as a sentence naming what they wrote rather than as
    /// "data did not match any variant".
    fn named(name: &str) -> Option<Self> {
        match name {
            "money" => Some(ValueKind::Money),
            "percentage" => Some(ValueKind::Percentage),
            "duration" => Some(ValueKind::Duration),
            "text" => Some(ValueKind::Text),
            _ => None,
        }
    }

    /// Can this kind hold this written value?
    fn holds(self, value: &str) -> bool {
        let trimmed = value.trim();
        match self {
            ValueKind::Money => amount(trimmed).is_some(),
            ValueKind::Percentage => percentage(trimmed),
            ValueKind::Duration => duration(trimmed),
            ValueKind::Text => !trimmed.is_empty(),
        }
    }

    /// The kind in a person's words, for the sentence that explains why
    /// a passage reached them.
    fn in_words(self) -> &'static str {
        match self {
            ValueKind::Money => "an amount",
            ValueKind::Percentage => "a percentage",
            ValueKind::Duration => "a length of time",
            ValueKind::Text => "words",
        }
    }
}

/// The kinds one term's value may take.
///
/// More than one because a no-claims discount is written as money on
/// one schedule and as a percentage on the next, and both are correct.
/// A term declares what it *can* hold, not what it usually does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueShape(Vec<ValueKind>);

/// `"money"` and `["money", "percentage"]` both read naturally in a
/// manifest, so both are accepted; an unknown name is refused by the
/// name the author wrote, not by the shape of the JSON.
impl<'de> Deserialize<'de> for ValueShape {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Any(Vec<String>),
        }

        let names = match Raw::deserialize(deserializer)? {
            Raw::One(name) => vec![name],
            Raw::Any(names) => names,
        };
        names
            .into_iter()
            .map(|name| {
                ValueKind::named(&name).ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "{name:?} isn't a kind of value Kettle can check — a term holds \
                         money, a percentage, a duration or text"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(ValueShape)
    }
}

impl ValueShape {
    pub fn of(kinds: &[ValueKind]) -> Self {
        ValueShape(kinds.to_vec())
    }

    /// Is this written value one of the kinds the term can hold?
    ///
    /// An empty shape holds nothing. It cannot arise from a loaded pack
    /// — validation refuses one — and refusing is the safe arm anyway:
    /// a shape declaring no kind has said nothing, and treating silence
    /// as permission is the failure this guard exists to stop.
    pub fn holds(&self, value: &str) -> bool {
        self.0.iter().any(|kind| kind.holds(value))
    }

    /// "an amount", "an amount or a percentage" — what the pack said it
    /// expected, in the words the refusal is written in.
    pub fn in_words(&self) -> String {
        let words: Vec<&str> = self.0.iter().map(|kind| kind.in_words()).collect();
        match words.as_slice() {
            [] => "nothing".to_owned(),
            [only] => (*only).to_owned(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// "65%", "7.5 per cent" — a number and a proportion marker, nothing
/// else. As strict as [`amount`], and for the same reason.
fn percentage(value: &str) -> bool {
    let number = value
        .strip_suffix('%')
        .or_else(|| value.strip_suffix("per cent"))
        .or_else(|| value.strip_suffix("percent"));
    number.is_some_and(|number| Decimal::from_str(number.trim()).is_ok())
}

/// "14 days", "12 months" — a number and a unit of time. A date range
/// is not one, which is the case #380 was found on.
fn duration(value: &str) -> bool {
    const UNITS: &[&str] = &[
        "day", "days", "week", "weeks", "month", "months", "year", "years",
    ];
    let mut words = value.split_whitespace();
    let (Some(number), Some(unit), None) = (words.next(), words.next(), words.next()) else {
        return false;
    };
    Decimal::from_str(number).is_ok() && UNITS.contains(&unit.to_lowercase().as_str())
}

/// One term's fate across the two documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermDiff {
    pub term: String,
    pub basis: String,
    pub change: TermChange,
    /// What this row asserts, so the report never renders a read value
    /// and a computed one in the same voice (#366).
    pub kind: Kind,
    /// The passages behind this row — the earlier document's first
    /// where both exist, so a reader can check either side.
    pub quotes: Vec<Quote>,
}

/// One passage behind a diff row, keeping which document it was read
/// from (#379). Two passages with no attribution cannot be checked: a
/// row claiming a value moved shows both, and the reader has to know
/// which is last year's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    /// Which input document, indexing the run's bound inputs — the
    /// same index [`Term::document`] carries.
    pub document: usize,
    /// The passage, verbatim.
    pub text: String,
}

/// What happened to one `(term, basis)` between the two documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermChange {
    /// Both documents say the same thing. Worth reporting: a renewal
    /// where nothing moved is an answer, not an empty page.
    Unchanged { value: String },
    /// Both documents name it and they differ.
    Changed {
        from: String,
        to: String,
        /// `to − from`, when both sides parse as amounts. `None` where
        /// a value is a phrase ("14 days"): the term changed, and an
        /// invented number would be worse than no number.
        delta: Option<Decimal>,
    },
    /// Only the later document names it.
    Added { value: String },
    /// Only the earlier document names it.
    Removed { value: String },
}

/// What a diff decided: the rows it will show, and the terms it
/// refused to pair (#377).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TermDiffs {
    pub rows: Vec<TermDiff>,
    /// Terms that were read and deliberately not compared. Never
    /// silently dropped: each reaches a person with its quotes, and the
    /// report says why.
    pub not_compared: Vec<NotCompared>,
}

/// A term the diff refused to pair, because pairing it would have been
/// arbitrary (#377).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotCompared {
    pub term: String,
    pub basis: String,
    /// How many times the document that repeated it stated it — the
    /// number a person is told. The larger of the two sides, not the
    /// total: "appears 3 times in this document" is the fact, and
    /// summing both documents would overstate it.
    pub readings: usize,
    /// Every reading's quote, both documents, in document order. A
    /// person deciding which section they meant needs all of them.
    pub quotes: Vec<String>,
    /// Why it was refused. Two refusals with the same consequence and
    /// opposite causes: a reader told only "Kettle hasn't compared
    /// this" cannot tell "your schedule says it three times" from "your
    /// two documents call it different things", and those ask the
    /// person to do different things (#366 — a claim renders with its
    /// kind).
    pub why: NotComparedWhy,
}

/// Why a `(term, basis)` was not compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotComparedWhy {
    /// One of the documents states it more than once, so pairing would
    /// be a choice among readings rather than a comparison (#377).
    StatedMoreThanOnce,
    /// The two documents label the same kind of value differently, so
    /// pairing would assert either a change that may be a relabelling
    /// or a relabelling that may be a change (#461).
    ///
    /// Derived from the disagreement itself, which is a signal the run
    /// already had and used to throw away: on the 8 August v12
    /// measurement the model answered `compulsory_excess` out of one
    /// document and `total_excess` out of the other, on the identical
    /// sentence. It told us it was unsure.
    LabelledDifferently {
        /// What the earlier document called this kind of value.
        earlier: Vec<String>,
        /// What the later one called it. Disjoint from `earlier`, or
        /// there would have been no disagreement to derive.
        later: Vec<String>,
    },
}

/// Compare the terms of two documents, keyed on `(term, basis)`.
///
/// `before` and `after` are document indices, not roles: which document
/// is last year's is settled at binding time (#332), and getting it the
/// wrong way round does not fail — it silently reverses the comparison
/// and reports a price cut where there was a rise. Resolving the order
/// stays with the caller that holds the bound roles.
///
/// Terms from any other document are ignored: a run may carry more than
/// two (#330), and a third document's value joining the diff would be a
/// finding about something nobody asked to compare.
///
/// Output order is the reader's — term name, then basis — so the same
/// two documents always produce the same page.
///
/// # A term stated twice in one document is not compared (#377)
///
/// `(term, basis)` is a sufficient key for a document where each
/// modelled term occurs once, and **arbitrary** for one where the same
/// heading appears under three cover sections. This used to resolve
/// itself by taking the first reading, which is how a commercial
/// schedule produced an excess subtracted across sections and a
/// section's premium compared against the whole schedule's total —
/// both rendered as Kettle's own arithmetic.
///
/// So the rule is the repetition itself: if either document states a
/// `(term, basis)` more than once, it does not pair, and every reading
/// of it goes to a person with its quote. Deliberately *not* keyed on
/// "scope could not be derived" — scope derivation is a heuristic over
/// text shape (`Segment` carries no font size or weight), and a missed
/// heading would then silently restore the old behaviour. Keyed here,
/// a heading detector can only ever turn a referral into a comparison,
/// never a referral into a wrong number.
pub fn diff_terms(
    terms: &[Term],
    before: usize,
    after: usize,
    families: &TermFamilies,
) -> TermDiffs {
    /// One `(term, basis)`'s two sides: what the earlier document said,
    /// and what the later one did. Either may be absent — that is what
    /// added and removed are.
    type Sides<'a> = (Option<&'a Term>, Option<&'a Term>);

    // Every reading of each key, so repetition is visible before
    // anything is paired. Counted per side: "appears 3 times in this
    // document" is the fact a person is told, and pooling the two
    // documents would overstate it.
    /// Every reading of one `(term, basis)`, kept per side: what the
    /// earlier document said, and what the later one did.
    type Readings<'a> = (Vec<&'a Term>, Vec<&'a Term>);

    let mut readings: BTreeMap<(&str, &str), Readings<'_>> = BTreeMap::new();
    for term in terms {
        if term.term == OTHER {
            continue;
        }
        let entry = readings
            .entry((term.term.as_str(), term.basis.as_str()))
            .or_default();
        if term.document == before {
            entry.0.push(term);
        } else if term.document == after {
            entry.1.push(term);
        }
    }
    // #461: the two documents label the same *kind* of value
    // differently. Gathered per declared family, per side — a family
    // whose labels are disjoint across the two documents has no reading
    // that could pair with another, and every pairing available is
    // between two different labels.
    let mut by_family: BTreeMap<&str, (BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();
    for ((term, _), (earlier, later)) in &readings {
        let Some(family) = families.of(term) else {
            continue;
        };
        let entry = by_family.entry(family).or_default();
        if !earlier.is_empty() {
            entry.0.insert(term);
        }
        if !later.is_empty() {
            entry.1.insert(term);
        }
    }
    // Both sides must have said something, or there is no disagreement
    // — one document simply not mentioning an excess is an ordinary
    // added or removed term, and refusing that would hide a real
    // finding behind a referral.
    let disagreed: BTreeMap<&str, (Vec<String>, Vec<String>)> = by_family
        .iter()
        .filter(|(_, (earlier, later))| {
            !earlier.is_empty() && !later.is_empty() && earlier.is_disjoint(later)
        })
        .map(|(family, (earlier, later))| {
            let named = |side: &BTreeSet<&str>| side.iter().map(|it| (*it).to_owned()).collect();
            (*family, (named(earlier), named(later)))
        })
        .collect();

    let why = |term: &str, earlier: usize, later: usize| {
        match families.of(term).and_then(|family| disagreed.get(family)) {
            Some((was, is)) => NotComparedWhy::LabelledDifferently {
                earlier: was.clone(),
                later: is.clone(),
            },
            // Repetition is checked second on purpose: a term both
            // repeated *and* labelled differently is refused either
            // way, and the disagreement is the more surprising fact to
            // put in front of a person.
            None => {
                debug_assert!(earlier > 1 || later > 1);
                NotComparedWhy::StatedMoreThanOnce
            }
        }
    };
    let refuse = |term: &str, earlier: usize, later: usize| {
        earlier > 1
            || later > 1
            || families
                .of(term)
                .is_some_and(|family| disagreed.contains_key(family))
    };

    let not_compared: Vec<NotCompared> = readings
        .iter()
        .filter(|((term, _), (earlier, later))| refuse(term, earlier.len(), later.len()))
        .map(|((term, basis), (earlier, later))| NotCompared {
            term: (*term).to_owned(),
            basis: (*basis).to_owned(),
            readings: earlier.len().max(later.len()),
            quotes: earlier
                .iter()
                .chain(later.iter())
                .map(|term| term.quote.clone())
                .collect(),
            why: why(term, earlier.len(), later.len()),
        })
        .collect();
    let refused: std::collections::BTreeSet<(&str, &str)> = readings
        .iter()
        .filter(|((term, _), (earlier, later))| refuse(term, earlier.len(), later.len()))
        .map(|(key, _)| *key)
        .collect();

    let mut paired: BTreeMap<(&str, &str), Sides<'_>> = BTreeMap::new();
    for term in terms {
        if refused.contains(&(term.term.as_str(), term.basis.as_str())) {
            continue;
        }
        if term.term == OTHER {
            continue;
        }
        let side = if term.document == before {
            0
        } else if term.document == after {
            1
        } else {
            continue;
        };
        let entry = paired
            .entry((term.term.as_str(), term.basis.as_str()))
            .or_default();
        // Only one reading per side can reach here — a key with more was
        // refused above — so this fills a slot rather than choosing
        // between candidates. The old comment called that choice "a
        // pack-level question, not something to resolve here", and was
        // right; taking the first reading anyway is what #377 was.
        let slot = if side == 0 {
            &mut entry.0
        } else {
            &mut entry.1
        };
        slot.get_or_insert(term);
    }

    let rows = paired
        .into_iter()
        .filter_map(|((term, basis), sides)| {
            let change = match sides {
                (Some(before), Some(after)) if before.value == after.value => {
                    TermChange::Unchanged {
                        value: after.value.clone(),
                    }
                }
                (Some(before), Some(after)) => TermChange::Changed {
                    from: before.value.clone(),
                    to: after.value.clone(),
                    delta: delta(&before.value, &after.value),
                },
                (None, Some(after)) => TermChange::Added {
                    value: after.value.clone(),
                },
                (Some(before), None) => TermChange::Removed {
                    value: before.value.clone(),
                },
                // `BTreeMap` entries are only created by a term, so a
                // key with neither side cannot arise. Dropping it is
                // still the honest arm: nothing was read.
                (None, None) => return None,
            };
            let quotes = [sides.0, sides.1]
                .into_iter()
                .flatten()
                .map(|term| Quote {
                    document: term.document,
                    text: term.quote.clone(),
                })
                .collect();
            // The only row that computes anything is a change Rust
            // could subtract. A phrase change ("14 days" -> "21 days")
            // is two passages and no arithmetic, and rendering it in
            // the same voice as a delta is the flattening #366 refuses.
            let kind = match change {
                TermChange::Changed { delta: Some(_), .. } => Kind::WorkedOut,
                _ => Kind::ReadAndVerified,
            };
            Some(TermDiff {
                term: term.to_owned(),
                basis: basis.to_owned(),
                change,
                quotes,
                kind,
            })
        })
        .collect();

    TermDiffs { rows, not_compared }
}

/// `to − from` as exact decimal money, or `None` if either side is not
/// an amount this parser understands.
///
/// Deliberately strict. It reads an optional currency symbol, digits
/// with thousands separators, and nothing else — "14 days" is not an
/// amount, and a looser parser that read `14` out of it would produce a
/// delta about the wrong thing.
fn delta(from: &str, to: &str) -> Option<Decimal> {
    Some(amount(to)? - amount(from)?)
}

/// One written value as `Decimal`: "£1,234.56" -> 1234.56.
fn amount(value: &str) -> Option<Decimal> {
    let trimmed = value.trim();
    let digits: String = trimmed
        .strip_prefix(['£', '$', '€'])
        .unwrap_or(trimmed)
        .replace(',', "");
    if digits.is_empty() {
        return None;
    }
    Decimal::from_str(&digits).ok()
}
