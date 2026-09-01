//! #431: turn synthetic letters into *genuine* letter-pack output for
//! the study's letter corpus.
//!
//! The same rule `study_corpus` follows for statements (25 August
//! 2026): a report Kettle *would* produce is a different artefact from
//! one Kettle *did* produce, so this runs the real pipeline — the same
//! `run_pack` the desktop app calls, the same pack, the same model —
//! and whatever comes out is the corpus. Any error the pipeline makes
//! on its own is recorded in the file beside the bed's expected answer,
//! which is what the harness's "clean" controls are audited against.
//!
//! The letters come from the `kettle-examples` generator (27 August
//! 2026): invented by construction, never read by the study's author
//! before a session, and plentiful — so a pilot can draw ten the author
//! has not seen, where the ten hand-audited statements are ten the
//! author has read every row of.
//!
//! One file per letter, carrying everything a task needs and a stranger
//! needs to re-score it: the letter's text, its hash, the proposed
//! actions exactly as `propose_letter_actions` emitted them, and the
//! bed's expected obligations (`<stem>.expected.json`) as the gold the
//! seeds are authored against.
//!
//! Usage:
//!   cargo run -p runner --features pdf --example study_letters -- \
//!     --model <path.gguf> --letters ../kettle-examples/out-bed \
//!     [--count 14] [--skip 0] [--out fixtures/study/letters]

use runner::actions::propose_letter_actions;
use runner::exec::Endpoint;
use runner::run::{run_pack, Answers, Payload, Progress, RunOutcome};
use runner::run_dir::NoLog;
use runner::sidecar::{binary_in, Sidecar, SidecarRuntime};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const PACK: &str = "app.kttl.letter-to-actions";

fn blake3_of(path: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_reader(std::fs::File::open(path).expect("open letter"))
        .expect("read letter");
    hasher.finalize().to_hex().to_string()
}

fn main() {
    let mut argv = std::env::args().skip(1);
    let mut model = None::<PathBuf>;
    let mut letters = PathBuf::from("../kettle-examples/out-bed");
    let mut out = PathBuf::from("fixtures/study/letters");
    let mut count = 14usize;
    let mut skip = 0usize;
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--model" => model = argv.next().map(PathBuf::from),
            "--letters" => letters = argv.next().map(PathBuf::from).unwrap_or(letters),
            "--out" => out = argv.next().map(PathBuf::from).unwrap_or(out),
            "--count" => {
                count = argv
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--count takes a number")
            }
            // A second corpus the first has not spent. `--skip 18` is
            // how the author gets letters they have not read: the draw
            // is deterministic, so dropping the first N of it leaves
            // exactly what the previous corpus did not take.
            "--skip" => {
                skip = argv
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--skip takes a number")
            }
            other => panic!("unknown flag {other}"),
        }
    }
    let model = model.expect("--model");
    std::fs::create_dir_all(&out).expect("create output directory");

    let pack = runner::packs::load_pack(Path::new("packs").join(PACK).as_path())
        .unwrap_or_else(|e| panic!("could not load {PACK}: {e}"));

    // Every `.txt` with an expected answer beside it, in name order, so
    // the corpus is the same list on every machine. Injection variants
    // are real shapes and stay in.
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&letters)
        .expect("letters directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().map(|e| e == "txt").unwrap_or(false)
                && path.with_extension("expected.json").exists()
        })
        .collect();
    sources.sort();
    // Kinds in turn, not names in order: the first fourteen names are
    // all appointment and council letters, which state their dates and
    // so can carry no mis-resolution. A corpus of fourteen should hold
    // one or two of every kind, as the generator's own sets do.
    let kind_of = |path: &PathBuf| -> String {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match stem.find(|c: char| c.is_ascii_digit()) {
            Some(at) => stem[..at].trim_end_matches('-').to_owned(),
            None => stem,
        }
    };
    let mut by_kind: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for path in sources {
        let kind = kind_of(&path);
        match by_kind.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, paths)) => paths.push(path),
            None => by_kind.push((kind, vec![path])),
        }
    }
    let wanted = skip + count;
    let mut sources: Vec<PathBuf> = Vec::new();
    let mut round = 0;
    while sources.len() < wanted {
        let mut any = false;
        for (_, paths) in &by_kind {
            if let Some(path) = paths.get(round) {
                any = true;
                if sources.len() < wanted {
                    sources.push(path.clone());
                }
            }
        }
        if !any {
            break;
        }
        round += 1;
    }
    if skip >= sources.len() {
        panic!(
            "--skip {skip} leaves nothing: the bed holds {} letters",
            sources.len()
        );
    }
    let sources: Vec<PathBuf> = sources.split_off(skip);
    println!("{} letters across {} kinds", sources.len(), by_kind.len());

    let log = std::env::temp_dir().join("study-letters-sidecar.log");
    let mut sidecar = Sidecar::spawn(
        &binary_in(Path::new("sidecars")),
        &model,
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

    let answers = Answers::FromModel(Endpoint::local(sidecar.port()));
    let cancel = AtomicBool::new(false);
    let model_file = model
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    for (index, path) in sources.iter().enumerate() {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = format!("letter-{:02}", index + 1);
        let clock = Instant::now();
        let mut last = String::new();
        let outcome: RunOutcome = match run_pack(
            &pack,
            std::slice::from_ref(path),
            &answers,
            &cancel,
            &mut |progress: Progress| {
                if progress.step != last {
                    last = progress.step.to_owned();
                    print!("\r{stem}: {last}                    ");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            },
            &NoLog,
        ) {
            Ok(outcome) => outcome,
            Err(e) => {
                println!("\n{stem}: run failed ({e:?})");
                continue;
            }
        };
        let Payload::Extraction(extraction) = &outcome.payload else {
            println!("\n{stem}: not an extraction run");
            continue;
        };
        let actions = propose_letter_actions(extraction, &id);

        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(path.with_extension("expected.json")).expect("expected"),
        )
        .expect("expected.json parses");
        // Only the obligations the bed says exist: the `null` rows are
        // passages that ask nothing, and the harness's gold is what
        // *should* be on the page.
        let gold: Vec<serde_json::Value> = expected["obligations"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter(|row| !row["expect"].is_null())
                    .map(|row| {
                        serde_json::json!({
                            "id": row["id"],
                            "segment": row["segment"],
                            "kind": row["expect"]["kind"],
                            "party": row["expect"]["party"],
                            "deadline": row["expect"]["deadline"],
                            "anchor": row["expect"]["anchor"],
                            "due": row["expect"]["due"],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let file = serde_json::json!({
            "schema": "kettle/study-letter@0",
            "id": id,
            "source": {
                "file": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                "hash": format!("blake3:{}", blake3_of(path)),
                "text": std::fs::read_to_string(path).expect("letter text"),
            },
            "pack": { "id": pack.manifest.id, "version": pack.manifest.version },
            "model": model_file,
            "actions": actions,
            "expected": gold,
        });
        // The source letter travels with the artefact made from it.
        // `crates/privacy-audit`'s committed-documents guard refuses a
        // committed artefact that names or quotes a document this
        // repository does not have — the whole point being that an
        // artefact made from somebody's real records cannot be
        // vouched for. These letters are synthetic and their text is
        // already inside the JSON, so copying the file beside it costs
        // nothing and makes the corpus self-contained. Without it the
        // guard was red from the day the first corpus was committed.
        if let Some(name) = path.file_name() {
            std::fs::copy(path, out.join(name)).expect("copy the source letter");
        }
        let target = out.join(format!("{id}.json"));
        std::fs::write(
            &target,
            serde_json::to_string_pretty(&file).expect("serialise letter"),
        )
        .expect("write letter");
        println!(
            "\r{id} ← {stem}: {} actions proposed, {} expected, {} needs-review, {:.0}s        ",
            file["actions"]["actions"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0),
            file["expected"].as_array().map(Vec::len).unwrap_or(0),
            outcome.needs_review.len(),
            clock.elapsed().as_secs_f64(),
        );
    }
}
