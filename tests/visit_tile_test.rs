use std::path::Path;

use mapsforge_rs::{
    types::{LatLong, MapFile},
    visit::{PoiInfo, TileVisitor, WayInfo},
    Result,
};

const MAP_PATH: &str = "../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map";

#[derive(Default)]
struct Collect {
    pois: Vec<(LatLong, i8, Vec<usize>, Option<String>)>,
    ways: Vec<(i8, Vec<usize>, Option<String>, Vec<Vec<LatLong>>)>,
    skip_all_ways: bool,
}

impl TileVisitor for Collect {
    fn poi(&mut self, p: &PoiInfo) {
        self.pois.push((p.position, p.layer, p.tag_ids.to_vec(), p.name.map(String::from)));
    }
    fn way(&mut self, w: &WayInfo) {
        self.ways.push((w.layer, w.tag_ids.to_vec(), w.name.map(String::from), w.blocks.to_vec()));
    }
    fn accept_way(&mut self, _bitmap: u16) -> bool {
        !self.skip_all_ways
    }
}

fn open() -> Option<MapFile<std::io::BufReader<std::fs::File>>> {
    if !Path::new(MAP_PATH).exists() {
        eprintln!("skipping, {MAP_PATH} not found");
        return None;
    }
    Some(MapFile::open(MAP_PATH).unwrap())
}

const FUNCHAL: (f64, f64) = (32.6669, -16.9241);

#[test]
fn visit_matches_read_tile() -> Result<()> {
    let Some(mut map) = open() else { return Ok(()) };
    let sub_file = map.sub_file_for_zoom(14).unwrap();
    let max_zoom = map.header.zoom_interval_configuration[sub_file].max_zoom_level;
    let idx = map.tile_index_at(sub_file, FUNCHAL.0, FUNCHAL.1).unwrap();

    let expected = map.read_tile(sub_file, idx)?;
    let mut got = Collect::default();
    map.visit_tile(sub_file, idx, max_zoom, &mut got)?;

    assert_eq!(got.pois.len(), expected.pois.len());
    assert_eq!(got.ways.len(), expected.ways.len());
    for (g, e) in got.pois.iter().zip(&expected.pois) {
        assert_eq!(g.0.latitude, e.position_offset.latitude);
        assert_eq!(g.1, e.layer);
        assert_eq!(g.2, e.tag_ids);
        assert_eq!(g.3, e.name);
    }
    for (g, e) in got.ways.iter().zip(&expected.ways) {
        assert_eq!(g.0, e.layer);
        assert_eq!(g.1, e.tag_ids);
        assert_eq!(g.2, e.name);
        assert_eq!(g.3.len(), e.coordinate_blocks.len());
        for (gb, eb) in g.3.iter().zip(&e.coordinate_blocks) {
            assert_eq!(gb.len(), eb.coordinates.len());
            assert!(gb.iter().zip(&eb.coordinates).all(|(a, b)| a.latitude == b.latitude && a.longitude == b.longitude));
        }
    }
    Ok(())
}

#[test]
fn visit_filters_by_zoom_and_skips_ways() -> Result<()> {
    let Some(mut map) = open() else { return Ok(()) };
    let sub_file = map.sub_file_for_zoom(14).unwrap();
    let interval = &map.header.zoom_interval_configuration[sub_file];
    let (min_zoom, max_zoom) = (interval.min_zoom_level, interval.max_zoom_level);
    let idx = map.tile_index_at(sub_file, FUNCHAL.0, FUNCHAL.1).unwrap();

    let mut low = Collect::default();
    map.visit_tile(sub_file, idx, min_zoom, &mut low)?;
    let mut high = Collect::default();
    map.visit_tile(sub_file, idx, max_zoom, &mut high)?;
    assert!(low.ways.len() < high.ways.len());
    assert!(low.pois.len() <= high.pois.len());

    let mut skipped = Collect { skip_all_ways: true, ..Default::default() };
    map.visit_tile(sub_file, idx, max_zoom, &mut skipped)?;
    assert!(skipped.ways.is_empty());
    assert_eq!(skipped.pois.len(), high.pois.len());

    // tiles outside the map are rejected
    assert!(map.tile_index_at(sub_file, 0.0, 0.0).is_none());
    Ok(())
}
