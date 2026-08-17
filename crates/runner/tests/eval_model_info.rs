//! Reading a model's identity off its file name (#25).
//!
//! An `EvalReport` is a claim about particular weights, so it records
//! the file itself — and the file name is what a person typed at
//! `--model`, so it is what the table has to be able to name.

use runner::eval::fixture::model_info;

/// The model the pipeline was proven against (README, 2026-07-20).
#[test]
fn reads_parameters_and_quantisation_off_a_gguf_name() {
    let model = model_info("qwen2.5-7b-instruct-q4_k_m.gguf", 8192);

    assert_eq!(model.file, "qwen2.5-7b-instruct-q4_k_m.gguf");
    assert_eq!(model.params, "7B");
    assert_eq!(model.quant, "Q4_K_M");
    assert_eq!(model.context, 8192);
}

/// A name that says neither is still a model someone ran. The report
/// records what it knows and does not invent the rest — a made-up "7B"
/// against real scores is a tier claim nobody can check.
#[test]
fn a_name_that_says_nothing_claims_nothing() {
    let model = model_info("mystery.gguf", 4096);

    assert_eq!(model.file, "mystery.gguf");
    assert_eq!(model.params, "unknown");
    assert_eq!(model.quant, "unknown");
}
