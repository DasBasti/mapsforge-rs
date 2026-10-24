//! Prints the tag tables of a map and how often each tag occurs in the tiles of a view.
//!   cargo run --release --example tag_stats <file.map> [lat lon zoom]
use std::collections::BTreeMap;

use mapsforge_rs::{types::MapFile, view::View, visit::*};

#[derive(Default)]
struct Count {
    poi: BTreeMap<usize, usize>,
    way: BTreeMap<usize, usize>,
    closed: BTreeMap<usize, usize>,
}
impl TileVisitor for Count {
    fn poi(&mut self, p: &PoiInfo) {
        for &t in p.tag_ids {
            *self.poi.entry(t).or_default() += 1;
        }
    }
    fn way(&mut self, w: &WayInfo) {
        let closed = w.blocks.first().is_some_and(|b| {
            b.len() > 3 && {
                let (a, z) = (b[0], b[b.len() - 1]);
                (a.latitude - z.latitude).abs() < 1e-7 && (a.longitude - z.longitude).abs() < 1e-7
            }
        });
        for &t in w.tag_ids {
            *self.way.entry(t).or_default() += 1;
            if closed {
                *self.closed.entry(t).or_default() += 1;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut map = MapFile::open(&args[1]).unwrap();
    if args.len() < 5 {
        println!("POI tags:");
        for (i, t) in map.header.poi_tags.iter().enumerate() {
            println!("  {i} {t}");
        }
        println!("way tags:");
        for (i, t) in map.header.way_tags.iter().enumerate() {
            println!("  {i} {t}");
        }
        return;
    }
    let (lat, lon, zoom): (f64, f64, u8) = (args[2].parse().unwrap(), args[3].parse().unwrap(), args[4].parse().unwrap());
    let sf = map.sub_file_for_zoom(zoom).unwrap();
    let base = map.header.zoom_interval_configuration[sf].base_zoom_level;
    let v = View::centered(lat, lon, zoom, 600, 448, map.header.tile_size);
    let mut c = Count::default();
    for (x, y) in v.visible_tiles(base).iter() {
        if let Some(i) = map.tile_index_of(sf, x, y) {
            map.visit_tile(sf, i, zoom, &mut c).unwrap();
        }
    }
    println!("ways (count / closed):");
    for (t, n) in &c.way {
        println!("  {:6} {:6} {}", n, c.closed.get(t).unwrap_or(&0), map.header.way_tags[*t]);
    }
    println!("pois:");
    for (t, n) in &c.poi {
        println!("  {:6} {}", n, map.header.poi_tags[*t]);
    }
}
