
use std::path::PathBuf;

use mapsforge_rs::{types::MapFile, Result};

#[test]
fn test_tile_index_reading() -> Result<()> {
    let test_file_path = PathBuf::from("test_data/test_map.map");
    let map_file = MapFile::open(test_file_path)?;

    
    assert_eq!(map_file.tile_indices.len(), map_file.header.num_zoom_intervals as usize);

    
    if let Some(first_interval) = map_file.tile_indices.first() {
        
        let expected_tiles = MapFile::calculate_total_tiles(
            &map_file.header.bounding_box,
            map_file.header.zoom_interval_configuration[0].base_zoom_level
        );
        assert_eq!(first_interval.len(), expected_tiles as usize);

        
        if let Some(first_tile) = first_interval.first() {
            assert!(first_tile.offset > 0 || first_tile.is_water);
        }
    }

    Ok(())
}