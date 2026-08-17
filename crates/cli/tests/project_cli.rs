//! #478: `kettle project` — the public measurement tree, generated.
//!
//! The decision of 16 August is a projection, not a second repository:
//! `crates/`, `packs/`, `evals/`, `assurance/` and the files they need to
//! build, materialised from this tree on merge, never hand-mirrored
//! (#269). The boundary is declared once, in `assurance/claims.json`'s
//! `published` list, because a registry that refuses a claim standing on
//! unpublishable evidence and a projection that decides what to publish
//! must be reading the same sentence — two lists is how the public tree
//! comes to disagree with the page describing it.
//!
//! What these tests hold is the part a reviewer cannot eyeball: that the
//! projected tree is a tree somebody can *use*. A projection that omits
//! the workspace manifest, or a file the privacy contract names, ships a
//! public repository whose own CI is red — on a project whose entire
//! credential is that its claims match its evidence.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn published_boundary() -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join("assurance/claims.json"))
        .expect("the registry is readable");
    runner::assurance::Registry::from_json(&text)
        .expect("the registry parses")
        .published
}

fn inside_boundary(path: &str, published: &[String]) -> bool {
    published.iter().any(|prefix| path.starts_with(prefix))
}

/// The privacy contract is published, and it names the files it declares
/// a network path in. Two of them sit in `app/src-tauri/` — the Tauri
/// configuration and the model manifest — and `app/` is otherwise closed.
///
/// So `every_declared_path_still_exists`, which is the boundary test that
/// rots quietly and the reason it exists, fails in the projected tree
/// while passing here. A public repository that cannot run the privacy
/// audit it ships is the worst possible place for that gap: the audit is
/// the evidence for `no-non-loopback-network`.
///
/// The fix is to publish those two files, not to stop declaring them.
/// They are configuration the measurement layer reads — `kettle scores`
/// resolves the model manifest out of `app/src-tauri/models.json` — and
/// publishing a manifest is not publishing the product surface.
#[test]
fn every_file_the_privacy_contract_names_is_inside_the_published_boundary() {
    let published = published_boundary();
    let contract: toml::Value = toml::from_str(
        &std::fs::read_to_string(repo_root().join("privacy-boundary.toml"))
            .expect("the privacy contract is readable"),
    )
    .expect("the privacy contract is TOML");

    let declared: Vec<String> = contract["path"]
        .as_array()
        .expect("the contract declares paths")
        .iter()
        .filter_map(|path| path["file"].as_str())
        .map(str::to_owned)
        .collect();
    assert!(
        declared.len() >= 4,
        "the contract declares paths to check against"
    );

    let outside: Vec<&String> = declared
        .iter()
        .filter(|file| !inside_boundary(file, &published))
        .collect();

    assert!(
        outside.is_empty(),
        "the projected tree publishes privacy-boundary.toml but not the \
         file(s) it declares, so its own boundary test fails there: {outside:?}"
    );
}

/// A projected tree whose root manifest names a crate it does not carry
/// does not `cargo build`, let alone `cargo test` — and "inspectable, and
/// re-runnable given the weights" is the wording the decision committed
/// to. Cheap to assert here; expensive to discover on the first public
/// clone.
#[test]
fn the_projected_workspace_carries_every_crate_its_manifest_names() {
    let published = published_boundary();
    let manifest: toml::Value = toml::from_str(
        &std::fs::read_to_string(repo_root().join("Cargo.toml"))
            .expect("the workspace manifest is readable"),
    )
    .expect("the workspace manifest is TOML");

    assert!(
        inside_boundary("Cargo.toml", &published),
        "the projection omits the workspace manifest, so the published \
         crates are not a workspace"
    );
    assert!(
        inside_boundary("Cargo.lock", &published),
        "the projection omits the lock file, so a reader re-running a \
         measurement resolves different dependencies than we measured on"
    );

    let members: Vec<String> = manifest["workspace"]["members"]
        .as_array()
        .expect("the workspace names its members")
        .iter()
        .filter_map(|member| member.as_str())
        .map(str::to_owned)
        .collect();

    let missing: Vec<&String> = members
        .iter()
        .filter(|member| !inside_boundary(member, &published))
        .collect();
    assert!(
        missing.is_empty(),
        "the workspace manifest names crates the projection does not \
         publish: {missing:?}"
    );
}

/// The licence is the instrument the decision chose, so it travels with
/// the code it licenses. A public repository with no LICENSE is not
/// Apache-2.0 licensed by virtue of a private tree saying so.
#[test]
fn the_projection_carries_the_licence_it_is_published_under() {
    let published = published_boundary();
    for file in ["LICENSE", "NOTICE"] {
        assert!(
            inside_boundary(file, &published),
            "{file} sits outside the published boundary, so the projected \
             tree carries code under no stated terms"
        );
    }
}

/// The selection is a pure function of what the repository *tracks* and
/// what the boundary declares. Tracked, because that is the only list
/// that cannot contain a `*.private.*` file or a stray model: the data
/// rules keep them out of git, so a projection built from git inherits
/// the guarantee instead of re-deriving it.
#[test]
fn only_tracked_files_inside_the_boundary_are_selected() {
    let tracked = vec![
        "crates/runner/src/lib.rs".to_owned(),
        "packs/app.kttl.renewal-diff/pack.json".to_owned(),
        "assurance/claims.json".to_owned(),
        "app/src/App.svelte".to_owned(),
        "reference/design/tokens.css".to_owned(),
        "CLAUDE.md".to_owned(),
    ];
    let published = vec![
        "crates/".to_owned(),
        "packs/".to_owned(),
        "assurance/".to_owned(),
    ];

    let selected = cli::project::selected(&tracked, &published);

    assert_eq!(
        selected,
        vec![
            "assurance/claims.json",
            "crates/runner/src/lib.rs",
            "packs/app.kttl.renewal-diff/pack.json",
        ],
        "the selection is the boundary's intersection with the tracked \
         tree, in a stable order"
    );
}

/// A projection cannot invent a path. Anything it emits came from the
/// tracked list, so the only way a private file reaches the public tree
/// is by being committed here first — which the data rules already
/// govern, and `.gitignore` already enforces.
#[test]
fn the_selection_never_emits_a_path_the_tree_does_not_track() {
    let tracked = vec!["crates/runner/src/lib.rs".to_owned()];
    let published = vec!["crates/".to_owned(), "evals/".to_owned()];

    let selected = cli::project::selected(&tracked, &published);

    assert!(
        selected
            .iter()
            .all(|path| tracked.iter().any(|t| t == path)),
        "the selection emitted a path the tree does not track: {selected:?}"
    );
    assert_eq!(
        selected,
        vec!["crates/runner/src/lib.rs"],
        "a declared prefix matching nothing tracked contributes nothing"
    );
}

/// The exam bed is published (decided 17 August 2026). It was never
/// withheldable: deleting all 180 `generated-exam-*` files from the
/// renewal pack and running `kettle bed` restores them byte-identically,
/// because the generator lives in `crates/runner/src/eval/letters.rs` and
/// the set declarations live in the committed bed specs — both inside the
/// boundary. Withholding the files while publishing the one command that
/// regenerates them is theatre a reader can discover, which is the same
/// finding that settled the prompts: either the bed is public or its
/// generator is not.
///
/// So this asserts the projection does *not* carve the exam set out —
/// pinned as a test because "publish it" is the kind of decision a later
/// tidy-up reverses on instinct.
#[test]
fn the_exam_bed_is_published_because_its_generator_is() {
    let tracked = vec![
        "packs/app.kttl.renewal-diff/fixtures/generated-development-basis_changed-amber-01.expected.json".to_owned(),
        "packs/app.kttl.renewal-diff/fixtures/generated-exam-basis_changed-alpine-lake-01.expected.json".to_owned(),
        "packs/app.kttl.renewal-diff/fixtures/renewal-bed-spec.json".to_owned(),
    ];
    let published = vec!["packs/".to_owned()];

    let selected = cli::project::selected(&tracked, &published);

    assert_eq!(
        selected.len(),
        3,
        "the projection carved out a set the bed spec regenerates: {selected:?}"
    );
    assert!(
        selected.iter().any(|path| path.contains("generated-exam-")),
        "the exam bed is missing from a tree that publishes the generator \
         and the spec, which withholds nothing and looks like curation"
    );
}

/// The projection carries a `.gitignore`, and the reason is a defect it
/// would have prevented.
///
/// The first publish shipped a 47MB `kettle` binary and all of
/// `target/debug`, because the verification step built *inside* the
/// projection and the publish step then copied the directory wholesale.
/// The workflows now build elsewhere, which is the real fix; this is the
/// second line of defence, because `git add -A` in a tree with no
/// `.gitignore` takes whatever it finds.
///
/// One-way, which is what earns a belt as well as braces: a blob pushed to
/// a repository about to go public stays fetchable from its history long
/// after a commit removes it.
#[test]
fn the_projection_carries_a_gitignore_so_stray_build_output_cannot_enter() {
    assert!(
        inside_boundary(".gitignore", &published_boundary()),
        "the projected tree has no .gitignore, so anything built inside it \
         is one `git add -A` away from being published"
    );
}
