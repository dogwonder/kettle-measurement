//! `kettle packs --json` — the public, build-time projection of the pack
//! manifests (#478).
//!
//! The website never owns a list of what Kettle can read. A two-column
//! table written by hand there is stale the next time a pack changes,
//! and it changed three times in a fortnight. So the page asks this
//! command, which
//! loads every pack through the same loader a run uses and hands back
//! only what a public page can support — the pack's own words, its
//! named documents, and what it may do.
//!
//! Unlike `packs list`, a pack that fails to load is a **refusal** here
//! rather than a skipped line. A run skipping a broken pack is a run
//! doing its best; a public page silently dropping one is a page
//! quietly misdescribing the product.

use runner::packs::{load_pack, Manifest, TimeKind};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Broken = 2,
}

#[derive(Debug)]
pub struct Outcome {
    pub text: String,
    pub code: ExitCode,
}

#[derive(Debug, Serialize)]
struct PublicPacks {
    schema: &'static str,
    packs: Vec<PublicPack>,
}

#[derive(Debug, Serialize)]
struct PublicPack {
    id: String,
    name: String,
    version: String,
    /// The pack's own one-line promise. Rendered verbatim: a page that
    /// paraphrases it has invented a claim the pack does not make.
    description: String,
    inputs: Vec<PublicInput>,
    capabilities: Vec<String>,
    /// What the pack says a run costs in time. Published because the
    /// public page had a hand-written grid of three tasks, one of which
    /// ("Index a year of paperwork") is not a pack at all — the same
    /// defect this file exists to prevent, one section further down the
    /// page. A promise about time is a claim like any other, and the
    /// pack is the only thing entitled to make it.
    time: PublicTime,
}

#[derive(Debug, Serialize)]
struct PublicTime {
    /// The commitment class: quick, kettle-worthy, overnight, varies.
    kind: TimeKind,
    /// The pack's own words beside it — "by letter", "with statement
    /// size". Never derived from anything by string surgery.
    estimate: String,
}

#[derive(Debug, Serialize)]
struct PublicInput {
    role: String,
    label: String,
    accept: Vec<String>,
    /// Cardinality in the words the CLI already uses, so "one file" and
    /// "at least one file" read the same everywhere (#334).
    count: String,
}

fn project(manifest: &Manifest) -> PublicPack {
    PublicPack {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        inputs: manifest
            .inputs
            .iter()
            .map(|input| PublicInput {
                role: input.role.clone(),
                label: input.label.clone(),
                accept: input.accept.clone(),
                count: input.count.in_words(),
            })
            .collect(),
        capabilities: manifest.capabilities.clone(),
        time: PublicTime {
            kind: manifest.copy().time.kind,
            estimate: manifest.copy().time.estimate.clone(),
        },
    }
}

/// Every pack under `packs_dir`, in directory order, or a refusal
/// naming the first pack that would not load.
pub fn run_json(packs_dir: &Path) -> Outcome {
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(packs_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(e) => {
            return Outcome {
                text: format!("Could not read {}: {e}", packs_dir.display()),
                code: ExitCode::Broken,
            };
        }
    };
    dirs.sort();

    let mut packs = Vec::new();
    for dir in dirs {
        match load_pack(&dir) {
            Ok(pack) => packs.push(project(&pack.manifest)),
            Err(e) => {
                return Outcome {
                    text: format!(
                        "{} will not load, so the public page cannot describe it: {e}",
                        dir.display()
                    ),
                    code: ExitCode::Broken,
                };
            }
        }
    }

    let document = PublicPacks {
        schema: "kettle/public-packs@0",
        packs,
    };
    match serde_json::to_string_pretty(&document) {
        Ok(text) => Outcome {
            text,
            code: ExitCode::Ok,
        },
        Err(e) => Outcome {
            text: format!("Could not serialise the pack projection: {e}"),
            code: ExitCode::Broken,
        },
    }
}
