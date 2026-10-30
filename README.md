# mapsforge-rs (WIP)

A Rust parser for Mapsforge binary map files. This library provides functionality to read and parse Mapsforge map files (.map), which are commonly used for offline mapping applications.

## Features

- [x] Map header parsing
  - Magic bytes validation
  - Version checking
  - Bounding box parsing
- [x] Map tile parsing (`MapFile::read_tile`, `read_tile_at`)
- [x] POI and way parsing (tags, names, coordinate blocks, single/double delta)
- [ ] Sub-file structure parsing (Coming soon)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mapsforge-rs = "0.1.0"
```

## Usage

### Reading Map Header

```rust
use std::fs::File;
use std::io::BufReader;
use mapsforge_rs::MapHeader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open and read the map file
    let file = File::open("path/to/map.map")?;
    let mut reader = BufReader::new(file);
    
    // Parse the header
    let header = MapHeader::read_from_file(&mut reader)?;
    
    // Access header information
    println!("Map bounds: {:?}", header.bounding_box);
    println!("File version: {}", header.file_version);
    println!("Tile size: {}", header.tile_size);
    
    Ok(())
}
```



### Stylesheets

How the map is drawn (colors, line widths, draw order, symbols, labels, hidden features) is
configured by a text stylesheet, see `src/stylesheet.rs` for the format and `styles/hiking.style`
for the built-in one. From Rust: `Stylesheet::parse(text)` and `StyleTable::from_header(&sheet, &header)`.
From the C API (`include/mapsforge.h`):

```c
mfr_style_t* style = mfr_style_parse(text, len, &err);  // NULL: see mfr_last_error()
mfr_set_style(map, style);                              // NULL = built-in
mfr_style_free(style);
```

`cargo run --example dump_style` prints the built-in stylesheet.

`styles/bw.style` is a black-and-white variant for 1-bit screens: only black and white inks, tints
by dithering (forest `black/25`, scrub `black/12`, water `black/hlines`, orchard `black/diag`),
steps and rails as `black/50`. Check a file with `cargo run --example check_style <file>`.
`styles/bike.style` is a variant for bike riding. Demo screens of both: [docs/STYLES.md](docs/STYLES.md).

## Requirements

- Rust 1.56 or higher
- `byteorder` crate for handling endianness
- No other external dependencies required

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Based on the [Mapsforge binary format](https://github.com/mapsforge/mapsforge/blob/master/docs/Specification-Binary-Map-File.md)
- Inspired by the Java implementation in the main Mapsforge project

## Development Status

This project is in active development. Current focus is on implementing basic file parsing capabilities.
