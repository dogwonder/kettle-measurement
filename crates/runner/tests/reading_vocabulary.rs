//! What Kettle can read off a page, stated as a table (#399, #595).
//!
//! Every bed in this repository was authored beside the parser it
//! exercises, by the same hand, in the same sitting — so the set of
//! phrases the bed contains and the set the parser accepts are the same
//! set, and a measurement between two things that coincide by
//! construction discovers nothing. This table is the other instrument:
//! no model, no GPU, no fixtures. A list of the surface forms a British
//! letter actually uses, each with what Kettle makes of it — and a
//! **refusal is a first-class entry**, because some forms Kettle should
//! not read, and saying so is what stops a later widening from quietly
//! making Kettle guess.
//!
//! The table lives in `evals/capabilities/date-forms.json` so the
//! capability inventory and this test read one list. Since scoring 19
//! (#628) nothing finds a phrase: the model reads it into structure
//! (`count`, `unit`, `qualifier`, `counts_from`) and Rust checks that
//! structure against the words and counts from a base the model read.
//! So each row carries the structure a correct reading gives, and the
//! boundary under test is `timeline::resolve_structured` — the same
//! function the runtime calls — never a parser of prose.
//!
//! Add a row when a real letter shows a form this table lacks. Never
//! delete one to make a change pass.

use chrono::NaiveDate;
use runner::claim::Kind;
use runner::run::When;

#[test]
fn every_surface_form_reads_as_the_table_says() {
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../../../evals/capabilities/date-forms.json"))
            .expect("date form inventory");
    let mut wrong = Vec::new();
    let mut checked = 0;
    for case in inventory["cases"].as_array().expect("cases") {
        let phrase = case["phrase"].as_str().expect("a phrase");
        let verifier = &case["verifier"];
        let read: When =
            serde_json::from_value(verifier["proposal"].clone()).expect("a structure proposal");
        let pointed = verifier["pointed"].as_bool().unwrap_or(false);
        // The base the model would have read for this phrase: the
        // letter's own date where the period counts from it, and
        // nothing where the words name their own day or refuse one.
        let base: Option<NaiveDate> = match read.counts_from.as_str() {
            "letter_date" | "month_end" => verifier["base_date"]
                .as_str()
                .map(|d| d.parse().expect("the inventory's dates parse")),
            _ => None,
        };
        let got =
            runner::timeline::resolve_structured(phrase, &read, pointed, base, Kind::WorkedOut)
                .ok()
                .map(|resolved| resolved.date.to_string());
        let want = case["source_truth"]["date"].as_str().map(str::to_owned);
        checked += 1;
        if got != want {
            wrong.push(format!(
                "{}: {phrase:?} with {:?} expected {want:?}, got {got:?}",
                case["id"], read
            ));
        }
    }
    assert!(checked >= 33, "the table has shrunk to {checked} rows");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
