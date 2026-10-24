use mapsforge_rs::types::MapFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut map = MapFile::open("../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map")?;

    // Funchal at the sub file's base zoom level
    let tile = map.read_tile_at(32.6669, -16.9241, 14)?;
    println!("zoom table: {:?}", tile.zoom_table);
    println!("{} pois, {} ways", tile.pois.len(), tile.ways.len());
    for poi in tile.pois.iter().take(5) {
        println!("{poi:?}");
    }
    for way in tile.ways.iter().take(5) {
        println!("{way:?}");
    }

    Ok(())
}
