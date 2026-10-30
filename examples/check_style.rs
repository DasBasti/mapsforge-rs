//! Parses a stylesheet file and reports errors: `cargo run --example check_style styles/bw.style`
use mapsforge_rs::stylesheet::Stylesheet;

fn main() {
    let path = std::env::args().nth(1).expect("usage: check_style <file>");
    let text = std::fs::read_to_string(&path).unwrap();
    match Stylesheet::parse(&text) {
        Ok(_) => println!("{path}: ok"),
        Err(e) => {
            eprintln!("{path}: {e:?}");
            std::process::exit(1);
        }
    }
}
