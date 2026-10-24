use std::{fmt::Write, path::Path};

use mapsforge_rs::{types::MapFile, Result};

const MAP_PATH: &str = "../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map";
const SVG_PATH: &str = "target/tile.svg";

/// Parses a tile of the Madeira map and renders its POIs and ways to `target/tile.svg`.
/// Coordinates are the offsets from the tile's top-left corner in degrees, so
/// x = longitude offset and y = -latitude offset.
#[test]
fn test_visualize_tile() -> Result<()> {
    if !Path::new(MAP_PATH).exists() {
        eprintln!("skipping, {MAP_PATH} not found");
        return Ok(());
    }
    let mut map = MapFile::open(MAP_PATH)?;
    let tile = map.read_tile_at(32.6669, -16.9241, 14)?;

    const SCALE: f64 = 40_000.0;
    let mut svg = String::new();
    writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 900 900" width="900" height="900">"#).unwrap();
    writeln!(svg, r#"<rect width="900" height="900" fill="white" stroke="black"/>"#).unwrap();

    for way in &tile.ways {
        let is_highway = way
            .tags
            .iter()
            .flatten()
            .any(|t| t.key == "highway");
        let (stroke, width) = if is_highway { ("#555", 1.5) } else { ("#9bc", 0.7) };
        for block in &way.coordinate_blocks {
            let points: Vec<String> = block
                .coordinates
                .iter()
                .map(|c| format!("{:.1},{:.1}", c.longitude * SCALE, -c.latitude * SCALE))
                .collect();
            writeln!(svg, r#"<polyline points="{}" fill="none" stroke="{stroke}" stroke-width="{width}"/>"#, points.join(" ")).unwrap();
        }
    }
    for poi in &tile.pois {
        writeln!(
            svg,
            r#"<circle cx="{:.1}" cy="{:.1}" r="2.5" fill="red"/>"#,
            poi.position_offset.longitude * SCALE,
            -poi.position_offset.latitude * SCALE
        )
        .unwrap();
    }
    svg.push_str("</svg>\n");

    std::fs::write(SVG_PATH, &svg).unwrap();
    println!("wrote {SVG_PATH}: {} pois, {} ways", tile.pois.len(), tile.ways.len());
    assert!(!tile.ways.is_empty());
    Ok(())
}
