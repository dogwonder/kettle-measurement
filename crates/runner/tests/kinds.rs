//! What a payment *is* — decided by how it behaves, never by the model
//! (#253). The model was being scored on whether a merchant name is a
//! subscription, when subscription-ness is a fact about cadence it was
//! never shown. These tests pin the deterministic derivation that
//! replaces that question.

use runner::kinds::{recurring_kind, spend_kind};
use std::collections::BTreeMap;

fn map() -> BTreeMap<String, String> {
    [
        ("streaming", "subscription"),
        ("housing", "utility"),
        ("finance", "regular_spend"),
        ("unknown", "subscription"),
    ]
    .into_iter()
    .map(|(category, kind)| (category.to_owned(), kind.to_owned()))
    .collect()
}

#[test]
fn a_recurring_series_takes_its_kind_from_the_packs_category_map() {
    // The mapping is pack policy, carried as pack data: rent recurring
    // monthly is a bill, Netflix recurring monthly is a subscription,
    // and a monthly standing order to a person is regular spending.
    // The runner only looks it up.
    let map = map();
    assert_eq!(recurring_kind(&map, "streaming"), "subscription");
    assert_eq!(recurring_kind(&map, "housing"), "utility");
    assert_eq!(recurring_kind(&map, "finance"), "regular_spend");
}

#[test]
fn an_unidentified_recurring_payment_is_the_audits_subject_not_a_gap() {
    // The model answering "unknown" is honesty, not failure. A payment
    // that recurs and cannot be named is exactly what a subscription
    // audit exists to put in front of a person — the pack maps it to
    // subscription, and the forced-low confidence (run.rs) puts it in
    // "check these yourself".
    assert_eq!(recurring_kind(&map(), "unknown"), "subscription");
}

#[test]
fn a_recurring_payment_the_pack_could_not_place_is_surfaced_however_it_was_said() {
    // #302. `unknown` and `other` are the same statement — "this name
    // did not tell me what the merchant is" — and the classify prompt
    // offers only one of them, so which one comes back is the model's
    // word choice. The shipped pack sent them opposite ways:
    // `unknown` to subscription (surfaced, the audit's subject) and
    // `other` to regular_spend (silently not a subscription).
    //
    // 21 of the 63 subscriptions the 7B confidently denied were
    // `software -> other`: Backblaze, Pcloud, iDrive — recurring
    // payments a person is making, filed as ordinary spending because
    // the model reached for the residual category Kettle treats as
    // safe. A residual category cannot be safe. It is the one place a
    // subscription hides.
    let pack = runner::packs::load_pack(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packs/app.kttl.subscription-audit"),
    )
    .expect("the shipped pack loads");

    for residual in ["unknown", "other"] {
        assert_eq!(
            recurring_kind(&pack.manifest.kinds, residual),
            "subscription",
            "a recurring payment to a merchant the pack could not place ({residual}) must \
             reach a person, not be filed as ordinary spending"
        );
    }
}

#[test]
fn a_category_the_map_does_not_name_falls_back_to_the_unknown_entry() {
    // Load-time validation makes this unreachable for a well-formed
    // pack (the map must cover the schema's enum), so this is the
    // fail-safe, not a path: fall to what the pack said about unknown,
    // never invent a kind the pack didn't write.
    let mut partial = map();
    partial.remove("housing");
    assert_eq!(recurring_kind(&partial, "housing"), "subscription");
}

#[test]
fn repeated_irregular_spending_is_regular_spend_and_a_single_payment_is_one_off() {
    // The weekly shop: many debits, amounts that never repeat exactly,
    // so no series — but three or more distinct days is a habit.
    assert_eq!(spend_kind(12), "regular_spend");
    assert_eq!(spend_kind(3), "regular_spend");
    // One payment, or a duplicate charge on one or two days, is not.
    assert_eq!(spend_kind(1), "one_off");
    assert_eq!(spend_kind(2), "one_off");
    assert_eq!(spend_kind(0), "one_off");
}
