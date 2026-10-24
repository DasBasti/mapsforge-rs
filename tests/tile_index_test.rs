
use std::path::PathBuf;

use mapsforge_rs::{types::MapFile, Result};

#[test]
fn test_tile_index_reading() -> Result<()> {
    let test_file_path = PathBuf::from("test_data/test_map.map");
    let map_file = MapFile::open(test_file_path)?;

    
    let mut map_file = map_file;
    assert!(map_file.header.num_zoom_intervals > 0);

    for sub_file in 0..map_file.header.zoom_interval_configuration.len() {
        let expected_tiles = MapFile::<std::io::BufReader<std::fs::File>>::calculate_total_tiles(
            &map_file.header.bounding_box,
            map_file.header.zoom_interval_configuration[sub_file].base_zoom_level,
        );
        assert_eq!(map_file.tile_count(sub_file), Some(expected_tiles));

        let first_tile = map_file.tile_index_entry(sub_file, 0)?;
        assert!(first_tile.offset > 0 || first_tile.is_water);

        // one past the last entry must be rejected
        assert!(map_file.tile_index_entry(sub_file, expected_tiles as usize).is_err());
    }

    Ok(())
}
