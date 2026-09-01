//! A withdrawn pack (#545) is measured and never offered: absent from
//! the public `packs --json` the website reads, and named as withdrawn
//! — with its date, reason and record — by `packs list`.

use std::path::PathBuf;

fn packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .canonicalize()
        .expect("the repo's packs directory")
}

#[test]
fn the_public_projection_omits_a_withdrawn_pack() {
    let outcome = cli::packs::run_json(&packs_dir());
    assert_eq!(outcome.code, cli::packs::ExitCode::Ok, "{}", outcome.text);
    let document: serde_json::Value = serde_json::from_str(&outcome.text).expect("json");
    let ids: Vec<&str> = document["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .map(|p| p["id"].as_str().expect("id"))
        .collect();
    assert!(ids.contains(&"app.kttl.letter-to-actions"), "{ids:?}");
    assert!(ids.contains(&"app.kttl.renewal-diff"), "{ids:?}");
    assert!(
        !ids.contains(&"app.kttl.subscription-audit"),
        "a public page must not describe a task the app does not offer: {ids:?}"
    );
}

#[test]
fn packs_list_names_the_withdrawal() {
    let outcome = cli::packs::run_list(&packs_dir());
    assert!(
        outcome.contains("app.kttl.subscription-audit"),
        "the lab still lists it: {outcome}"
    );
    assert!(
        outcome.contains("withdrawn 2026-08-30") && outcome.contains("#545"),
        "and says when, why and where it was decided: {outcome}"
    );
}

#[test]
fn the_public_projection_carries_each_packs_goal_verbatim() {
    let outcome = cli::packs::run_json(&packs_dir());
    assert_eq!(outcome.code, cli::packs::ExitCode::Ok, "{}", outcome.text);
    let document: serde_json::Value = serde_json::from_str(&outcome.text).expect("json");
    for pack in document["packs"].as_array().expect("packs") {
        let goal = &pack["goal"];
        for field in ["who", "can", "done_when"] {
            assert!(
                goal[field].as_str().is_some_and(|s| !s.trim().is_empty()),
                "{} publishes its goal's `{field}`: {goal}",
                pack["id"]
            );
        }
    }
}
