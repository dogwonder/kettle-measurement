//! `kettle project` — the public measurement tree, generated (#478).
//!
//! The 16 August decision published the measurement layer and kept the
//! product surface back. This is the mechanism, and it is deliberately a
//! *projection*: the public repository is materialised from this tree, so
//! there is never a second copy of a crate, a fixture or a prompt to
//! drift (#269, the same rule that keeps the demo building from here).
//!
//! Two properties are worth more than the copying:
//!
//! **The boundary is declared once.** `assurance/claims.json`'s
//! `published` list is what the registry already validates against — a
//! proven claim whose only evidence sits outside it is refused (#516). If
//! the projection read a second list, the registry could refuse a claim
//! for being unpublishable while the projection published it anyway, and
//! the page describing the tree would be describing a different tree.
//!
//! **The selection comes from what git tracks.** That is the only list
//! which cannot hold a `*.private.*` document or a `.gguf`: the data
//! rules keep them untracked, so a projection built from `git ls-files`
//! inherits that guarantee rather than re-deriving it in a second place.
//! The listing is read at the edge and passed in, like every clock and
//! version in this crate, so the selection stays a function a test can
//! hold still.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    Broken = 2,
}

pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

fn broken(text: String) -> Outcome {
    Outcome {
        text,
        code: ExitCode::Broken,
    }
}

/// The tracked paths the declared boundary publishes, sorted.
///
/// Prefix matching, and the same `starts_with` the registry validates
/// evidence with (`assurance::Evidence::publicly_reachable`) — one
/// sentence, read the same way in both places. Sorted because a
/// projection whose file order wanders produces a different commit for an
/// identical tree, and a public repository's history should record
/// changes rather than iteration order.
pub fn selected<'a>(tracked: &'a [String], published: &[String]) -> Vec<&'a str> {
    let mut selected: Vec<&str> = tracked
        .iter()
        .map(String::as_str)
        .filter(|path| published.iter().any(|prefix| path.starts_with(prefix)))
        .collect();
    selected.sort_unstable();
    selected
}

/// The boundary as declared, or a refusal naming why it cannot be read.
///
/// An undeclared boundary is a refusal here, where it constrains nothing
/// in the registry (validation treats an absent list as "not being
/// published"). The asymmetry is deliberate: validating a tree nobody is
/// publishing is routine, and *publishing* one whose boundary is
/// undeclared would copy the whole repository to a public remote.
fn boundary(root: &Path) -> Result<Vec<String>, Outcome> {
    let path = root.join("assurance/claims.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| broken(format!("Could not read {}: {e}\n", path.display())))?;
    let registry =
        runner::assurance::Registry::from_json(&text).map_err(|e| broken(format!("{e}\n")))?;
    if registry.published.is_empty() {
        return Err(broken(format!(
            "{} declares no published boundary, so there is nothing to \
             project. Publishing everything is not the default.\n",
            path.display()
        )));
    }
    Ok(registry.published)
}

/// Where the projection came from, read at the edge.
///
/// A published tree that cannot say which revision produced it is a
/// mirror, and #269's rule is against mirrors: the point of generating
/// this is that a reader can tell what it is a projection *of*.
pub struct Provenance {
    /// `git describe --always` in the source tree.
    pub revision: String,
    pub generated: chrono::NaiveDate,
}

/// Project `root` into `out`, or say what would be projected when `out`
/// is `None`.
///
/// `tracked` is the repository's tracked paths, repo-relative and
/// forward-slashed — `git ls-files` at the edge.
pub fn run(
    root: &Path,
    tracked: &[String],
    out: Option<&Path>,
    provenance: &Provenance,
) -> Outcome {
    let published = match boundary(root) {
        Ok(published) => published,
        Err(outcome) => return outcome,
    };
    let selected = selected(tracked, &published);

    if selected.is_empty() {
        return broken(
            "The declared boundary matches nothing this tree tracks. Either \
             the boundary names paths that have moved, or the listing is \
             empty — both publish an empty repository.\n"
                .to_owned(),
        );
    }

    let mut out_text = String::new();
    for prefix in &published {
        let count = selected
            .iter()
            .filter(|path| path.starts_with(prefix.as_str()))
            .count();
        out_text.push_str(&format!("  {prefix} — {count} file(s)\n"));
    }

    let Some(out) = out else {
        out_text.push_str(&format!(
            "\n{} file(s) would be projected. Nothing written.\n",
            selected.len()
        ));
        return Outcome {
            text: out_text,
            code: ExitCode::Ok,
        };
    };

    if let Err(message) = validate_destination(root, out) {
        return broken(format!("{message}\n"));
    }

    if let Err(e) = write_tree(root, out, &selected) {
        return broken(format!("{e}\n"));
    }
    if let Err(e) = write_marker(out, &published, &selected, provenance) {
        return broken(format!("{e}\n"));
    }

    out_text.push_str(&format!(
        "\nProjected {} file(s) into {}.\n",
        selected.len(),
        out.display()
    ));
    Outcome {
        text: out_text,
        code: ExitCode::Ok,
    }
}

/// Refuse a destination whose cleanup could remove source material.
///
/// The default lives under `target/`, which is disposable build output.
/// A destination outside the source tree is safe too. Anything else
/// inside the source tree could be a tracked directory, while an equal
/// or ancestor destination would remove the whole source tree.
fn validate_destination(root: &Path, out: &Path) -> Result<(), String> {
    let source = canonical_for_comparison(root)
        .map_err(|e| format!("Could not resolve source {}: {e}", root.display()))?;
    let destination = canonical_for_comparison(out)
        .map_err(|e| format!("Could not resolve destination {}: {e}", out.display()))?;
    let disposable = canonical_for_comparison(&source.join("target")).map_err(|e| {
        format!(
            "Could not resolve the disposable target directory under {}: {e}",
            source.display()
        )
    })?;

    if source.starts_with(&destination) {
        return Err(format!(
            "Refusing to project into {} because clearing it would remove the source tree {}.",
            out.display(),
            root.display()
        ));
    }
    if destination.starts_with(&source) && !destination.starts_with(&disposable) {
        return Err(format!(
            "Refusing to project into {} because it is inside the source tree and outside its disposable target directory.",
            out.display()
        ));
    }
    Ok(())
}

/// Canonicalise the existing part of a path, then restore any suffix
/// that has not been created yet. This catches symlink aliases without
/// requiring the projection destination to exist already.
fn canonical_for_comparison(path: &Path) -> std::io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} has no existing ancestor", path.display()),
            )
        })?;
        missing.push(name.to_os_string());
        existing = existing.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} has no existing ancestor", path.display()),
            )
        })?;
    }
    let mut resolved = std::fs::canonicalize(existing)?;
    for part in missing.into_iter().rev() {
        if part == std::ffi::OsStr::new(".") {
            continue;
        }
        if part == std::ffi::OsStr::new("..") {
            resolved.pop();
        } else {
            resolved.push(part);
        }
    }
    Ok(resolved)
}

/// Copy the selection into a tree that holds nothing else.
///
/// The destination is cleared first: a projection that merges into
/// whatever was already there would keep publishing a file after the
/// boundary stopped naming it, which is precisely the drift a generated
/// tree exists to prevent.
fn write_tree(root: &Path, out: &Path, selected: &[&str]) -> std::io::Result<()> {
    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)?;

    for path in selected {
        let destination = out.join(path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(root.join(path), &destination)
            .map_err(|e| std::io::Error::new(e.kind(), format!("Could not project {path}: {e}")))?;
    }
    Ok(())
}

/// Say what this tree is, in both registers.
///
/// `PROJECTION.json` is the marker `runner::assurance` reads: its
/// presence is what tells validation that a cited path in the closed half
/// is expected-absent rather than deleted. `PROJECTION.md` is the same
/// fact for a person who has just cloned the repository and wants to know
/// why `README.md` links to documents that are not here.
///
/// Both are generated. A hand-written note about a generated tree is the
/// second copy #269 exists to prevent, and it would be the copy that goes
/// stale first.
fn write_marker(
    out: &Path,
    published: &[String],
    selected: &[&str],
    provenance: &Provenance,
) -> std::io::Result<()> {
    let manifest = serde_json::json!({
        "schema": "kettle/projection@0",
        "source": "dogwonder/kettle",
        "revision": provenance.revision,
        "generated": provenance.generated.to_string(),
        "published": published,
        "files": selected.len(),
    });
    std::fs::write(
        out.join(runner::assurance::PROJECTION_MARKER),
        serde_json::to_string_pretty(&manifest).expect("the manifest serialises") + "\n",
    )?;

    let boundary = published
        .iter()
        .map(|prefix| format!("- `{prefix}`\n"))
        .collect::<String>();
    std::fs::write(
        out.join("PROJECTION.md"),
        format!(
            "# This tree is a projection\n\n\
             Generated from `dogwonder/kettle` at `{}` on {}, carrying {} \
             files. It is not edited here: every file is a copy, and a \
             change made in this repository would be overwritten by the \
             next projection rather than reaching the product.\n\n\
             ## What it carries\n\n{}\n\
             That is the measurement layer — the pipeline crates, the task \
             packs with their prompts and their development and exam beds, \
             the committed baselines, and the assurance registry. The Tauri \
             shell and the Svelte frontend are not here, so links in \
             `README.md` to `CLAUDE.md`, `app/DECISIONS.md` and \
             `app/RELEASE-CHECKS.md` point into the half that stays \
             closed.\n\n\
             ## What that means for the registry\n\n\
             `assurance/claims.json` names the evidence behind each \
             product-level claim, and a few of those citations are surfaces \
             or tests inside the closed half. Validation reads this \
             marker and treats those as absent by design; every citation \
             the boundary *does* publish is checked here exactly as \
             strictly as it is in the source tree. Run `cargo run -p \
             kettle -- claims` to re-derive the statuses rather than \
             trusting the ones recorded in the file.\n\n\
             ## Reproduction\n\n\
             The code and the beds are here; the model weights and the \
             llama-server sidecar are not, and neither is redistributable \
             from here. The honest wording is *inspectable, and re-runnable \
             given the weights* — `evals/README.md` names the weights each \
             baseline was recorded against.\n",
            provenance.revision,
            provenance.generated,
            selected.len(),
            boundary,
        ),
    )?;
    Ok(())
}

/// The repository's tracked paths, as `git ls-files` reports them.
///
/// The edge. Failure is a refusal rather than a fallback to walking the
/// filesystem: a directory walk would happily publish an untracked
/// `statement.private.csv` sitting in the tree, and "we could not ask git
/// so we guessed" is not a sentence this command may say.
pub fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .map_err(|e| format!("Could not run git in {}: {e}", root.display()))?;

    if !output.status.success() {
        return Err(format!(
            "git ls-files failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| path.replace('\\', "/"))
        .collect())
}

/// The source revision, as `git describe --always` reports it.
///
/// The edge again. A projection that cannot name its source is refused
/// rather than stamped "unknown": the revision is how a reader ties a
/// published measurement back to the tree that produced it, and a public
/// tree with that field empty invites exactly the "which version was
/// this?" question it exists to answer.
pub fn revision(root: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["describe", "--always", "--dirty"])
        .output()
        .map_err(|e| format!("Could not run git in {}: {e}", root.display()))?;

    if !output.status.success() {
        return Err(format!(
            "git describe failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Where a projection lands by default, when no destination is named.
pub fn default_out_dir() -> PathBuf {
    PathBuf::from("target/public-tree")
}

#[cfg(test)]
mod destination_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kettle-project-destination-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch source");
        path
    }

    #[test]
    fn source_and_ancestor_destinations_are_refused() {
        let source = scratch("overlap").join("source");
        std::fs::create_dir_all(&source).expect("source");

        assert!(validate_destination(&source, &source).is_err());
        assert!(validate_destination(&source, source.parent().expect("parent")).is_err());
    }

    #[test]
    fn only_target_is_disposable_inside_the_source() {
        let source = scratch("inside");

        assert!(validate_destination(&source, &source.join("packs/projected")).is_err());
        assert!(validate_destination(&source, &source.join("target/public-tree")).is_ok());
        assert!(
            validate_destination(&source, &source.join("target/not-created/../../tracked"))
                .is_err()
        );
    }

    #[test]
    fn a_separate_destination_is_allowed() {
        let parent = scratch("separate");
        let source = parent.join("source");
        let destination = parent.join("public-tree");
        std::fs::create_dir_all(&source).expect("source");

        assert!(validate_destination(&source, &destination).is_ok());
    }
}
