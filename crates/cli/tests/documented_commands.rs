//! #418: a command copied from the repository must name something the
//! repository actually builds. The guard discovers runnable examples
//! rather than blessing a fixed list of documents, so the next guide
//! or model table is covered without being added here by hand.
//!
//! One deliberate exception, in the `px-ok` idiom: a command documented
//! as running at a pinned historical checkout (the Act I exhibit
//! replays the tree before #261, where the packages had other names)
//! carries `# pinned-tree:` and its reason on the line. The comment is
//! inert when copy-pasted; a bare marker without the colon still fails.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn every_documented_cli_command_names_the_package_and_binary_we_build() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root.join("crates/cli/Cargo.toml")).expect("CLI manifest is readable"),
    )
    .expect("CLI manifest is TOML");
    let package = manifest["package"]["name"]
        .as_str()
        .expect("the CLI package has a name");
    let binaries: Vec<&str> = manifest["bin"]
        .as_array()
        .expect("the CLI declares its binary")
        .iter()
        .filter_map(|bin| bin["name"].as_str())
        .collect();

    assert_eq!(
        package, "kettle",
        "the user-facing package and binary agree"
    );
    assert!(
        binaries.contains(&"kettle"),
        "the CLI manifest does not build a `kettle` binary"
    );

    let mut documents = Vec::new();
    find_documents(&root, &mut documents);
    documents.sort();

    let mut wrong = Vec::new();
    for path in documents {
        let text = fs::read_to_string(&path).expect("a discovered document is readable");
        for (line, command) in runnable_commands(&path, &text) {
            let location = format!("{}:{line}", path.strip_prefix(&root).unwrap().display());

            if command.starts_with("kettle ") {
                wrong.push(format!(
                    "{location}: `{command}` assumes a `kettle` on PATH; use `cargo run -p kettle -- …`"
                ));
            }

            let words: Vec<&str> = command.split_whitespace().collect();
            if words.first() == Some(&"cargo") && words.get(1) == Some(&"run") {
                if let Some(index) = words
                    .iter()
                    .position(|word| *word == "-p" || *word == "--package")
                {
                    if words.get(index + 1) != Some(&package) {
                        wrong.push(format!(
                            "{location}: `{command}` names package {:?}, but Cargo builds `{package}`",
                            words.get(index + 1).copied().unwrap_or("<missing>")
                        ));
                    }
                }

                if let Some(index) = words.iter().position(|word| *word == "--bin") {
                    let named = words.get(index + 1).copied().unwrap_or("<missing>");
                    if !binaries.contains(&named) {
                        wrong.push(format!(
                            "{location}: `{command}` names binary `{named}`, which the package does not build"
                        ));
                    }
                }
            }
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn find_documents(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("repository directory is readable") {
        let entry = entry.expect("repository entry is readable");
        let path = entry.path();
        let file_type = entry.file_type().expect("repository entry has a type");
        if file_type.is_dir() {
            if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".claude" | ".codex" | ".git" | "node_modules" | "target")
            ) {
                continue;
            }
            find_documents(&path, found);
        } else if file_type.is_file()
            && matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("html" | "md" | "toml")
            )
        {
            found.push(path);
        }
    }
}

fn runnable_commands(path: &Path, text: &str) -> Vec<(usize, String)> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    if extension == Some("toml") {
        return text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let command = line.trim_start().strip_prefix('#')?.trim_start();
                is_cli_command(command).then(|| (index + 1, command.to_owned()))
            })
            .collect();
    }

    if extension == Some("html") {
        let mut commands = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let mut remaining = line;
            while let Some((_, after_open)) = remaining.split_once("<code>") {
                let Some((code, after_close)) = after_open.split_once("</code>") else {
                    break;
                };
                let command = code.trim();
                if is_cli_command(command) {
                    commands.push((index + 1, command.to_owned()));
                }
                remaining = after_close;
            }
        }
        return commands;
    }

    let mut shell = false;
    let mut commands = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(language) = trimmed.strip_prefix("```") {
            if shell {
                shell = false;
            } else {
                shell = matches!(language.trim(), "sh" | "bash" | "shell");
            }
            continue;
        }
        if shell && is_cli_command(trimmed) && !pinned_to_another_tree(trimmed) {
            commands.push((index + 1, trimmed.to_owned()));
        }
    }
    commands
}

fn is_cli_command(line: &str) -> bool {
    line.starts_with("kettle ") || line.starts_with("cargo run ")
}

/// The command runs at a checkout its document pins, so today's
/// manifest is the wrong referee. The colon makes the reason
/// mandatory: `# pinned-tree` alone stays checked.
fn pinned_to_another_tree(line: &str) -> bool {
    line.contains("# pinned-tree:")
}

/// A documented command that names a pack must name one the repository
/// ships. Written 14 August 2026, after `eval letter-triage` — the pack
/// is `app.kttl.letter-to-actions` — was pasted onto a rented GPU and
/// refused in three seconds, on the far side of a ten-minute CUDA
/// build. The runner's refusal was correct and arrived too late to be
/// cheap; this one costs 0.1s and arrives before the box is rented.
#[test]
fn every_documented_pack_id_names_a_pack_the_repository_ships() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let shipped = shipped_pack_ids(&root);
    assert!(
        shipped.len() >= 3,
        "the repository ships packs to check against"
    );

    let mut documents = Vec::new();
    find_documents(&root, &mut documents);
    documents.sort();

    let mut wrong = Vec::new();
    for path in documents {
        let text = fs::read_to_string(&path).expect("a discovered document is readable");
        for (line, command) in runnable_commands(&path, &text) {
            let Some(named) = pack_argument(&command) else {
                continue;
            };
            if !shipped.iter().any(|id| id == named) {
                wrong.push(format!(
                    "{}:{line}: `{command}` names pack `{named}`, which the repository does not ship",
                    path.strip_prefix(&root).unwrap().display()
                ));
            }
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_placeholder_pack_is_not_mistaken_for_a_real_one() {
    assert_eq!(
        pack_argument("cargo run -p kettle -- eval app.kttl.renewal-diff --model w.gguf"),
        Some("app.kttl.renewal-diff"),
        "a named pack is checked"
    );
    assert_eq!(
        pack_argument("cargo run -p kettle -- eval <pack> --model <weights>"),
        None,
        "a placeholder names nothing to check"
    );
    assert_eq!(
        pack_argument("cargo run -p kettle -- packs list"),
        None,
        "a subcommand that takes no pack is left alone"
    );
    assert_eq!(
        pack_argument("cargo run -p kettle -- eval letter-triage --model w.gguf"),
        Some("letter-triage"),
        "the shorthand that cost a CUDA build is reported, not skipped"
    );
}

/// The pack ids under `packs/`, read from each manifest rather than
/// from the directory name — the two agree today, and the manifest is
/// what the runner resolves against.
fn shipped_pack_ids(root: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(root.join("packs")).expect("packs directory is readable") {
        let path = entry
            .expect("packs entry is readable")
            .path()
            .join("pack.json");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let manifest: serde_json::Value =
            serde_json::from_str(&text).expect("a pack manifest is JSON");
        if let Some(id) = manifest["id"].as_str() {
            ids.push(id.to_owned());
        }
    }
    ids
}

/// The pack a documented command names, if it names one. A `<placeholder>`
/// names nothing — the document is telling the reader to substitute.
fn pack_argument(command: &str) -> Option<&str> {
    // A trailing `# comment` is prose, not an argument: `bed  # write it`
    // otherwise reads as a pack called `#`.
    let command = command
        .split_once(" #")
        .map_or(command, |(before, _)| before);
    let words: Vec<&str> = command.split_whitespace().collect();
    // `cargo run -p kettle -- eval …` carries two `run`s and only the
    // one after the separator is Kettle's. Looking from the left found
    // cargo's, which is how this helper's own first version was wrong.
    let start = words
        .iter()
        .position(|word| *word == "--")
        .map_or(0, |i| i + 1);
    let index = start
        + words[start..]
            .iter()
            .position(|word| matches!(*word, "eval" | "bed" | "run"))?;
    let named = words.get(index + 1)?;
    if named.starts_with('<') || named.starts_with('-') {
        return None;
    }
    Some(named)
}

#[test]
fn a_command_pinned_to_another_tree_is_not_checked_against_todays_manifest() {
    let text = "```sh\ncargo run -p cli -- render a.json  # pinned-tree: names at 1f14c8f\ncargo run -p cli -- render b.json\ncargo run -p cli -- render c.json  # pinned-tree\n```\n";
    let commands = runnable_commands(Path::new("doc.md"), text);
    let lines: Vec<usize> = commands.iter().map(|(line, _)| *line).collect();
    assert_eq!(
        lines,
        vec![3, 4],
        "only the reasoned marker exempts; a bare `# pinned-tree` and an unmarked command stay checked"
    );
}
