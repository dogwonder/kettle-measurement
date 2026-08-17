//! What a payment *is*, decided by how it behaves (#253).
//!
//! The model used to answer `kind`, and the Stage 3 bed showed what
//! that cost: whether a gym charge is a membership or a one-off class
//! is a fact about cadence, and the model only ever saw the merchant's
//! name. 92 of its 96 confident-wrong answers sat in exactly that gap.
//! So `kind` moved here — arithmetic over facts the pipeline already
//! establishes — and the model's question shrank to the one thing a
//! name can support: what sort of merchant this is (`category`), with
//! "unknown" as a first-class honest answer.
//!
//! The category→kind mapping for recurring series is **pack data**
//! (`pack.json` `kinds`), because "recurring housing money is a bill,
//! recurring streaming money is a subscription" is pack policy — a
//! runner hard-coding it would be pack-specific runner code, which is
//! the thing #51 refuses. Load-time validation holds the map complete
//! against the classify schema's category enum, so the lookups here
//! cannot miss for a well-formed pack.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which decision produced a `kind` (#272).
///
/// Written down where the branch is taken, because the branch *is* the
/// cause and inferring it afterwards does not work: `recurring_kind`
/// returns `regular_spend` for a detected series in a `retail` or
/// `food_drink` category, and [`spend_kind`] returns the same string
/// for three scattered debit days. Reading the kind cannot tell those
/// apart, which is how the #237 confident-wrong cell was attributed
/// wrongly twice — first as 91% cadence, actually 90% category.
///
/// Deliberately not a model's explanation of a failure (#258): the
/// pipeline took a branch, and a branch is a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindFrom {
    /// A recurring series was detected, so the pack's category→kind map
    /// decided what it is. The model's category is the input, which is
    /// why an error here is a *category* error.
    CategoryMap,
    /// No series: the kind came from counting distinct debit days.
    /// Cadence decided, and the model's category had no say.
    Cadence,
    /// No series, but the payments looked periodic — declined, and
    /// surfaced for a person because of it (#271). Kept apart from
    /// [`KindFrom::Cadence`] because "cadence declined confidently" and
    /// "cadence declined something that looked like a series" are
    /// different failures with different fixes.
    CadenceDespitePeriodic,
    /// A detected income series. Money coming in is never a spending
    /// kind, whatever the category says.
    Income,
}

/// The kind of a merchant with a detected recurring series, from the
/// pack's category→kind map.
///
/// The fallback chain is a fail-safe, not a path: validation makes a
/// missing category unreachable, but if it ever happens the answer is
/// what the pack said about `unknown` — surface it as the audit's
/// subject — never a kind nobody wrote down.
pub fn recurring_kind(map: &BTreeMap<String, String>, category: &str) -> String {
    map.get(category)
        .or_else(|| map.get("unknown"))
        .cloned()
        .unwrap_or_else(|| "subscription".to_owned())
}

/// The kind of a merchant with no recurring series, from its debit
/// behaviour: three or more distinct days is a habit (the weekly shop,
/// the coffee run), fewer is a one-off — including a duplicate charge,
/// which is one purchase however many rows it produced.
pub fn spend_kind(distinct_debit_days: usize) -> &'static str {
    if distinct_debit_days >= 3 {
        "regular_spend"
    } else {
        "one_off"
    }
}
