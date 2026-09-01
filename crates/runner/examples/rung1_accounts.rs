//! #568 rung 1: how much does the raw model invent over filed charity
//! accounts, before any guard exists?
//!
//! Two arms over the same corpus, pre-registered on #474 before the
//! corpus was fetched:
//!
//! - **A, closed**: the summariser's closed questions, schema-valid,
//!   with nothing in Rust checking the answer. This is the rung the
//!   5% floor is read against.
//! - **B, prose**: the same documents explained in plain English, with
//!   the quotes unverified. Diagnostic only — it says whether this
//!   material can produce invention at all, which is what tells "the
//!   accounts are easy" apart from "closed questions already removed
//!   it".
//!
//! Not a pack, deliberately (#568 gates the pack on this result), and
//! not a run: it writes every request and answer beside the claims so
//! the judging can be re-asked without the GPU.
//!
//! Usage:
//!   cargo run -p runner --features pdf --example rung1_accounts -- \
//!     --corpus <dir of *.pdf> --model <path.gguf> --out <dir> [--arm a|b|both] [--plan]

#[path = "rung1/mod.rs"]
mod rung1;

use rung1::{
    batches, chunks, closed_prompt, closed_schema, passage_id, prose_prompt, prose_schema,
    BATCH_CHARS, PROSE_CHARS,
};
use runner::document::read_document;
use runner::exec::{assert_grammar_safe, call_constrained, Endpoint};
use runner::sidecar::{binary_in, Sidecar, SidecarRuntime};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

struct Args {
    corpus: PathBuf,
    model: PathBuf,
    out: PathBuf,
    arm: String,
    plan: bool,
}

fn args() -> Args {
    let mut argv = std::env::args().skip(1);
    let (mut corpus, mut model, mut out) = (None, None, None);
    let (mut arm, mut plan) = ("both".to_owned(), false);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--corpus" => corpus = argv.next().map(PathBuf::from),
            "--model" => model = argv.next().map(PathBuf::from),
            "--out" => out = argv.next().map(PathBuf::from),
            "--arm" => arm = argv.next().unwrap_or_else(|| "both".to_owned()),
            "--plan" => plan = true,
            other => panic!("unknown flag {other}"),
        }
    }
    Args {
        corpus: corpus.expect("--corpus"),
        model: model.unwrap_or_default(),
        out: out.expect("--out"),
        arm,
        plan,
    }
}

fn main() {
    let args = args();
    fs::create_dir_all(args.out.join("raw")).expect("out dir");

    let mut documents = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&args.corpus)
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false))
        .collect();
    entries.sort();
    for (index, path) in entries.iter().enumerate() {
        let read = match read_document(path, index, Some(Path::new("sidecars"))) {
            Ok(read) => read,
            Err(e) => {
                println!("{}: unreadable ({e:?})", path.display());
                continue;
            }
        };
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let stem = name.split('.').next().unwrap_or(&name).to_owned();
        let all: Vec<(String, String)> = read
            .segments
            .iter()
            .map(|s| (s.ordinal.to_string(), s.text.clone()))
            .collect();
        let asked: Vec<(String, String)> = all
            .iter()
            .filter(|(_, t)| rung1::worth_asking(t))
            .cloned()
            .collect();
        let full_text = all
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        documents.push((stem, all.len(), asked, full_text));
    }

    let mut plan_calls = 0;
    for (stem, total, asked, full) in &documents {
        let closed = batches(asked.clone(), BATCH_CHARS).len();
        let prose = chunks(full, PROSE_CHARS).len();
        plan_calls += closed + prose;
        println!(
            "{stem}: {total} passages, {} asked, {closed} closed calls, {prose} prose calls",
            asked.len()
        );
    }
    println!("total calls: {plan_calls}");
    if args.plan {
        return;
    }

    let log = args.out.join("sidecar.log");
    let mut sidecar = Sidecar::spawn(
        &binary_in(Path::new("sidecars")),
        &args.model,
        &log,
        SidecarRuntime::default(),
    )
    .expect("spawn sidecar");
    sidecar
        .wait_until_ready(Duration::from_secs(600))
        .expect("sidecar ready");
    println!(
        "sidecar ready on port {} ({})",
        sidecar.port(),
        sidecar.device().unwrap_or_else(|| "device unknown".into())
    );
    let endpoint = Endpoint::local(sidecar.port());
    let cancel = AtomicBool::new(false);

    let closed = closed_schema();
    let prose = prose_schema();
    assert_grammar_safe(&closed).expect("closed schema is grammar-safe");
    assert_grammar_safe(&prose).expect("prose schema is grammar-safe");

    let mut claims = fs::File::create(args.out.join("claims.jsonl")).expect("claims file");
    let mut sources = fs::File::create(args.out.join("sources.jsonl")).expect("sources file");
    use std::io::Write;

    let started = Instant::now();
    let mut done = 0;
    for (stem, _total, asked, full) in &documents {
        writeln!(sources, "{}", json!({ "document": stem, "text": full })).expect("write source");

        if args.arm != "b" {
            for (n, batch) in batches(asked.clone(), BATCH_CHARS).into_iter().enumerate() {
                let prompt = closed_prompt(&batch);
                let label = format!("{stem}-closed-{n:03}");
                let answer = ask(&endpoint, &prompt, &closed, &cancel, &args.out, &label);
                done += 1;
                if let Some(answer) = answer {
                    let by_id: std::collections::BTreeMap<&str, &str> = batch
                        .iter()
                        .map(|(id, text)| (id.as_str(), text.as_str()))
                        .collect();
                    for item in answer["answers"].as_array().into_iter().flatten() {
                        // The model echoes the id as it saw it printed,
                        // brackets and all. Judging joins on the number,
                        // so join on the number here too.
                        let digits = passage_id(item["id"].as_str().unwrap_or_default());
                        let id = digits.as_str();
                        writeln!(
                            claims,
                            "{}",
                            json!({
                                "document": stem,
                                "arm": "closed",
                                "call": label,
                                "passage": id,
                                "passage_text": by_id.get(id).copied().unwrap_or_default(),
                                "kind": item["kind"],
                                "value": item["value"],
                                "quote": item["quote"],
                                "confidence": item["confidence"],
                            })
                        )
                        .expect("write claim");
                    }
                }
                progress(done, plan_calls, started);
            }
        }

        if args.arm != "a" {
            for (n, chunk) in chunks(full, PROSE_CHARS).into_iter().enumerate() {
                let prompt = prose_prompt(&chunk);
                let label = format!("{stem}-prose-{n:03}");
                let answer = ask(&endpoint, &prompt, &prose, &cancel, &args.out, &label);
                done += 1;
                if let Some(answer) = answer {
                    writeln!(
                        claims,
                        "{}",
                        json!({
                            "document": stem,
                            "arm": "prose",
                            "call": label,
                            "chunk": chunk,
                            "explanation": answer["explanation"],
                        })
                    )
                    .expect("write claim");
                }
                progress(done, plan_calls, started);
            }
        }
    }
    println!("\ndone: {done} calls in {:.1?}", started.elapsed());
}

fn ask(
    endpoint: &Endpoint,
    prompt: &str,
    schema: &Value,
    cancel: &AtomicBool,
    out: &Path,
    label: &str,
) -> Option<Value> {
    fs::write(out.join("raw").join(format!("{label}.request.txt")), prompt).ok();
    match call_constrained(endpoint, prompt, schema, cancel) {
        Ok(answer) => {
            fs::write(
                out.join("raw").join(format!("{label}.response.json")),
                serde_json::to_string_pretty(&answer).unwrap_or_default(),
            )
            .ok();
            Some(answer)
        }
        Err(e) => {
            fs::write(
                out.join("raw").join(format!("{label}.error.txt")),
                format!("{e}"),
            )
            .ok();
            println!("\n{label}: {e}");
            None
        }
    }
}

fn progress(done: usize, total: usize, started: Instant) {
    let each = started.elapsed().as_secs_f64() / done as f64;
    let left = (total.saturating_sub(done)) as f64 * each;
    print!(
        "\r{done}/{total} calls, {each:.1}s each, ~{:.0}m left      ",
        left / 60.0
    );
    use std::io::Write;
    std::io::stdout().flush().ok();
}
