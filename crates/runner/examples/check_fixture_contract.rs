//! No model, endpoint or weights: exercise the actual disk fixture loader.
fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: check_fixture_contract BED");
    match runner::eval::fixture::fixtures_at(std::path::Path::new(&path)) {
        Ok(fixtures) if !fixtures.is_empty() => println!(
            "{} fixtures accepted by scoring version {}",
            fixtures.len(),
            runner::eval::SCORING_VERSION
        ),
        Ok(_) => {
            eprintln!("no fixtures found");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
