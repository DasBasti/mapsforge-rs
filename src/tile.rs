use std::{
    f64::consts::PI,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{
    error::MapforgeError,
    header::DEBUG_INFO_MASK,
    types::{BoundingBox, MapFile, MapHeader, TileIndexEntry},
    Result,
};

const INDEX_SIGNATURE: &str = "+++IndexStart+++";
const WATER_TILE_MASK: u8 = 0x80;

impl MapFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let header = MapHeader::read_from_file(&mut reader)?;

        let mut zoom_tile_indices = Vec::with_capacity(header.num_zoom_intervals as usize);

        for interval in &header.zoom_interval_configuration {
            reader.seek(SeekFrom::Start(interval.sub_file_start))?;

            if header.flags & DEBUG_INFO_MASK != 0 {
                let mut sig = [0u8; 16];
                reader.read_exact(&mut sig)?;
                let index_sig = String::from_utf8_lossy(&sig).trim().to_string();

                if index_sig != INDEX_SIGNATURE {
                    return Err(MapforgeError::InvalidIndexSignature);
                }
            }

            let total_tiles_index =
                Self::calculate_total_tiles(&header.bounding_box, interval.base_zoom_level);

            let mut tile_index = Vec::with_capacity(total_tiles_index as usize);
            for _ in 0..total_tiles_index {
                let mut bytes = [0u8; 5];

                reader.read_exact(&mut bytes)?;

                let is_water_tile = (bytes[0] & WATER_TILE_MASK) != 0;

                bytes[0] &= !WATER_TILE_MASK;

                let tile_index_entry = TileIndexEntry {
                    is_water: is_water_tile,
                    offset: u64::from_be_bytes([
                        0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4],
                    ]),
                };
                tile_index.push(tile_index_entry);
            }

            zoom_tile_indices.push(tile_index);
        }

        Ok(Self {
            header,
            reader,
            tile_indices: zoom_tile_indices,
        })
    }

    // pub fn get_tile_at(&mut self, lat: f64, lon:f64, zoom:u8) -> Result<Tile> {

    // }

    pub fn calculate_total_tiles(bounding_box: &BoundingBox, zoom: u8) -> u32 {
        // X calculation (longitude)
        let x_min =
            ((bounding_box.min_lon + 180.0) / 360.0 * 2_f64.powi(zoom as i32)).floor() as i64;
        let x_max =
            ((bounding_box.max_lon + 180.0) / 360.0 * 2_f64.powi(zoom as i32)).floor() as i64;

        // Y calculation (latitude)
        let lat_rad_min = bounding_box.min_lat.to_radians();
        let lat_rad_max = bounding_box.max_lat.to_radians();

        let y_min = ((1.0 - (lat_rad_max.tan() + 1.0 / lat_rad_max.cos()).ln() / PI) / 2.0
            * 2_f64.powi(zoom as i32))
        .floor() as i64;
        let y_max = ((1.0 - (lat_rad_min.tan() + 1.0 / lat_rad_min.cos()).ln() / PI) / 2.0
            * 2_f64.powi(zoom as i32))
        .floor() as i64;

        let num_x = (x_max - x_min + 1) as u32;
        let num_y = (y_max - y_min + 1) as u32;

        let total = num_x * num_y;

        total
    }
}
