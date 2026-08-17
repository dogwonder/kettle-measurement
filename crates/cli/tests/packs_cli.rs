//! #478: the public "how it works" page answers "which questions can
//! this document answer?" from the pack manifests themselves.
//!
//! The essay's note on the page warned that a hand-written table there
//! is stale the next time a pack changes. So the page owns no table:
//! it renders this projection, which comes through the same loader a
//! run does. A pack that would not load cannot be advertised, and a
//! pack whose declared inputs change moves the page without anyone
//! editing copy.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn packs_are_a_validated_projection_of_their_manifests_not_page_copy() {
    let outcome = cli::packs::run_json(&repo_root().join("packs"));
    assert_eq!(outcome.code, cli::packs::ExitCode::Ok, "{}", outcome.text);

    let document: serde_json::Value = serde_json::from_str(&outcome.text).expect("packs JSON");
    assert_eq!(document["schema"], "kettle/public-packs@0");

    let packs = document["packs"].as_array().expect("packs array");
    assert!(
        packs.len() >= 3,
        "the shipped packs should all project: {}",
        outcome.text
    );

    let renewal = packs
        .iter()
        .find(|pack| pack["id"] == "app.kttl.renewal-diff")
        .expect("renewal pack projects");
    assert_eq!(renewal["name"], "Renewal diff");
    // The question the pack answers, in the pack's own words. The page
    // must not paraphrase it into marketing copy.
    assert!(
        renewal["description"]
            .as_str()
            .expect("description")
            .contains("changed"),
        "{}",
        renewal["description"]
    );

    // Two named documents, each with its own label, types and
    // cardinality (#334). Flattening these is exactly the mistake
    // `packs list` was fixed for: it hid that the pack wants two.
    let inputs = renewal["inputs"].as_array().expect("inputs");
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0]["role"], "previous");
    assert_eq!(inputs[0]["label"], "Last year's policy");
    assert_eq!(inputs[1]["role"], "renewal");
    assert!(inputs[0]["accept"]
        .as_array()
        .expect("accept")
        .iter()
        .any(|kind| kind == "application/pdf"));
    assert_eq!(inputs[0]["count"], "one file");

    // What the pack may do, so the page's read-only promise is the
    // manifest's rather than a sentence someone typed.
    assert_eq!(
        renewal["capabilities"].as_array().expect("capabilities"),
        &vec![serde_json::Value::from("read")]
    );
}

#[test]
fn a_pack_that_would_not_load_stops_the_public_projection() {
    let dir = std::env::temp_dir().join(format!("kettle-packs-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("broken-pack")).expect("create pack dir");
    std::fs::write(dir.join("broken-pack/pack.json"), "{ not json").expect("write manifest");

    let outcome = cli::packs::run_json(&dir);
    assert_eq!(outcome.code, cli::packs::ExitCode::Broken);
    assert!(
        outcome.text.contains("broken-pack"),
        "the refusal names the pack: {}",
        outcome.text
    );
}
