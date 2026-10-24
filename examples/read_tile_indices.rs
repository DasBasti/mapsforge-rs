use mapsforge_rs::types::MapFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut map = MapFile::open("../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map")?;

    println!("Header: {:#?}", map.header);

    for i in 0..map.header.zoom_interval_configuration.len() {
        let zoom_interval = &map.header.zoom_interval_configuration[i];
        let total_tiles = map.tile_count(i).unwrap();

        println!("\nZoom Interval {}", i);
        println!("Base zoom level: {}", zoom_interval.base_zoom_level);
        println!("Total tiles: {}", total_tiles);

        println!("\nFirst 5 tile entries:");
        for j in 0..total_tiles.min(5) as usize {
            let tile = map.tile_index_entry(i, j)?;
            println!("Tile {}: is_water: {}, offset: {}", j, tile.is_water, tile.offset);
        }
    }

    Ok(())
}
