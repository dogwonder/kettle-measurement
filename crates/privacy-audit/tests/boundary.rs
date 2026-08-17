//! #233: the network boundary Kettle claims is the one its source can
//! reach.

use privacy_audit::{call_sites, declared, PathKind};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn all_network_paths_are_declared_by_the_privacy_contract() {
    // The guard itself. Every network call site in source must appear
    // in `privacy-boundary.toml`, with a reason and a date — so adding
    // one is a deliberate, reviewable act rather than a dependency
    // bump nobody read.
    let root = repo_root();
    let declared = declared(&root).expect("privacy-boundary.toml parses");

    let mut undeclared = Vec::new();
    for site in call_sites(&root) {
        let path = site.file.to_string_lossy().replace('\\', "/");
        let known = declared
            .iter()
            .any(|d| path.ends_with(&d.file) && site.calls.contains(&d.calls));
        if !known {
            undeclared.push(format!("{}:{} — {}", path, site.line, site.calls));
        }
    }

    assert!(
        undeclared.is_empty(),
        "undeclared network paths — add them to privacy-boundary.toml \
         with a reason, or remove them:\n  {}",
        undeclared.join("\n  ")
    );
}

#[test]
fn every_declared_path_still_exists() {
    // The other direction, and the one that rots quietly. A boundary
    // file listing paths that were deleted years ago reads as though
    // Kettle reaches more of the network than it does, and a reviewer
    // who finds one stale entry stops trusting the rest of the file.
    let root = repo_root();
    let sites = call_sites(&root);

    let stale: Vec<String> = declared(&root)
        .expect("privacy-boundary.toml parses")
        .iter()
        .filter(|d| {
            !sites.iter().any(|site| {
                site.file
                    .to_string_lossy()
                    .replace('\\', "/")
                    .ends_with(&d.file)
                    && site.calls.contains(&d.calls)
            })
        })
        .map(|d| format!("{} — {}", d.file, d.calls))
        .collect();

    assert!(
        stale.is_empty(),
        "privacy-boundary.toml declares paths that no longer exist: {stale:?}"
    );
}

#[test]
fn nothing_reaches_the_network_except_loopback_and_an_explicit_download() {
    // The contract in one sentence, asserted rather than described.
    // Anything else — telemetry, crash reporting, update checks, remote
    // fonts or report assets — needs its own designed and disclosed
    // issue before a `PathKind` exists for it, and this test is what
    // makes adding one impossible to do quietly.
    for path in declared(&repo_root()).expect("privacy-boundary.toml parses") {
        assert!(
            matches!(
                path.kind,
                PathKind::Loopback | PathKind::ExplicitModelDownload
            ),
            "{} declares a path that is neither loopback nor an explicit \
             model download",
            path.file
        );
    }
}
