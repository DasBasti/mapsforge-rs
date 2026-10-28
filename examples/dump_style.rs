//! Prints the built-in stylesheet: `cargo run --example dump_style > styles/hiking.style`
use mapsforge_rs::stylesheet::Stylesheet;

fn main() {
    print!("{}", Stylesheet::builtin().to_text());
}
