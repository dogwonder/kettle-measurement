//! What kind of claim a report makes, so it can say so (#366).
//!
//! A report flattens four kinds of claim into one visual register, and
//! the register it picks is "settled". #348 found this on merchant
//! grouping — "the one step where every failure has occurred is the one
//! step the report presents as settled" — but it is a fact about
//! rendering, not about merchants: a date quoted off a letter and a
//! date Rust counted to arrive at a person identically, and they are
//! wrong in different ways.
//!
//! Every kind here is **derived from what the code did**, never
//! declared alongside it. A declared kind is one more assertion nobody
//! checks, which is the failure #367 exists to prevent; a derived one
//! is the code saying what it did.
//!
//! One of #366's four kinds has no variant yet. A judged label arrives
//! with the test that needs it — an unused variant is a claim nothing
//! proves. `Yours` arrived that way in #412, when a person settling a
//! disputed letter date gave it something to prove.

use serde::{Deserialize, Serialize};

/// What a rendered claim asserts, and therefore how it can be wrong.
///
/// It serialises with the report, because a report is a record: read
/// back next year it must still say which of its dates were counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Read off the page, verbatim, with the passage behind it. Wrong
    /// is a parsing bug, and a person can check it against the source.
    ReadAndVerified,
    /// Rust's own arithmetic over values that were read. Wrong is a bug
    /// with a test, never a model error — the model neither computes
    /// nor compares (CLAUDE.md).
    WorkedOut,
    /// The person's own answer, or arithmetic that depends on it
    /// (#358, #412).
    ///
    /// Reached when two readings of a photographed letter disagreed
    /// about its date and a person settled it. Every deadline counted
    /// from that date is theirs in the only sense that matters here:
    /// if one is wrong, it is wrong because their answer was, not
    /// because the arithmetic or the reading was. Calling it worked out
    /// would claim arithmetic over values read off the page, which by
    /// then is untrue.
    ///
    /// Still derived, never declared: the resolver returns this only on
    /// the branch that used a supplied date.
    Yours,
}

impl Kind {
    /// Every kind, for the tests that must cover all of them.
    ///
    /// A list beside an enum drifts from it, so [`Kind::exhaustive`]
    /// below makes the compiler hold the two together: add a variant
    /// and that match stops compiling until this list grows too. The
    /// alternative is a report that renders a new kind with another
    /// kind's words, which is the copy-layer lie #367 refuses — and it
    /// would ship green, because a test that enumerates kinds by hand
    /// never asks about the one nobody added.
    pub const ALL: &'static [Kind] = &[Kind::ReadAndVerified, Kind::WorkedOut, Kind::Yours];

    /// Exists to fail compilation, not to be called.
    #[allow(dead_code)]
    fn exhaustive(self) -> usize {
        let index = match self {
            Kind::ReadAndVerified => 0,
            Kind::WorkedOut => 1,
            Kind::Yours => 2,
        };
        debug_assert_eq!(Self::ALL[index], self);
        index
    }
}
