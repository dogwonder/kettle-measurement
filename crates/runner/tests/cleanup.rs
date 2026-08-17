use runner::cleanup::{clean_merchant, group_merchants};

#[test]
fn psp_prefixes_are_stripped() {
    assert_eq!(clean_merchant("PAYPAL *DISNEYPLUS"), "DISNEYPLUS");
    assert_eq!(clean_merchant("SQ *KAFFA COFFEE"), "KAFFA COFFEE");
    assert_eq!(clean_merchant("ZETTLE_*COFFEECART"), "COFFEECART");

    // A non-PSP merchant passes through untouched.
    assert_eq!(clean_merchant("BRITISH GAS"), "BRITISH GAS");
}

#[test]
fn a_processor_is_stripped_whichever_side_of_the_star_it_spaces() {
    // #261: the same merchant reached through three processors must clean
    // to one name. `STRIPE* X` survived intact while `SQ *X` cleaned, so
    // one merchant split into two groups before grouping proper began.
    for raw in [
        "STRIPE* ALDERRENT",
        "SQ *ALDERRENT",
        "PAYPAL *ALDERRENT",
        "STRIPE*ALDERRENT",
        "SUMUP  *ALDERRENT",
    ] {
        assert_eq!(clean_merchant(raw), "ALDERRENT", "{raw}");
    }
}

#[test]
fn a_star_that_is_not_a_processor_is_left_alone() {
    // Only names whose leading token is a known processor are split on
    // the star — an unknown one is merchant text, not a prefix.
    assert_eq!(clean_merchant("ODEON*CINEMA 21"), "ODEON*CINEMA 21");
}

#[test]
fn amazon_marketplace_codes_are_canonicalised() {
    // The * suffix is an order code, not a merchant — stripping it would
    // leave gibberish, so the whole thing maps to one canonical name.
    assert_eq!(clean_merchant("AMZNMktplace*2K4J"), "Amazon Marketplace");
    assert_eq!(clean_merchant("AMZNMktplace*9QQ7"), "Amazon Marketplace");
}

#[test]
fn merchant_variants_group_together() {
    let names = [
        "TESCO STORES 3412",
        "TESCO STORES 1101",
        "NETFLIX.COM",
        "TESCO STORES 0042",
    ];
    // Greedy and order-stable: first-seen name is the group representative.
    assert_eq!(group_merchants(&names), vec![vec![0, 1, 3], vec![2]]);
}

#[test]
fn distinct_merchants_sharing_a_suffix_stay_apart() {
    let names = ["PUREGYM LTD", "SPOTIFY LTD"];
    assert_eq!(group_merchants(&names), vec![vec![0], vec![1]]);
}

#[test]
fn distinct_merchants_sharing_a_prefix_stay_apart() {
    // #261: Jaro-Winkler weights a shared prefix heavily, so these four
    // scored above 0.85 against each other and collapsed into one group.
    // Their payments then fabricated each other's cadences — one rent
    // became five series at three cadences nobody pays. Over-grouping
    // invents; under-grouping only costs review, so grouping fails that
    // way instead.
    let names = [
        "ALDERRENT",
        "ALDERGROCER",
        "ALDERMARKET",
        "ALDERSEASONTICKET",
    ];
    assert_eq!(
        group_merchants(&names),
        vec![vec![0], vec![1], vec![2], vec![3]]
    );

    let phrases = ["ALDER ENERGY PAYMENT", "ALDER PAYROLL PAYMENT"];
    assert_eq!(group_merchants(&phrases), vec![vec![0], vec![1]]);
}

#[test]
fn only_branch_codes_may_differ_within_a_group() {
    // What a variant *is*: the same merchant words, plus a store number
    // or till code. Different words are a different merchant, however
    // similar they look.
    let names = [
        "TESCO STORES 3412",
        "TESCO STORES",
        "TESCO EXPRESS 3412",
        "tesco stores 0042",
    ];
    assert_eq!(group_merchants(&names), vec![vec![0, 1, 3], vec![2]]);
}
