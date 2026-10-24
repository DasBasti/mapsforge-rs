use std::{
    f64::{self, consts::PI},
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

use crate::{
    error::MapforgeError,
    header::DEBUG_INFO_MASK,
    blocks,
    poi, way,
    types::{BoundingBox, MapFile, MapHeader, Tile, TileIndexEntry},
    utils,
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

                let offset =u64::from_be_bytes([
                        0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4],
                    ]);

                let tile_index_entry = TileIndexEntry {
                    is_water: is_water_tile,
                    offset,
                    offset_abs: offset+interval.sub_file_start,
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

    /// Reads the tile with the given index (row-major, as stored in the tile index)
    /// of the given sub file, including all POIs and ways of all its zoom levels.
    pub fn read_tile(&mut self, sub_file: usize, tile_index: usize) -> Result<Tile> {
        let entry = self
            .tile_indices
            .get(sub_file)
            .and_then(|t| t.get(tile_index))
            .ok_or(MapforgeError::InvalidTileIndex)?;
        let start = entry.offset_abs;
        let debug = self.header.flags & DEBUG_INFO_MASK != 0;
        let zoom_interval = &self.header.zoom_interval_configuration[sub_file];

        self.reader.seek(SeekFrom::Start(start))?;

        let mut debug_signature = None;
        if debug {
            let mut sig = [0u8; 32];
            self.reader.read_exact(&mut sig)?;
            debug_signature = Some(String::from_utf8_lossy(&sig).trim().to_string());
        }

        let zoom_table = blocks::read_zoom_table(&mut self.reader, zoom_interval)?;

        // offset to the first way, relative to the end of this field
        let first_way_offset = utils::read_vbe_u_int(&mut self.reader)?;
        let ways_start = self.reader.stream_position()? + first_way_offset as u64;

        let mut pois = Vec::new();
        for &(poi_count, _) in &zoom_table {
            pois.extend(poi::process_pois(&mut self.reader, poi_count, &self.header.poi_tags, debug)?);
        }

        self.reader.seek(SeekFrom::Start(ways_start))?;
        let mut ways = Vec::new();
        for &(_, way_count) in &zoom_table {
            ways.extend(way::process_ways(&mut self.reader, way_count, &self.header.way_tags, debug)?);
        }

        Ok(Tile { debug_signature, zoom_table, first_way_offset, pois, ways })
    }

    /// Reads the tile covering `lat`/`lon` from the sub file serving `zoom`.
    pub fn read_tile_at(&mut self, lat: f64, lon: f64, zoom: u8) -> Result<Tile> {
        let sub_file = self
            .header
            .zoom_interval_configuration
            .iter()
            .position(|z| (z.min_zoom_level..=z.max_zoom_level).contains(&zoom))
            .ok_or(MapforgeError::InvalidTileIndex)?;
        let base_zoom = self.header.zoom_interval_configuration[sub_file].base_zoom_level;

        let bbox = self.header.bounding_box;
        let (x_min, y_min) = Self::get_tiles(bbox.max_lat, bbox.min_lon, base_zoom);
        let (x_max, _) = Self::get_tiles(bbox.min_lat, bbox.max_lon, base_zoom);
        let (x, y) = Self::get_tiles(lat, lon, base_zoom);
        let columns = x_max - x_min + 1;
        if x < x_min || x > x_max || y < y_min {
            return Err(MapforgeError::InvalidTileIndex);
        }

        self.read_tile(sub_file, ((y - y_min) * columns + (x - x_min)) as usize)
    }
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

        

        num_x * num_y
    }
    
    pub fn get_tiles(lat_deg: f64, lon_deg: f64, zoom: u8) -> (i32, i32) {
        let n = (1 << zoom as i32) as f64;

        let x = (n * (lon_deg + 180.0) / 360.0) as i32;

        let lat_rad = lat_deg.to_radians();
        let y = (n * (1.0 - (lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / PI) / 2.0) as i32;

        (x, y)
    }

    fn tilex2long(x: u32, zoom: u8) -> f64
    {
        x as f64 / (1 << zoom) as f64 * 360.0 - 180.0
    }
    
    fn tiley2lat(y: u32, zoom: u8) -> f64
    {
        let n = PI - 2.0 * PI * y as f64 / (1 << zoom) as f64;
        180.0 / PI * (0.5 * (n.exp()) - (-n).exp()).atan()
    }

}
