//! Read one picture twice and show where the two readings disagree
//! (#412). Local only, macOS only, and no fixture here is a photograph.
//!
//!     cargo run -p runner --features vision --example compare_readings -- <path>
//!
//! The question it answers is the one #412 must not assume: how often
//! two readings of a *good* photograph disagree. If that is often, the
//! gate fires constantly and is worthless.

use runner::ocr::{read_page_with, Correction};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: compare_readings <picture>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(&path);

    let applied = read_page_with(path, Correction::Applied).expect("read with correction");
    let literal = read_page_with(path, Correction::Literal).expect("read without correction");

    let disputes = applied.disagreements(&literal);
    println!(
        "{} lines, {} disputed ({:.0}%)",
        applied.lines.len(),
        disputes.len(),
        100.0 * disputes.len() as f32 / applied.lines.len().max(1) as f32
    );
    for dispute in &disputes {
        println!("  corrected: {}", dispute.read);
        println!("  literal  : {}\n", dispute.also_read);
    }
}
