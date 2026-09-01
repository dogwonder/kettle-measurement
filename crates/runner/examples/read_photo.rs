//! Read one picture of a document and print what Vision made of it
//! (#399). Local only — the reader is macOS-only and behind a feature,
//! and no fixture in this repo is a photograph of anybody's post.
//!
//!     cargo run -p runner --features vision --example read_photo -- <path>

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: read_photo <picture>");
        std::process::exit(2);
    };
    match runner::ocr::read_page(std::path::Path::new(&path)) {
        Ok(reading) => {
            for line in &reading.lines {
                println!(
                    "{:.3}  x{:.3}-{:.3} y{:.3}  {}",
                    line.confidence, line.left, line.right, line.top, line.text
                );
            }
            println!("\n--- by geometry ---");
            for segment in runner::document::segments_from_pages(&[reading.page()]) {
                println!("[{}] {}", segment.ordinal, segment.text);
            }
            println!("\n--- as text ---");
            match reading.text() {
                Ok(text) => {
                    let segments = runner::document::segments_from_text(&text);
                    for segment in &segments {
                        println!("[{}] {}", segment.ordinal, segment.text);
                    }
                }
                Err(refused) => println!("refused: {refused}"),
            }
        }
        Err(e) => println!("could not read: {e}"),
    }
}
