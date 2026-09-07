//! A worked example is written fresh, never copied from the bed.
//!
//! Decided 7 September 2026, the day a prompt edit made the letter bed
//! green by giving the worked example the bed's own failing sentences:
//! example 912 was byte-identical to a line in 84 committed three-asks
//! fixtures, 913 differed from twenty more by one date, and the
//! re-scored bed read 0.99. Whatever that number measured, it was no
//! longer independent of the prompt. The project's own norm had been
//! set five days earlier (`89ba0660`, "written fresh rather than drawn
//! from the bed") and a convention a session can forget is what a test
//! is for.
//!
//! Every pack that declares `examples` on a model step is checked, so a
//! new pack inherits the rule. An exception is a decision: it goes in
//! [`STAGED_BED_SENTENCES`] with the example id, a reason and a date,
//! and a stage naming an example that is no longer in the bed fails,
//! so an exception cannot outlive its reason.

use std::path::{Path, PathBuf};

/// Worked-example segments that are allowed to appear in the bed, each
/// with the reason it was allowed and when. Shape: pack id, example id,
/// reason, date.
const STAGED_BED_SENTENCES: &[(&str, u64, &str, &str)] = &[
    (
        "app.kttl.letter-to-actions",
        907,
        "the already-done conditional (#614, `de41c679`): the example was written as a \
         generalisation of a real letter and its six bed occurrences were authored in the \
         same commit as counter-examples, not lifted from a bed that already had them",
        "2026-09-03",
    ),
    (
        "app.kttl.letter-to-actions",
        909,
        "the pointing ask (#544, #612): the invoice shape's deontic sentence is the one \
         construction the bed and the example share by design, since the example teaches \
         where the date and the sum are read from and the bed measures that reading",
        "2026-09-04",
    ),
];

fn packs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
}

/// Every segment of every worked example a pack declares, with the
/// example's id.
fn example_segments(pack_dir: &Path) -> Vec<(u64, String)> {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(pack_dir.join("pack.json")).expect("a manifest"),
    )
    .expect("a manifest parses");
    let mut segments = Vec::new();
    for step in manifest["pipeline"].as_array().into_iter().flatten() {
        let Some(examples) = step["examples"].as_str() else {
            continue;
        };
        let file: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(pack_dir.join(examples)).expect("the examples file"),
        )
        .expect("the examples file parses");
        for result in file["results"].as_array().into_iter().flatten() {
            // Only passage-shaped examples carry a segment; a
            // classification example carries a descriptor and is not a
            // sentence the bed could have.
            if let (Some(id), Some(segment)) = (result["id"].as_u64(), result["segment"].as_str()) {
                segments.push((id, segment.to_owned()));
            }
        }
    }
    segments
}

/// The bed's text, every fixture concatenated.
fn bed_text(pack_dir: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(pack_dir.join("fixtures")) else {
        return String::new();
    };
    let mut text = String::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let readable = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("txt" | "csv" | "md")
        );
        if readable {
            if let Ok(fixture) = std::fs::read_to_string(&path) {
                text.push_str(&fixture);
                text.push('\n');
            }
        }
    }
    text
}

#[test]
fn a_worked_example_is_written_fresh_and_never_copied_from_the_bed() {
    let mut checked = 0;
    for entry in std::fs::read_dir(packs_dir()).expect("the packs directory") {
        let pack_dir = entry.expect("an entry").path();
        if !pack_dir.join("pack.json").exists() {
            continue;
        }
        let pack = pack_dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a pack directory name")
            .to_owned();
        let segments = example_segments(&pack_dir);
        if segments.is_empty() {
            continue;
        }
        checked += 1;
        let bed = bed_text(&pack_dir);
        for (id, segment) in segments {
            let staged = STAGED_BED_SENTENCES
                .iter()
                .find(|(staged_pack, staged_id, ..)| *staged_pack == pack && *staged_id == id);
            let in_bed = bed.contains(segment.as_str());
            match staged {
                Some((_, _, reason, date)) => assert!(
                    in_bed,
                    "{pack} example {id} is staged as a bed sentence (staged {date}: {reason}) \
                     but no fixture contains it any more — remove it from STAGED_BED_SENTENCES"
                ),
                None => assert!(
                    !in_bed,
                    "{pack} example {id} is a sentence the bed already contains:\n  {segment:?}\n\
                     A worked example teaches the model the bed's own answer, and the bed then \
                     measures nothing. Write it fresh, or stage it in STAGED_BED_SENTENCES with \
                     a reason and a date."
                ),
            }
        }
    }
    assert!(checked > 0, "no pack declares a worked example to check");
}
