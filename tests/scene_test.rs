//! Several maps in one scene: the shapes of all maps are drawn in one drawing order.
use std::io::Cursor;

use mapsforge_rs::{
    render::{DrawTarget, Point, Scene},
    style::{paint, StyleTable},
    stylesheet::Stylesheet,
    types::MapFile,
    view::View,
    Result,
};

const LAT: f64 = 49.143;
const LON: f64 = 7.765;
const BASE_ZOOM: u8 = 14;

/// Writes the numbers of the map format.
#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_be_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend(v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend(v.to_be_bytes());
    }
    fn vbe_u(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.u8((v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        self.u8(v as u8);
    }
    fn vbe_s(&mut self, v: i64) {
        let mut rest = v.unsigned_abs();
        while rest >= 0x40 {
            self.u8((rest & 0x7f) as u8 | 0x80);
            rest >>= 7;
        }
        self.u8(rest as u8 | if v < 0 { 0x40 } else { 0 });
    }
    fn degrees(&mut self, v: f64) {
        self.vbe_s((v * 1e6).round() as i64);
    }
    fn string(&mut self, s: &str) {
        self.vbe_u(s.len() as u64);
        self.0.extend(s.as_bytes());
    }
}

fn tile_of(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
    let n = (1u64 << zoom) as f64;
    let rad = lat.to_radians();
    (
        ((lon + 180.0) / 360.0 * n).floor() as u32,
        ((1.0 - (rad.tan() + 1.0 / rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor() as u32,
    )
}

fn tile_lat(y: u32, zoom: u8) -> f64 {
    let n = std::f64::consts::PI - 2.0 * std::f64::consts::PI * y as f64 / (1u64 << zoom) as f64;
    n.sinh().atan().to_degrees()
}

fn tile_lon(x: u32, zoom: u8) -> f64 {
    x as f64 / (1u64 << zoom) as f64 * 360.0 - 180.0
}

/// A map of the zoom 14 tile around LAT/LON with closed ways `(tag, ring)`, all in one tile.
fn build_map(ways: &[(&str, Vec<(f64, f64)>)], created: u64) -> Vec<u8> {
    let (x, y) = tile_of(LAT, LON, BASE_ZOOM);
    let (max_lat, min_lon) = (tile_lat(y, BASE_ZOOM), tile_lon(x, BASE_ZOOM));
    let (min_lat, max_lon) = (tile_lat(y + 1, BASE_ZOOM), tile_lon(x + 1, BASE_ZOOM));
    let tags: Vec<&str> = ways.iter().map(|(tag, _)| *tag).collect();

    let mut way_data = Writer::default();
    for (i, (_, ring)) in ways.iter().enumerate() {
        let mut body = Writer::default();
        body.u16(0xffff);
        body.u8((5 << 4) | 1); // layer 0, one tag
        body.vbe_u(i as u64);
        body.u8(0); // no name
        body.vbe_u(1); // one block
        body.vbe_u(ring.len() as u64);
        let mut previous = (max_lat, min_lon);
        for &(lat, lon) in ring {
            body.degrees(lat - previous.0);
            body.degrees(lon - previous.1);
            previous = (lat, lon);
        }
        way_data.vbe_u(body.0.len() as u64);
        way_data.0.extend(body.0);
    }

    let mut tile = Writer::default();
    for zoom in 12..=21 {
        tile.vbe_u(0);
        tile.vbe_u(if zoom == 12 { ways.len() as u64 } else { 0 });
    }
    tile.vbe_u(0); // no POIs
    tile.0.extend(way_data.0);

    let mut sub_file = Writer::default();
    sub_file.u8(0);
    sub_file.u32(5); // the tile starts after its index entry
    sub_file.0.extend(tile.0);

    let mut header = Writer::default();
    header.u64(0);
    header.u64(created);
    for v in [min_lat + 1e-6, min_lon + 1e-6, max_lat - 1e-6, max_lon - 1e-6] {
        header.i32((v * 1e6).round() as i32);
    }
    header.u16(256);
    header.string("Mercator");
    header.u8(0);
    header.u16(0); // POI tags
    header.u16(tags.len() as u16);
    for tag in &tags {
        header.string(tag);
    }
    header.u8(1);
    let header_size = 4 + header.0.len() + 3 + 16;
    header.u8(BASE_ZOOM);
    header.u8(12);
    header.u8(21);
    header.u64((20 + 4 + header_size) as u64);
    header.u64(sub_file.0.len() as u64);

    let mut file = Writer::default();
    file.0.extend(b"mapsforge binary OSM");
    file.u32(header_size as u32);
    file.u32(3);
    file.0.extend(header.0);
    file.0.extend(sub_file.0);
    let size = file.0.len() as u64;
    file.0[28..36].copy_from_slice(&size.to_be_bytes());
    file.0
}

fn square(half: f64) -> Vec<(f64, f64)> {
    vec![
        (LAT - half, LON - half),
        (LAT - half, LON + half),
        (LAT + half, LON + half),
        (LAT + half, LON - half),
        (LAT - half, LON - half),
    ]
}

/// The colors of the filled polygons in drawing order.
#[derive(Default)]
struct Fills(Vec<u32>);

impl DrawTarget for Fills {
    fn fill_polygon(&mut self, _rings: &[&[Point]], color: u32) {
        self.0.push(color);
    }
    fn polyline(&mut self, _points: &[Point], _color: u32, _width: u8) {}
    fn point(&mut self, _at: Point, _color: u32, _radius: u8) {}
}

fn open(bytes: Vec<u8>) -> Result<(MapFile<Cursor<Vec<u8>>>, StyleTable)> {
    let map = MapFile::from_source(Cursor::new(bytes))?;
    let styles = StyleTable::from_header(&Stylesheet::builtin(), &map.header);
    Ok((map, styles))
}

fn view() -> View {
    View::centered(LAT, LON, 16, 448, 600, 256)
}

#[test]
fn the_land_of_one_map_lies_below_the_forest_of_the_other() -> Result<()> {
    let (mut forest, forest_styles) = open(build_map(&[("landuse=forest", square(0.001))], 1))?;
    // a map with white land polygons, like the official maps have around their coastlines
    let (mut land, land_styles) = open(build_map(&[("natural=nosea", square(0.003))], 2))?;
    let forest_color = forest_styles.way_style(&[0]).unwrap().color;
    let land_color = land_styles.way_style(&[0]).unwrap().color;
    assert_ne!(forest_color, land_color);

    // one after the other the land covers the forest
    let mut separate = Fills::default();
    forest.render(&view(), &forest_styles, &mut separate)?;
    land.render(&view(), &land_styles, &mut separate)?;
    assert_eq!(separate.0, vec![forest_color, land_color]);

    // in one scene the forest is drawn on the land, whatever the order of the maps
    let mut scene = Scene::new();
    assert_eq!(forest.collect(&view(), &forest_styles, &mut scene)?, 1);
    assert_eq!(land.collect(&view(), &land_styles, &mut scene)?, 1);
    let mut together = Fills::default();
    scene.draw(&mut together);
    assert_eq!(together.0, vec![land_color, forest_color]);

    // cleared, the scene can be used for the next screen
    scene.clear();
    assert!(scene.is_empty());
    Ok(())
}

#[test]
fn render_is_collect_and_draw() -> Result<()> {
    let bytes = build_map(&[("landuse=forest", square(0.001)), ("natural=water", square(0.0005))], 7);
    let (mut map, styles) = open(bytes)?;
    assert_eq!(map.header.creation_date, 7);

    let mut rendered = Fills::default();
    map.render(&view(), &styles, &mut rendered)?;
    let mut scene = Scene::new();
    map.collect(&view(), &styles, &mut scene)?;
    let mut drawn = Fills::default();
    scene.draw(&mut drawn);
    assert_eq!(rendered.0, drawn.0);
    assert_eq!(rendered.0.len(), 2);
    assert!(paint::coverage(rendered.0[0]) < 16, "the forest is a pattern");
    Ok(())
}
