//! Writing the generated eval bed to disk (#265).
//!
//! `runner::eval::bed` is pure: it turns the committed spec into bytes
//! and touches no filesystem. This is the thin edge that puts those
//! bytes where the fixtures live, so changing the bed is one command
//! and a reviewable diff rather than a throwaway script.
//!
//! `--check` writes nothing and reports what would move. It is the same
//! comparison `cargo test -p runner --test bed` makes; having it here
//! too means you can ask the question without a test runner, and see
//! every file that differs rather than the first one.

use runner::eval::bed;
use std::path::Path;

/// What happened, and whether the caller should exit non-zero.
pub struct Outcome {
    /// What to print.
    pub text: String,
    /// True when the bed on disk already matches the spec, or was
    /// brought into line with it.
    pub ok: bool,
}

/// Regenerate a letter bed, or say what regenerating would change.
fn run_letters(pack_dir: &Path, check: bool) -> Outcome {
    let spec = match runner::eval::letters::committed_spec(pack_dir) {
        Ok(spec) => spec,
        Err(e) => {
            return Outcome {
                text: format!("{e}\n"),
                ok: false,
            }
        }
    };
    let dir = pack_dir.join("fixtures");
    let mut changed = Vec::new();
    let generated = runner::eval::letters::generate(&spec);
    let (twins, mut relations) = runner::eval::letters::twins(&generated);
    let with_twins: Vec<runner::eval::letters::GeneratedLetter> =
        generated.iter().cloned().chain(twins).collect();
    let (adversarial, adversarial_relations) =
        runner::eval::letters::adversarial_twins(&with_twins);
    relations.extend(adversarial_relations);
    let (controlled, controlled_relations) = runner::eval::letters::controlled_twins(&generated);
    relations.extend(controlled_relations);
    let mut files: Vec<(String, String)> = Vec::new();
    for letter in with_twins.into_iter().chain(adversarial).chain(controlled) {
        files.push((format!("{}.txt", letter.stem), letter.text));
        files.push((format!("{}.expected.json", letter.stem), letter.expected));
    }
    files.push((
        "relations.json".to_owned(),
        runner::eval::renewals::relations_file(relations),
    ));
    for (name, want) in files {
        {
            let path = dir.join(&name);
            if std::fs::read_to_string(&path).is_ok_and(|found| found == want) {
                continue;
            }
            changed.push(name);
            if check {
                continue;
            }
            if let Err(e) = std::fs::write(&path, &want) {
                return Outcome {
                    text: format!("Could not write {}: {e}\n", path.display()),
                    ok: false,
                };
            }
        }
    }
    Outcome {
        text: if changed.is_empty() {
            "The letter bed on disk is what the spec generates.\n".to_owned()
        } else if check {
            format!(
                "{} file(s) would change:\n  {}\n",
                changed.len(),
                changed.join("\n  ")
            )
        } else {
            format!("Wrote {} file(s).\n", changed.len())
        },
        ok: changed.is_empty() || !check,
    }
}

/// Regenerate a renewal bed (#66), or say what regenerating would
/// change. Three files per fixture rather than two: a comparison is two
/// documents and one set of expectations describing both (#354).
fn run_renewals(pack_dir: &Path, check: bool) -> Outcome {
    let spec = match runner::eval::renewals::committed_spec(pack_dir) {
        Ok(spec) => spec,
        Err(e) => {
            return Outcome {
                text: format!("{e}\n"),
                ok: false,
            }
        }
    };
    let dir = pack_dir.join("fixtures");
    let mut changed = Vec::new();
    let generated = runner::eval::renewals::generate(&spec);
    let (twin_files, mut relations) = runner::eval::renewals::twins(&generated);
    let (adversarial, adversarial_relations) =
        runner::eval::renewals::adversarial_twins(&generated);
    relations.extend(adversarial_relations);
    let mut files: Vec<(String, String)> = Vec::new();
    for renewal in generated.into_iter().chain(adversarial) {
        files.push((format!("{}-previous.txt", renewal.stem), renewal.previous));
        files.push((format!("{}-renewal.txt", renewal.stem), renewal.renewal));
        files.push((format!("{}.expected.json", renewal.stem), renewal.expected));
    }
    files.extend(twin_files);
    files.push((
        "relations.json".to_owned(),
        runner::eval::renewals::relations_file(relations),
    ));
    for (name, want) in files {
        {
            let path = dir.join(&name);
            if std::fs::read_to_string(&path).is_ok_and(|found| found == want) {
                continue;
            }
            changed.push(name);
            if check {
                continue;
            }
            if let Err(e) = std::fs::write(&path, &want) {
                return Outcome {
                    text: format!("Could not write {}: {e}\n", path.display()),
                    ok: false,
                };
            }
        }
    }
    Outcome {
        text: if changed.is_empty() {
            "The renewal bed on disk is what the spec generates.\n".to_owned()
        } else if check {
            format!(
                "{} file(s) would change:\n  {}\n",
                changed.len(),
                changed.join("\n  ")
            )
        } else {
            format!("Wrote {} file(s).\n", changed.len())
        },
        ok: changed.is_empty() || !check,
    }
}

/// Regenerate the bed in `pack_dir`, or say what regenerating would
/// change.
pub fn run(pack_dir: &Path, check: bool) -> Outcome {
    // Which bed a pack has is which spec it committed. A letter pack
    // and a statement pack generate different things from different
    // descriptions, and neither has to know about the other.
    if pack_dir
        .join("fixtures")
        .join("letter-bed-spec.json")
        .exists()
    {
        return run_letters(pack_dir, check);
    }
    if pack_dir
        .join("fixtures")
        .join("renewal-bed-spec.json")
        .exists()
    {
        return run_renewals(pack_dir, check);
    }
    let spec = match bed::committed_spec(pack_dir) {
        Ok(spec) => spec,
        Err(e) => {
            return Outcome {
                text: format!("{e}\n"),
                ok: false,
            }
        }
    };
    let dir = pack_dir.join("fixtures");
    let mut changed = Vec::new();
    let generated = bed::generate(&spec);
    let (twins, mut relations) = bed::twins(&generated);
    let (adversarial, adversarial_relations) = bed::adversarial_twins(&generated);
    relations.extend(adversarial_relations);
    let mut files: Vec<(String, String)> = Vec::new();
    for fixture in generated.into_iter().chain(twins).chain(adversarial) {
        files.push((format!("{}.csv", fixture.stem), fixture.csv));
        files.push((format!("{}.expected.json", fixture.stem), fixture.expected));
    }
    files.push((
        "relations.json".to_owned(),
        runner::eval::renewals::relations_file(relations),
    ));
    for (name, want) in files {
        let path = dir.join(&name);
        let same = std::fs::read_to_string(&path).is_ok_and(|found| found == want);
        if same {
            continue;
        }
        changed.push(name);
        if check {
            continue;
        }
        if let Err(e) = std::fs::write(&path, &want) {
            return Outcome {
                text: format!("Could not write {}: {e}\n", path.display()),
                ok: false,
            };
        }
    }

    let mut text = String::new();
    if changed.is_empty() {
        text.push_str("The bed on disk is exactly what the spec generates.\n");
        return Outcome { text, ok: true };
    }
    let verb = if check { "would change" } else { "wrote" };
    text.push_str(&format!("{} {} file(s):\n", verb, changed.len()));
    for name in &changed {
        text.push_str(&format!("  {name}\n"));
    }
    Outcome {
        text,
        // A check that found differences is a finding, not a success.
        ok: !check,
    }
}
