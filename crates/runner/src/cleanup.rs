//! Deterministic merchant cleanup and grouping, applied before the
//! normalise model step ever sees a name: strip payment-processor
//! prefixes so the model only handles the genuine merchant residue, and
//! decide which raw descriptors are the same merchant.
//!
//! Grouping fails toward *apart*. Two merchants wrongly merged pool
//! their payments and fabricate each other's cadences — one rent came
//! out as five series at three cadences nobody pays (#261). Two
//! descriptors wrongly left apart only cost a second review item. An
//! invented series is an invented deadline; a duplicate review item is
//! a duplicate review item.

use crate::parse::Transaction;

/// One merchant's transactions, after cleaning and variant merging.
/// This is what the pipeline groups by, and — since #261 — what the
/// exactness tests group by too: a test that grouped differently from
/// the runner was checking a pipeline nobody runs.
#[derive(Debug, Clone)]
pub struct MerchantGroup {
    /// Deterministically cleaned representative (what the model is asked
    /// about).
    pub cleaned: String,
    /// First raw string seen — kept so review items and findings can
    /// point back at the statement.
    pub raw_first: String,
    /// Every transaction in the group, oldest first.
    pub txns: Vec<Transaction>,
}

/// Group transactions by cleaned merchant, then merge name variants
/// (store numbers, till codes) with `group_merchants`.
pub fn group_transactions(txns: &[Transaction]) -> Vec<MerchantGroup> {
    let mut named: Vec<(String, String, Vec<Transaction>)> = Vec::new();
    for txn in txns {
        let cleaned = clean_merchant(&txn.raw_merchant);
        match named.iter_mut().find(|(name, ..)| *name == cleaned) {
            Some((.., list)) => list.push(txn.clone()),
            None => named.push((cleaned, txn.raw_merchant.clone(), vec![txn.clone()])),
        }
    }

    let names: Vec<&str> = named.iter().map(|(name, ..)| name.as_str()).collect();
    group_merchants(&names)
        .into_iter()
        .map(|members| {
            let (cleaned, raw_first, _) = &named[members[0]];
            let mut txns: Vec<Transaction> = members
                .iter()
                .flat_map(|&member| named[member].2.iter().cloned())
                .collect();
            txns.sort_by_key(|txn| txn.date);
            MerchantGroup {
                cleaned: cleaned.clone(),
                raw_first: raw_first.clone(),
                txns,
            }
        })
        .collect()
}

/// Group merchant-name variants so the model normalises one
/// representative per group, not every raw string. Greedy and
/// order-stable: each name joins the first group with the same stem,
/// else starts a new one.
///
/// A variant is the same merchant *words* plus a machine code — a store
/// number, a till id. That is the only difference allowed, and it is
/// checked exactly. String similarity was tried and withdrawn: it
/// scored on shared characters, so `ALDERRENT`, `ALDERGROCER` and
/// `ALDERMARKET` came out as one merchant (#261).
pub fn group_merchants(names: &[&str]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let stem = stem_of(name);
        match groups.iter_mut().find(|(known, _)| *known == stem) {
            Some((_, group)) => group.push(index),
            None => groups.push((stem, vec![index])),
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

/// The merchant words of a name, upper-cased, with branch and till
/// codes dropped: what has to match exactly for two descriptors to be
/// one merchant. A name that is *all* code keeps every token — `O2` is
/// a merchant, and an empty stem would swallow every other one.
fn stem_of(name: &str) -> String {
    let words: Vec<String> = name
        .split_whitespace()
        .filter(|token| !is_code(token))
        .map(|token| token.to_uppercase())
        .collect();
    if words.is_empty() {
        return name.to_uppercase();
    }
    words.join(" ")
}

/// A machine code rather than a merchant word: a token that is digits,
/// or digits with a short letter tag. Real statements append store
/// numbers (`3412`), till ids (`T02`) and reference stubs; none of them
/// identify the merchant, and all of them differ between rows of the
/// same one.
fn is_code(token: &str) -> bool {
    let digits = token.chars().filter(char::is_ascii_digit).count();
    digits > 0 && token.chars().count() - digits <= 2
}

/// Payment-processor tokens that precede a `*` and hide the real
/// merchant. Small and stable; grow it as real statements surface new
/// ones. Matched as a whole token against whatever sits left of the
/// first `*`, so spacing and underscores stop mattering — `STRIPE* X`,
/// `SQ *X` and `ZETTLE_*X` are one shape, where before only two of the
/// three cleaned and the third split the merchant in half (#261).
const PSP_TOKENS: &[&str] = &["PAYPAL", "SQ", "ZETTLE", "SUMUP", "IZ", "STRIPE", "WPY"];

/// Prefixes where the remainder is an order code, not a merchant — the
/// whole string maps to one canonical name.
const CANONICAL_PREFIXES: &[(&str, &str)] = &[("AMZNMktplace", "Amazon Marketplace")];

pub fn clean_merchant(raw: &str) -> String {
    let trimmed = raw.trim();
    for (prefix, canonical) in CANONICAL_PREFIXES {
        if trimmed.starts_with(prefix) {
            return (*canonical).to_owned();
        }
    }
    if let Some((before, after)) = trimmed.split_once('*') {
        let token = before.trim_matches(|c: char| c.is_whitespace() || c == '_');
        if PSP_TOKENS.contains(&token.to_uppercase().as_str()) {
            return after.trim().to_owned();
        }
    }
    trimmed.to_owned()
}
