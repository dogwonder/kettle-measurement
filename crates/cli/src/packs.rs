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
    /// What the pack is for, in a person's terms — `who`, `can`,
    /// `done_when` — verbatim from the manifest (30 August 2026).
    goal: PublicGoal,
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
struct PublicGoal {
    who: String,
    can: String,
    done_when: String,
}

#[derive(Debug, Serialize)]
struct PublicTime {
    /// The non-commitment class: duration varies between sittings.
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
        goal: manifest
            .goal
            .as_ref()
            .map(|goal| PublicGoal {
                who: goal.who.clone(),
                can: goal.can.clone(),
                done_when: goal.done_when.clone(),
            })
            .expect("run_json refuses an offered pack without a goal before projecting it"),
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
            // Measured but not offered (#545): a public page describing
            // a task the app does not offer is the misdescription this
            // command exists to prevent. `packs list` still names it.
            Ok(pack) if pack.manifest.withdrawn.is_some() => continue,
            // A pack with no user goal cannot be described to the
            // public: the page would have to invent what it is for.
            Ok(pack) if pack.manifest.goal.is_none() => {
                return Outcome {
                    text: format!(
                        "{} states no `goal`, so the public page cannot say what it is for — \
                         add who it is for, what they can do and when it is done \
                         (packs/AUTHORING.md, step 0)",
                        pack.manifest.id
                    ),
                    code: ExitCode::Broken,
                };
            }
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

/// `kettle packs list` — every pack under `packs_dir` that loads, in
/// directory order, withdrawn ones included and marked. A pack that
/// fails to load is skipped: a listing is a run doing its best.
pub fn run_list(packs_dir: &Path) -> String {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(packs_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    let packs: Vec<runner::packs::Pack> =
        dirs.iter().filter_map(|dir| load_pack(dir).ok()).collect();
    crate::plan::list_packs(&packs)
}
