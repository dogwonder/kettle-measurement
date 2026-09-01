//! #568 rung 1: the half of the instrument that needs no weights.
//!
//! Split out of `rung1_accounts.rs` so `tests/rung1_sampling.rs` can
//! reach it. Everything here is upstream of the run's denominator —
//! which passages are asked about, how they are grouped into calls, how
//! an answer joins back to its passage — and every one of them can fail
//! without saying so. A batcher that drops a passage shrinks the
//! denominator; a join that misses drops a claim from the numerator and
//! the denominator together, which leaves the rate looking reasonable
//! while the count quietly falls.
//!
//! It lives under `examples/rung1/` rather than in the library because
//! it is a measurement harness, not part of the pipeline Kettle ships.
//! Cargo only treats an `examples/` subdirectory as a target when it
//! holds a `main.rs`, so this compiles as a module of the example and
//! of the test, and as nothing on its own.

#![allow(dead_code)]

use serde_json::{json, Value};

/// One call's worth of passages, in characters. The context is 8,192
/// tokens; this leaves room for the instruction, the answer and the
/// margin between characters and tokens on a page full of figures.
pub const BATCH_CHARS: usize = 3_500;

/// One prose chunk, in characters. Larger than a closed batch because
/// the answer is one paragraph rather than one object per passage.
pub const PROSE_CHARS: usize = 6_000;

pub const KINDS: &[&str] = &[
    "total_income",
    "total_expenditure",
    "net_movement_in_funds",
    "funds_carried_forward",
    "restricted_funds",
    "unrestricted_funds",
    "designated_funds",
    "reserves_policy",
    "going_concern",
    "staff_costs",
    "trustee_payments",
    "none",
];

pub fn closed_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "answers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "kind": { "type": "string", "enum": KINDS },
                        "value": { "type": "string" },
                        "quote": { "type": "string" },
                        "confidence": { "type": "string", "enum": ["settled", "unsure"] }
                    },
                    "required": ["id", "kind", "value", "quote", "confidence"]
                }
            }
        },
        "required": ["answers"]
    })
}

pub fn prose_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "explanation": { "type": "string" } },
        "required": ["explanation"]
    })
}

pub fn closed_prompt(passages: &[(String, String)]) -> String {
    let mut prompt = String::from(
        "You are reading part of a charity's annual report and accounts.\n\n\
         Below are numbered passages, exactly as they appear in the document.\n\n\
         For each passage, give one fact if the passage states one, and the kind \
         `none` if it states none of them.\n\n\
         The kinds of fact are:\n\
         - total_income — the charity's total income for a financial year\n\
         - total_expenditure — its total expenditure for a financial year\n\
         - net_movement_in_funds — the surplus or deficit for the year\n\
         - funds_carried_forward — total funds at the end of the year\n\
         - restricted_funds — a balance or movement on restricted funds\n\
         - unrestricted_funds — a balance or movement on unrestricted funds\n\
         - designated_funds — a balance or movement on designated funds\n\
         - reserves_policy — what the trustees say their reserves policy is\n\
         - going_concern — what the accounts say about going concern\n\
         - staff_costs — total staff costs for a year\n\
         - trustee_payments — payments, benefits or expenses to trustees\n\
         - none — the passage states none of these\n\n\
         For each passage answer with:\n\
         - id: the passage's number\n\
         - kind: one of the kinds above\n\
         - value: the figure or wording the fact carries, copied exactly as printed\n\
         - quote: the words from that passage which carry it, copied exactly\n\
         - confidence: settled if the passage says it plainly, unsure otherwise\n\n\
         Answer about every passage, once each, in the order given. For kind \
         `none`, leave value and quote empty.\n\n\
         Passages:\n",
    );
    for (id, text) in passages {
        prompt.push_str(&format!("[{id}] {text}\n\n"));
    }
    prompt
}

pub fn prose_prompt(chunk: &str) -> String {
    format!(
        "You are helping a trustee understand their charity's annual report and \
         accounts.\n\nBelow is part of those accounts, exactly as it appears.\n\n\
         Explain in plain English what this part says: the money coming in and \
         going out, which funds it belongs to, and anything a trustee should \
         notice. Around 150 words.\n\n{chunk}\n"
    )
}

/// Passages worth asking about: one that carries no digit carries no
/// figure, and the closed questions are about figures and the sentences
/// that qualify them. Deterministic, declared before the run, and the
/// same filter a pack's preprocessing would apply.
///
/// This is the denominator's definition. The rate #568 closed on is
/// "invented claims out of judgeable ones", and *judgeable* is whatever
/// this function admits — so it belongs in a test rather than in a
/// sentence on the issue.
pub fn worth_asking(text: &str) -> bool {
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let words = text.split_whitespace().count();
    let policy_words = ["reserves", "going concern", "restricted", "designated"];
    let lower = text.to_lowercase();
    let says_policy = policy_words.iter().any(|w| lower.contains(w));
    (has_digit || says_policy) && words >= 3
}

/// Group passages into calls without ever cutting one.
///
/// A passage larger than the budget travels alone and whole. Splitting
/// it would mean asking the model about text the document does not
/// contain, which is precisely the thing this run exists to count.
pub fn batches(passages: Vec<(String, String)>, budget: usize) -> Vec<Vec<(String, String)>> {
    let mut out: Vec<Vec<(String, String)>> = Vec::new();
    let mut current: Vec<(String, String)> = Vec::new();
    let mut size = 0;
    for (id, text) in passages {
        let cost = text.len() + id.len() + 6;
        if !current.is_empty() && size + cost > budget {
            out.push(std::mem::take(&mut current));
            size = 0;
        }
        size += cost;
        current.push((id, text));
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Split prose on line boundaries, keeping every line exactly once and
/// in order. A line longer than the budget travels alone, for the same
/// reason a passage does.
pub fn chunks(text: &str, budget: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if !current.is_empty() && current.len() + line.len() + 1 > budget {
            out.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// The passage number, however the model echoed it.
///
/// It echoes the id as it saw it printed, brackets and all, and the
/// judging joins on the number. A join on the echoed string drops every
/// claim whose id came back decorated — and drops it from the numerator
/// and the denominator at once, so the rate stays plausible while the
/// count quietly falls.
pub fn passage_id(echoed: &str) -> String {
    echoed.chars().filter(|c| c.is_ascii_digit()).collect()
}
