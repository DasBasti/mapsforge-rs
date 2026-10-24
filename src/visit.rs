//! Streaming tile decoding: features are handed to a [`TileVisitor`] as they are read,
//! reusing scratch buffers, so no per-tile `Vec<Way>`/`Vec<POI>` has to fit in RAM.
use alloc::{string::String, vec::Vec};

use crate::{
    blocks,
    error::MapforgeError,
    header::DEBUG_INFO_MASK,
    io::{ByteRead, ByteSource},
    tag,
    types::{LatLong, MapFile},
    utils, way, Result,
};

const POI_LAYER_BITMASK: u8 = 0xf0;
const POI_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const POI_FEATURE_NAME: u8 = 0x80;
const POI_FEATURE_HOUSE_NUMBER: u8 = 0x40;
const POI_FEATURE_ELEVATION: u8 = 0x20;

const WAY_LAYER_BITMASK: u8 = 0xf0;
const WAY_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const WAY_FEATURE_NAME: u8 = 0x80;
const WAY_FEATURE_HOUSE_NUMBER: u8 = 0x40;
const WAY_FEATURE_REF: u8 = 0x20;
const WAY_FEATURE_LABEL_POSITION: u8 = 0x10;
const WAY_FEATURE_DATA_BLOCKS_BYTE: u8 = 0x08;
const WAY_FEATURE_DOUBLE_DELTA_ENCODING: u8 = 0x04;

/// A POI as read from the file. Borrowed data is only valid during the callback.
#[derive(Debug)]
pub struct PoiInfo<'a> {
    /// offset from the tile's top-left corner in degrees
    pub position: LatLong,
    pub layer: i8,
    /// indices into `MapHeader::poi_tags`
    pub tag_ids: &'a [usize],
    pub name: Option<&'a str>,
    pub house_number: Option<&'a str>,
    pub elevation: Option<i32>,
}

/// A way as read from the file. Borrowed data is only valid during the callback.
#[derive(Debug)]
pub struct WayInfo<'a> {
    pub sub_tile_bitmap: u16,
    pub layer: i8,
    /// indices into `MapHeader::way_tags`
    pub tag_ids: &'a [usize],
    pub name: Option<&'a str>,
    pub house_number: Option<&'a str>,
    pub reference: Option<&'a str>,
    /// label position offset to the first node
    pub label_position: Option<LatLong>,
    /// Coordinate blocks; for polygons the first one is the outer ring and the following
    /// ones are inner rings. Coordinates are degree offsets from the tile's top-left corner.
    pub blocks: &'a [Vec<LatLong>],
}

pub trait TileVisitor {
    fn poi(&mut self, poi: &PoiInfo);
    fn way(&mut self, way: &WayInfo);
    /// Called before the way's geometry is decoded; returning `false` skips it entirely.
    /// `sub_tile_bitmap` has one bit per cell of the tile's 4x4 sub tile grid (bit 15 is
    /// the top-left cell), so a visitor drawing only part of a tile can cull here.
    fn accept_way(&mut self, _sub_tile_bitmap: u16) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TileMeta {
    pub sub_file: usize,
    pub is_water: bool,
}

#[derive(Default)]
struct Scratch {
    tag_ids: Vec<usize>,
    name: Vec<u8>,
    house_number: Vec<u8>,
    reference: Vec<u8>,
    blocks: Vec<Vec<LatLong>>,
}

fn read_signature<R: ByteRead + ?Sized>(reader: &mut R) -> Result<()> {
    let mut sig = [0u8; 32];
    reader.read_bytes(&mut sig)
}

fn read_tag_ids<R: ByteRead + ?Sized>(reader: &mut R, count: u8, table_len: usize, out: &mut Vec<usize>) -> Result<()> {
    out.clear();
    for _ in 0..count {
        let id = utils::read_vbe_u_int(reader)?;
        if id >= table_len {
            return Err(MapforgeError::InvalidTagId(id));
        }
        out.push(id);
    }
    Ok(())
}

fn read_optional_str<'a, R: ByteRead + ?Sized>(
    reader: &mut R,
    present: bool,
    buf: &'a mut Vec<u8>,
) -> Result<Option<&'a str>> {
    if present {
        Ok(Some(utils::read_vbe_u_buf(reader, buf)?))
    } else {
        Ok(None)
    }
}

impl<S: ByteSource> MapFile<S> {
    /// Sub file serving `zoom`, if any.
    pub fn sub_file_for_zoom(&self, zoom: u8) -> Option<usize> {
        self.header
            .zoom_interval_configuration
            .iter()
            .position(|z| (z.min_zoom_level..=z.max_zoom_level).contains(&zoom))
    }

    /// Index of the tile covering `lat`/`lon` within `sub_file`, or `None` outside the map.
    pub fn tile_index_at(&self, sub_file: usize, lat: f64, lon: f64) -> Option<usize> {
        let base_zoom = self.header.zoom_interval_configuration.get(sub_file)?.base_zoom_level;
        let (x, y) = Self::get_tiles(lat, lon, base_zoom);
        self.tile_index_of(sub_file, x, y)
    }

    /// Index of tile `x`/`y` (at the sub file's base zoom) within `sub_file`.
    pub fn tile_index_of(&self, sub_file: usize, x: i32, y: i32) -> Option<usize> {
        let base_zoom = self.header.zoom_interval_configuration.get(sub_file)?.base_zoom_level;
        let bbox = self.header.bounding_box;
        let (x_min, y_min) = Self::get_tiles(bbox.max_lat, bbox.min_lon, base_zoom);
        let (x_max, y_max) = Self::get_tiles(bbox.min_lat, bbox.max_lon, base_zoom);
        if x < x_min || x > x_max || y < y_min || y > y_max {
            return None;
        }
        Some(((y - y_min) * (x_max - x_min + 1) + (x - x_min)) as usize)
    }

    /// Streams the features of a tile that are visible at `zoom` (features of all zoom
    /// table rows up to and including `zoom`) to `visitor`.
    pub fn visit_tile<V: TileVisitor>(
        &mut self,
        sub_file: usize,
        tile_index: usize,
        zoom: u8,
        visitor: &mut V,
    ) -> Result<TileMeta> {
        let entry = self.tile_index_entry(sub_file, tile_index)?;
        let debug = self.header.flags & DEBUG_INFO_MASK != 0;
        let interval = &self.header.zoom_interval_configuration[sub_file];
        let rows = (zoom.clamp(interval.min_zoom_level, interval.max_zoom_level) - interval.min_zoom_level) as usize + 1;

        self.reader.seek_to(entry.offset_abs)?;
        if debug {
            read_signature(&mut self.reader)?;
        }

        let zoom_table = blocks::read_zoom_table(&mut self.reader, interval)?;
        let first_way_offset = utils::read_vbe_u_int(&mut self.reader)?;
        let ways_start = self.reader.position()? + first_way_offset as u64;

        let (poi_count, way_count) = zoom_table[..rows]
            .iter()
            .fold((0, 0), |(p, w), &(pois, ways)| (p + pois, w + ways));

        let mut scratch = Scratch::default();
        let reader = &mut self.reader;
        let poi_tags = &self.header.poi_tags;
        let way_tags = &self.header.way_tags;

        for _ in 0..poi_count {
            visit_poi(reader, debug, poi_tags, &mut scratch, visitor)?;
        }

        reader.seek_to(ways_start)?;
        for _ in 0..way_count {
            visit_way(reader, debug, way_tags, &mut scratch, visitor)?;
        }

        Ok(TileMeta { sub_file, is_water: entry.is_water })
    }
}

fn visit_poi<R: ByteRead + ?Sized, V: TileVisitor>(
    reader: &mut R,
    debug: bool,
    poi_tags: &[String],
    scratch: &mut Scratch,
    visitor: &mut V,
) -> Result<()> {
    if debug {
        read_signature(reader)?;
    }
    let position = LatLong {
        latitude: utils::read_microdegrees(reader)?,
        longitude: utils::read_microdegrees(reader)?,
    };
    let special_byte = reader.read_u8()?;
    let layer = ((special_byte & POI_LAYER_BITMASK) >> 4) as i8 - tag::LAYER_OFFSET;
    read_tag_ids(reader, special_byte & POI_NUMBER_OF_TAGS_BITMASK, poi_tags.len(), &mut scratch.tag_ids)?;

    let flags = reader.read_u8()?;
    let name = read_optional_str(reader, flags & POI_FEATURE_NAME != 0, &mut scratch.name)?;
    let house_number = read_optional_str(reader, flags & POI_FEATURE_HOUSE_NUMBER != 0, &mut scratch.house_number)?;
    let elevation = if flags & POI_FEATURE_ELEVATION != 0 {
        Some(utils::read_vbe_s_int(reader)? as i32)
    } else {
        None
    };

    visitor.poi(&PoiInfo { position, layer, tag_ids: &scratch.tag_ids, name, house_number, elevation });
    Ok(())
}

fn visit_way<R: ByteSource + ?Sized, V: TileVisitor>(
    reader: &mut R,
    debug: bool,
    way_tags: &[String],
    scratch: &mut Scratch,
    visitor: &mut V,
) -> Result<()> {
    if debug {
        read_signature(reader)?;
    }
    // size of the rest of this way, lets us skip it
    let way_data_size = utils::read_vbe_u_int(reader)?;
    let data_start = reader.position()?;

    let sub_tile_bitmap = reader.read_u16_be()?;
    if !visitor.accept_way(sub_tile_bitmap) {
        return reader.seek_to(data_start + way_data_size as u64);
    }

    let special_byte = reader.read_u8()?;
    let layer = ((special_byte & WAY_LAYER_BITMASK) >> 4) as i8 - tag::LAYER_OFFSET;
    read_tag_ids(reader, special_byte & WAY_NUMBER_OF_TAGS_BITMASK, way_tags.len(), &mut scratch.tag_ids)?;

    let flags = reader.read_u8()?;
    let name = read_optional_str(reader, flags & WAY_FEATURE_NAME != 0, &mut scratch.name)?;
    let house_number = read_optional_str(reader, flags & WAY_FEATURE_HOUSE_NUMBER != 0, &mut scratch.house_number)?;
    let reference = read_optional_str(reader, flags & WAY_FEATURE_REF != 0, &mut scratch.reference)?;

    let label_position = if flags & WAY_FEATURE_LABEL_POSITION != 0 {
        Some(LatLong {
            latitude: utils::read_microdegrees(reader)?,
            longitude: utils::read_microdegrees(reader)?,
        })
    } else {
        None
    };

    let number_of_way_data_blocks = if flags & WAY_FEATURE_DATA_BLOCKS_BYTE != 0 {
        utils::read_vbe_u_int(reader)?
    } else {
        1
    };
    let double_delta = flags & WAY_FEATURE_DOUBLE_DELTA_ENCODING != 0;

    let mut used = 0;
    for _ in 0..number_of_way_data_blocks {
        let num_blocks = utils::read_vbe_u_int(reader)?;
        for _ in 0..num_blocks {
            let num_nodes = utils::read_vbe_u_int(reader)?;
            if scratch.blocks.len() <= used {
                scratch.blocks.push(Vec::new());
            }
            way::decode_way_nodes(reader, num_nodes, double_delta, &mut scratch.blocks[used])?;
            used += 1;
        }
    }

    visitor.way(&WayInfo {
        sub_tile_bitmap,
        layer,
        tag_ids: &scratch.tag_ids,
        name,
        house_number,
        reference,
        label_position,
        blocks: &scratch.blocks[..used],
    });
    Ok(())
}
