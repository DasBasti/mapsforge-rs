use mapsforge_rs::{types::MapFile, view::View, visit::*};

struct Count(usize, usize);
impl TileVisitor for Count {
    fn poi(&mut self, _: &PoiInfo) { self.0 += 1; }
    fn way(&mut self, _: &WayInfo) { self.1 += 1; }
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let lat: f64 = std::env::args().nth(2).unwrap().parse().unwrap();
    let lon: f64 = std::env::args().nth(3).unwrap().parse().unwrap();
    let zoom: u8 = std::env::args().nth(4).unwrap().parse().unwrap();
    let mut map = MapFile::open(path).unwrap();
    let h = &map.header;
    println!("v{} size {} tile {} flags {:#x} bbox {:?}", h.file_version, h.file_size, h.tile_size, h.flags, h.bounding_box);
    println!("{} poi tags, {} way tags, intervals {:#?}", h.poi_tags.len(), h.way_tags.len(), h.zoom_interval_configuration);
    println!("inline: {:?}", h.way_tags.iter().chain(&h.poi_tags).filter(|t| t.contains("=%")).collect::<Vec<_>>());
    let sf = map.sub_file_for_zoom(zoom).unwrap();
    let base = map.header.zoom_interval_configuration[sf].base_zoom_level;
    let v = View::centered(lat, lon, zoom, 600, 448, map.header.tile_size);
    for (x, y) in v.visible_tiles(base).iter() {
        let idx = map.tile_index_of(sf, x, y);
        let e = idx.map(|i| map.tile_index_entry(sf, i).unwrap());
        let mut c = Count(0, 0);
        let r = idx.map(|i| map.visit_tile(sf, i, zoom, &mut c));
        println!("tile {x},{y} idx {idx:?} {e:?} -> {r:?} pois {} ways {}", c.0, c.1);
    }
}
