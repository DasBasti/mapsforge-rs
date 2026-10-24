use core::f64::{self, consts::PI};

use alloc::{string::ToString, string::String, vec::Vec};

use crate::{
    blocks,
    error::MapforgeError,
    header::DEBUG_INFO_MASK,
    io::ByteSource,
    math, poi,
    types::{BoundingBox, MapFile, MapHeader, Tile, TileIndexEntry},
    utils, way, Result,
};

const INDEX_SIGNATURE: &[u8; 16] = b"+++IndexStart+++";
const WATER_TILE_MASK: u8 = 0x80;
const INDEX_ENTRY_SIZE: u64 = 5;

#[cfg(feature = "std")]
impl MapFile<std::io::BufReader<std::fs::File>> {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        Self::from_source(std::io::BufReader::new(file))
    }
}

impl<S: ByteSource> MapFile<S> {
    /// Parses the header and validates the index signature of every sub file.
    pub fn from_source(mut reader: S) -> Result<Self> {
        let header = MapHeader::read_from_file(&mut reader)?;

        if header.flags & DEBUG_INFO_MASK != 0 {
            for interval in &header.zoom_interval_configuration {
                reader.seek_to(interval.sub_file_start)?;
                let mut sig = [0u8; 16];
                reader.read_bytes(&mut sig)?;
                if &sig != INDEX_SIGNATURE {
                    return Err(MapforgeError::InvalidIndexSignature);
                }
            }
        }

        Ok(Self { header, reader })
    }

    /// Number of tiles (= index entries) of the given sub file.
    pub fn tile_count(&self, sub_file: usize) -> Option<u32> {
        let interval = self.header.zoom_interval_configuration.get(sub_file)?;
        Some(Self::calculate_total_tiles(&self.header.bounding_box, interval.base_zoom_level))
    }

    /// Reads one 5 byte tile index entry with a single seek.
    pub fn tile_index_entry(&mut self, sub_file: usize, tile_index: usize) -> Result<TileIndexEntry> {
        let interval = self
            .header
            .zoom_interval_configuration
            .get(sub_file)
            .ok_or(MapforgeError::InvalidTileIndex)?;
        if tile_index as u64 >= self.tile_count(sub_file).unwrap_or(0) as u64 {
            return Err(MapforgeError::InvalidTileIndex);
        }
        let sub_file_start = interval.sub_file_start;
        let signature_size = if self.header.flags & DEBUG_INFO_MASK != 0 { 16 } else { 0 };

        self.reader
            .seek_to(sub_file_start + signature_size + tile_index as u64 * INDEX_ENTRY_SIZE)?;
        let mut bytes = [0u8; 5];
        self.reader.read_bytes(&mut bytes)?;

        let is_water = bytes[0] & WATER_TILE_MASK != 0;
        bytes[0] &= !WATER_TILE_MASK;
        let offset = u64::from_be_bytes([0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]]);

        Ok(TileIndexEntry { is_water, offset, offset_abs: offset + sub_file_start })
    }

    /// Reads the tile with the given index (row-major, as stored in the tile index)
    /// of the given sub file, including all POIs and ways of all its zoom levels.
    pub fn read_tile(&mut self, sub_file: usize, tile_index: usize) -> Result<Tile> {
        let start = self.tile_index_entry(sub_file, tile_index)?.offset_abs;
        let debug = self.header.flags & DEBUG_INFO_MASK != 0;
        let zoom_interval = &self.header.zoom_interval_configuration[sub_file];

        self.reader.seek_to(start)?;

        let mut debug_signature = None;
        if debug {
            let mut sig = [0u8; 32];
            self.reader.read_bytes(&mut sig)?;
            debug_signature = Some(String::from_utf8_lossy(&sig).trim().to_string());
        }

        let zoom_table = blocks::read_zoom_table(&mut self.reader, zoom_interval)?;

        // offset to the first way, relative to the end of this field
        let first_way_offset = utils::read_vbe_u_int(&mut self.reader)?;
        let ways_start = self.reader.position()? + first_way_offset as u64;

        let mut pois = Vec::new();
        for &(poi_count, _) in &zoom_table {
            pois.extend(poi::process_pois(&mut self.reader, poi_count, &self.header.poi_tags, debug)?);
        }

        self.reader.seek_to(ways_start)?;
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

    /// Web-Mercator tile y (fractional) for a latitude in radians at `n` = 2^zoom tiles.
    fn mercator_y(lat_rad: f64, n: f64) -> f64 {
        (1.0 - math::ln(math::tan(lat_rad) + 1.0 / math::cos(lat_rad)) / PI) / 2.0 * n
    }

    pub fn calculate_total_tiles(bounding_box: &BoundingBox, zoom: u8) -> u32 {
        let n = (1u64 << zoom) as f64;

        let x_min = math::floor((bounding_box.min_lon + 180.0) / 360.0 * n) as i64;
        let x_max = math::floor((bounding_box.max_lon + 180.0) / 360.0 * n) as i64;

        // tile y grows southwards, so max_lat gives the smallest y
        let y_min = math::floor(Self::mercator_y(bounding_box.max_lat * PI / 180.0, n)) as i64;
        let y_max = math::floor(Self::mercator_y(bounding_box.min_lat * PI / 180.0, n)) as i64;

        let num_x = (x_max - x_min + 1) as u32;
        let num_y = (y_max - y_min + 1) as u32;

        num_x * num_y
    }

    pub fn get_tiles(lat_deg: f64, lon_deg: f64, zoom: u8) -> (i32, i32) {
        let n = (1u64 << zoom) as f64;

        let x = (n * (lon_deg + 180.0) / 360.0) as i32;
        let y = Self::mercator_y(lat_deg * PI / 180.0, n) as i32;

        (x, y)
    }

    pub fn tilex2long(x: u32, zoom: u8) -> f64 {
        x as f64 / (1u64 << zoom) as f64 * 360.0 - 180.0
    }

    pub fn tiley2lat(y: u32, zoom: u8) -> f64 {
        let n = PI - 2.0 * PI * y as f64 / (1u64 << zoom) as f64;
        180.0 / PI * math::atan(0.5 * (math::exp(n) - math::exp(-n)))
    }
}
