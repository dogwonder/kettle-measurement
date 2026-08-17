//! Spike measurement for #283: how short can the obligations echo be
//! before mismatched-echo detection weakens?
//!
//! The echo is a pairing check, not a source of data — the runner
//! already holds every segment's text. So the question is purely: at
//! prefix length N, can two segments *in the same batch* still be told
//! apart? A collision is where a confabulated pairing would slip
//! through undetected.
//!
//! This is the harness behind the numbers in #283's spike comment, kept
//! rather than deleted because a measurement that decided against a
//! change should be re-runnable by whoever doubts it later:
//!
//!     cargo run --release -p runner --example prefix_collisions
//!
//! It reads the pack's own fixtures through `segments_from_text`, so it
//! segments exactly as a run does rather than approximating it. The two
//! findings it exists to support: within one letter's batch a 3-word
//! prefix is collision-free across all 710 fixtures, and once letters
//! share a batch nothing under 15 words is — which is why shortening
//! the echo and grouping letters into one fixture file cannot both be
//! had.

use std::collections::HashMap;
use std::path::Path;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "packs/app.kttl.letter-to-actions/fixtures".to_owned());
    let batch_size: usize = std::env::args()
        .nth(2)
        .and_then(|n| n.parse().ok())
        .unwrap_or(20);

    let mut fixtures: Vec<(String, Vec<runner::document::Segment>)> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect();
    entries.sort();
    for path in entries {
        let text = std::fs::read_to_string(&path).expect("read fixture");
        let segments = runner::document::segments_from_text(&text);
        fixtures.push((name(&path), segments));
    }

    let total_segments: usize = fixtures.iter().map(|(_, s)| s.len()).sum();
    let chars: usize = fixtures
        .iter()
        .flat_map(|(_, s)| s.iter())
        .map(|s| s.text.chars().count())
        .sum();
    let mut lens: Vec<usize> = fixtures
        .iter()
        .flat_map(|(_, s)| s.iter())
        .map(|s| s.text.chars().count())
        .collect();
    lens.sort_unstable();
    let mut counts: Vec<usize> = fixtures.iter().map(|(_, s)| s.len()).collect();
    counts.sort_unstable();

    println!("fixtures: {}", fixtures.len());
    println!("segments: {total_segments}");
    println!(
        "segments per fixture: min {} median {} max {}",
        counts.first().unwrap(),
        counts[counts.len() / 2],
        counts.last().unwrap()
    );
    println!(
        "segment chars: mean {:.0} median {} p90 {} max {}",
        chars as f64 / total_segments as f64,
        lens[lens.len() / 2],
        lens[lens.len() * 9 / 10],
        lens.last().unwrap()
    );
    println!(
        "batch size {batch_size} → batches per fixture: {}",
        fixtures
            .iter()
            .map(|(_, s)| s.len().div_ceil(batch_size))
            .max()
            .unwrap()
    );

    if let Ok(dump) = std::env::var("SEGMENT_DUMP") {
        let json: Vec<_> = fixtures
            .iter()
            .map(|(name, segments)| {
                serde_json::json!({
                    "fixture": name,
                    "segments": segments.iter().map(|s| &s.text).collect::<Vec<_>>(),
                })
            })
            .collect();
        std::fs::write(&dump, serde_json::to_string(&json).unwrap()).expect("write dump");
        println!("\nsegments dumped to {dump}");
    }

    println!("\n== collisions within a batch, by prefix length (words) ==");
    println!("words  colliding_fixtures  colliding_segments  echo_chars_mean  saving_vs_full");
    for n in 1..=12 {
        report(
            &fixtures,
            batch_size,
            chars,
            total_segments,
            n,
            words_prefix,
        );
    }

    println!("\n== collisions within a batch, by prefix length (chars) ==");
    println!("chars  colliding_fixtures  colliding_segments  echo_chars_mean  saving_vs_full");
    for n in [8, 12, 16, 24, 32, 48, 64] {
        report(
            &fixtures,
            batch_size,
            chars,
            total_segments,
            n,
            chars_prefix,
        );
    }

    // The two changes in #283 interact: grouping letters into one
    // fixture file puts more segments in one batch, and a prefix only
    // has to be unique *within a batch*. So the safe prefix length is a
    // function of how many letters share a file.
    println!("\n== shortest collision-free word prefix, by letters per batch ==");
    println!("letters  segments/batch  words  chars(equiv)");
    for group in [1usize, 2, 4, 10, 20, 50] {
        let merged: Vec<(String, Vec<runner::document::Segment>)> = fixtures
            .chunks(group)
            .map(|chunk| {
                let mut text = Vec::new();
                for (_, segments) in chunk {
                    text.extend(segments.iter().cloned());
                }
                (chunk[0].0.clone(), text)
            })
            .collect();
        let biggest = merged.iter().map(|(_, s)| s.len()).max().unwrap();
        let words = (1..=40)
            .find(|n| collisions(&merged, usize::MAX, *n, words_prefix).is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| ">40".to_owned());
        let chars = (4..=200)
            .find(|n| collisions(&merged, usize::MAX, *n, chars_prefix).is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| ">200".to_owned());
        println!("{group:>7}  {biggest:>14}  {words:>5}  {chars:>12}");
    }

    // The adversarial case the issue names: what do the survivors at
    // the shortest safe length actually look like?
    for n in 1..=12 {
        let examples = collisions(&fixtures, batch_size, n, words_prefix);
        if examples.is_empty() {
            println!("\nfirst collision-free word prefix: {n}");
            break;
        }
        if n <= 6 {
            println!("\n-- worst pairs at {n} word(s) ({} fixtures) --", {
                let mut f: Vec<&str> = examples.iter().map(|(f, _, _)| f.as_str()).collect();
                f.sort_unstable();
                f.dedup();
                f.len()
            });
            for (fixture, a, b) in examples.iter().take(3) {
                println!("  {fixture}\n    A: {}\n    B: {}", clip(a), clip(b));
            }
        }
    }
}

fn name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn clip(text: &str) -> String {
    let t: String = text.chars().take(110).collect();
    if text.chars().count() > 110 {
        format!("{t}…")
    } else {
        t
    }
}

fn words_prefix(text: &str, n: usize) -> String {
    text.split_whitespace()
        .take(n)
        .collect::<Vec<_>>()
        .join(" ")
}

fn chars_prefix(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Every pair of segments sharing a prefix inside one batch.
fn collisions(
    fixtures: &[(String, Vec<runner::document::Segment>)],
    batch_size: usize,
    n: usize,
    prefix: fn(&str, usize) -> String,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (fixture, segments) in fixtures {
        for batch in segments.chunks(batch_size) {
            let mut seen: HashMap<String, &str> = HashMap::new();
            for segment in batch {
                let key = prefix(&segment.text, n);
                if let Some(first) = seen.get(&key) {
                    // Two items with identical text are interchangeable:
                    // swapping their answers produces the same answer, so
                    // no echo of any length distinguishes them and none
                    // needs to. Only distinct texts sharing a prefix are
                    // a weakening of the check.
                    if *first != segment.text.as_str() {
                        out.push((fixture.clone(), (*first).to_owned(), segment.text.clone()));
                    }
                } else {
                    seen.insert(key, &segment.text);
                }
            }
        }
    }
    out
}

fn report(
    fixtures: &[(String, Vec<runner::document::Segment>)],
    batch_size: usize,
    full_chars: usize,
    total_segments: usize,
    n: usize,
    prefix: fn(&str, usize) -> String,
) {
    let pairs = collisions(fixtures, batch_size, n, prefix);
    let mut colliding: Vec<&str> = pairs.iter().map(|(f, _, _)| f.as_str()).collect();
    colliding.sort_unstable();
    colliding.dedup();
    let echo_chars: usize = fixtures
        .iter()
        .flat_map(|(_, s)| s.iter())
        .map(|s| prefix(&s.text, n).chars().count())
        .sum();
    println!(
        "{n:>5}  {:>18}  {:>18}  {:>15.1}  {:>13.0}%",
        colliding.len(),
        pairs.len(),
        echo_chars as f64 / total_segments as f64,
        100.0 * (1.0 - echo_chars as f64 / full_chars as f64),
    );
}
