//! Which models to measure: one `--model`, or a list of them in a
//! `models.toml` (#38, brief §6).
//!
//! ```toml
//! # models.toml — the tiers this pack is measured against.
//! [[model]]
//! file = "models/qwen2.5-3b-instruct-q4_k_m.gguf"
//!
//! [[model]]
//! file = "models/gemma-3-4b-it-q4_k_m.gguf"
//! params = "4B"      # optional — otherwise read off the file name
//! quant  = "Q4_K_M"  # optional — likewise
//! context = 8192     # optional — otherwise the pack's own context
//! ```
//!
//! An eval report records the weights it measured, not a tier name, so
//! it needs the parameter count and quantisation. Both are conventional
//! in `.gguf` file names and are read off them; `models.toml` is there
//! for when the convention doesn't hold, and for keeping a list of tiers
//! under version control.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One model to measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    /// Where the weights are, as given on the command line.
    pub path: PathBuf,
    /// The file's name alone, which is what an eval report records — a
    /// full path would leak a home directory into a document people
    /// share.
    pub file: String,
    /// Parameter count as advertised, e.g. "3B".
    pub params: String,
    /// Quantisation, e.g. "Q4_K_M".
    pub quant: String,
    /// The context window to measure at, if the list pins one. `None`
    /// leaves it to the pack.
    pub context: Option<u32>,
}

impl ModelSpec {
    /// A model named on the command line, with its parameter count and
    /// quantisation read off the file name.
    pub fn from_path(path: &Path) -> ModelSpec {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        ModelSpec {
            path: path.to_path_buf(),
            params: params_in(&file).unwrap_or_else(|| "unknown".to_owned()),
            quant: quant_in(&file).unwrap_or_else(|| "unknown".to_owned()),
            file,
            context: None,
        }
    }
}

/// Every model to measure, in the order they should be measured.
pub fn resolve(model: Option<&Path>, models: Option<&Path>) -> Result<Vec<ModelSpec>, String> {
    resolve_in(model, models, Path::new(DEFAULT_MODELS_DIR))
}

/// Where `.gguf` weights live by default (CLAUDE.md, workspace layout).
pub const DEFAULT_MODELS_DIR: &str = "models";

/// As [`resolve`], but looking for bare file names in `models_dir`.
///
/// `--model qwen2.5-7b-instruct-q4_k_m.gguf` is what anyone types, and
/// the weights are in `models/`, not the directory they are standing
/// in. A name that isn't where they are is looked for there before the
/// eval gives up.
///
/// A path that exists is never second-guessed, and neither is one that
/// names a directory of its own: "no such model" about the path someone
/// actually typed is a better error than silently measuring a different
/// file that happened to share a name.
pub fn resolve_in(
    model: Option<&Path>,
    models: Option<&Path>,
    models_dir: &Path,
) -> Result<Vec<ModelSpec>, String> {
    match (model, models) {
        (Some(_), Some(_)) => Err("Choose either --model or --models, not both.".to_owned()),
        (Some(path), None) => Ok(vec![ModelSpec::from_path(&in_models_dir(path, models_dir))]),
        (None, Some(list)) => read_list(list),
        (None, None) => Err(
            "Say which model to measure: --model <file.gguf>, or --models <models.toml>."
                .to_owned(),
        ),
    }
}

/// A bare file name that isn't here, resolved against the models
/// directory. Anything else is left exactly as given.
fn in_models_dir(path: &Path, models_dir: &Path) -> PathBuf {
    let bare = path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty());
    if path.exists() || !bare {
        return path.to_path_buf();
    }
    let candidate = models_dir.join(path);
    if candidate.exists() {
        candidate
    } else {
        path.to_path_buf()
    }
}

// ---------------------------------------------------------------------------
// models.toml

#[derive(Debug, Deserialize)]
struct ModelList {
    #[serde(default)]
    model: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    file: PathBuf,
    params: Option<String>,
    quant: Option<String>,
    context: Option<u32>,
}

fn read_list(path: &Path) -> Result<Vec<ModelSpec>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read the model list {}: {e}", path.display()))?;
    parse_list(&text).map_err(|problem| format!("{}: {problem}", path.display()))
}

/// Read a `models.toml`. A list with no models in it is an error rather
/// than an empty eval — an empty table looks like a pass.
pub fn parse_list(text: &str) -> Result<Vec<ModelSpec>, String> {
    let list: ModelList =
        toml::from_str(text).map_err(|e| format!("could not make sense of it: {e}"))?;
    if list.model.is_empty() {
        return Err("no [[model]] entries — nothing to measure".to_owned());
    }
    Ok(list
        .model
        .into_iter()
        .map(|entry| {
            let mut spec = ModelSpec::from_path(&entry.file);
            if let Some(params) = entry.params {
                spec.params = params;
            }
            if let Some(quant) = entry.quant {
                spec.quant = quant;
            }
            spec.context = entry.context;
            spec
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Reading a file name

/// The parameter count in a `.gguf` file name: the first word that is a
/// number followed by a "b", e.g. `qwen2.5-3b-instruct-q4_k_m.gguf` →
/// `3B`. Deliberately unfussy — it is a convenience, and `models.toml`
/// overrides it.
fn params_in(file: &str) -> Option<String> {
    words(file).find_map(|word| {
        let size = word.strip_suffix('b')?;
        let looks_like_a_size = !size.is_empty()
            && size
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.');
        looks_like_a_size.then(|| word.to_uppercase())
    })
}

/// The quantisation in a `.gguf` file name: the first word that is a
/// "q" followed by a digit, e.g. `q4_k_m` → `Q4_K_M`.
fn quant_in(file: &str) -> Option<String> {
    words(file).find_map(|word| {
        let rest = word.strip_prefix('q')?;
        rest.starts_with(|character: char| character.is_ascii_digit())
            .then(|| word.to_uppercase())
    })
}

/// A file name's words, lowercased: `.gguf` dropped, then split on the
/// hyphens and dots that name these files.
fn words(file: &str) -> impl Iterator<Item = String> + '_ {
    file.trim_end_matches(".gguf")
        .to_lowercase()
        .split(['-', '.'])
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_params_and_quant_off_a_file_name() {
        let spec = ModelSpec::from_path(Path::new("models/qwen2.5-3b-instruct-q4_k_m.gguf"));

        assert_eq!(spec.file, "qwen2.5-3b-instruct-q4_k_m.gguf");
        assert_eq!(spec.params, "3B");
        assert_eq!(spec.quant, "Q4_K_M");
        assert_eq!(spec.context, None);
    }

    #[test]
    fn says_unknown_rather_than_guessing() {
        let spec = ModelSpec::from_path(Path::new("weights.gguf"));

        assert_eq!(spec.params, "unknown");
        assert_eq!(spec.quant, "unknown");
    }

    #[test]
    fn a_model_list_is_read_in_order_and_overrides_the_file_name() {
        let models = parse_list(
            r#"
            [[model]]
            file = "models/qwen2.5-3b-instruct-q4_k_m.gguf"

            [[model]]
            file = "models/mystery.gguf"
            params = "4B"
            quant = "Q5_K_M"
            context = 4096
            "#,
        )
        .expect("read the list");

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].file, "qwen2.5-3b-instruct-q4_k_m.gguf");
        assert_eq!(models[0].params, "3B");
        assert_eq!(models[1].params, "4B");
        assert_eq!(models[1].quant, "Q5_K_M");
        assert_eq!(models[1].context, Some(4096));
    }

    #[test]
    fn an_empty_model_list_is_a_problem_not_an_empty_eval() {
        let problem = parse_list("# nothing here yet\n").expect_err("refuse an empty list");

        assert!(problem.contains("nothing to measure"), "{problem}");
    }

    #[test]
    fn an_unreadable_model_list_says_so() {
        let problem = parse_list("this is not toml at all {{{").expect_err("refuse it");

        assert!(problem.contains("could not make sense of it"), "{problem}");
    }

    #[test]
    fn one_model_or_a_list_but_not_neither_and_not_both() {
        let path = PathBuf::from("a.gguf");

        assert_eq!(resolve(Some(&path), None).expect("one model").len(), 1);
        assert!(resolve(None, None).is_err());
        assert!(resolve(Some(&path), Some(&path)).is_err());
    }
}
