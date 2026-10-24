use alloc::{string::{String, ToString}, vec::Vec};

use crate::{error::MapforgeError, io::ByteRead, tag::{self, TAG_KEY_HOUSE_NUMBER, TAG_KEY_NAME, TAG_KEY_REF}, types::{LatLong, Tag, Way, WayCoordinateBlock}, utils, Result};

const WAY_LAYER_BITMASK: u8 = 0xf0;
const WAY_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const WAY_LAYER_SHIFT: u8 = 4;
const WAY_FEATURE_NAME: u8 = 0x80;
const WAY_FEATURE_HOUSE_NUMBER: u8 = 0x40;
const WAY_FEATURE_REF: u8 = 0x20;
const WAY_FEATURE_LABEL_POSITION: u8 = 0x10;
const WAY_FEATURE_DATA_BLOCKS_BYTE: u8 = 0x08;
const WAY_FEATURE_DOUBLE_DELTA_ENCODING: u8 = 0x04;

/// Reads `ways_on_query_zoomlevel` ways. Coordinates are kept as stored in the file:
/// the first node of each block is an offset from the tile's top-left corner, all
/// following nodes are absolute offsets from that corner (deltas already accumulated).
pub fn process_ways<R: ByteRead + ?Sized>(reader: &mut R, ways_on_query_zoomlevel: usize, way_tags: &[String], debug: bool) -> Result<Vec<Way>> {
    let mut ways: Vec<Way> = Vec::with_capacity(ways_on_query_zoomlevel);

    for _ in 0..ways_on_query_zoomlevel {
        let mut way: Way = Default::default();

        if debug {
            let mut sig = [0u8; 32];
            reader.read_bytes(&mut sig)?;
            way.debug_signature = Some(String::from_utf8_lossy(&sig).trim().to_string());
        }

        // size of the rest of this way in bytes, lets callers skip ways
        let _way_data_size = utils::read_vbe_u_int(reader)?;

        way.sub_tile_bitmap = reader.read_u16_be()?;

        let special_byte = reader.read_u8()?;
        way.layer = ((special_byte & WAY_LAYER_BITMASK) >> WAY_LAYER_SHIFT) as i8 - tag::LAYER_OFFSET;
        let number_of_tags = special_byte & WAY_NUMBER_OF_TAGS_BITMASK;

        way.tag_ids = Vec::with_capacity(number_of_tags as usize);
        let mut tags: Vec<Tag> = Vec::with_capacity(number_of_tags as usize);
        for _ in 0..number_of_tags {
            let tag_id = utils::read_vbe_u_int(reader)?;
            let tag_str = way_tags.get(tag_id).ok_or(MapforgeError::InvalidTagId(tag_id))?;
            way.tag_ids.push(tag_id);
            tags.push(tag::parse_tag(tag_str));
        }
        for &id in &way.tag_ids {
            if let Some(kind) = tag::inline_value_kind(&way_tags[id]) {
                tag::skip_inline_value(reader, kind)?;
            }
        }

        let flags = reader.read_u8()?;

        if flags & WAY_FEATURE_NAME != 0 {
            let name = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_NAME, &name));
            way.name = Some(name);
        }
        if flags & WAY_FEATURE_HOUSE_NUMBER != 0 {
            let house_number = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_HOUSE_NUMBER, &house_number));
            way.house_number = Some(house_number);
        }
        if flags & WAY_FEATURE_REF != 0 {
            let reference = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_REF, &reference));
            way.reference = Some(reference);
        }
        way.tags = Some(tags);

        if flags & WAY_FEATURE_LABEL_POSITION != 0 {
            // difference to the first way node, lat-diff then lon-diff
            way.label_position = Some(LatLong {
                latitude: utils::read_microdegrees(reader)?,
                longitude: utils::read_microdegrees(reader)?,
            });
        }

        let number_of_way_data_blocks = if flags & WAY_FEATURE_DATA_BLOCKS_BYTE != 0 {
            utils::read_vbe_u_int(reader)?
        } else {
            1
        };

        way.double_delta_encoding = flags & WAY_FEATURE_DOUBLE_DELTA_ENCODING != 0;

        for _ in 0..number_of_way_data_blocks {
            let num_way_coordinate_blocks = utils::read_vbe_u_int(reader)?;
            for _ in 0..num_way_coordinate_blocks {
                let num_way_nodes = utils::read_vbe_u_int(reader)?;
                let mut block: WayCoordinateBlock = Default::default();
                decode_way_nodes(reader, num_way_nodes, way.double_delta_encoding, &mut block.coordinates)?;
                block.initial_position = block.coordinates.first().copied().unwrap_or_default();
                way.coordinate_blocks.push(block);
            }
        }
        ways.push(way);
    }

    Ok(ways)
}

/// Decodes `num_way_nodes` nodes into `out` (cleared first). Coordinates are offsets from the
/// tile's top-left corner: the first node is stored as is, all following ones as deltas
/// (single delta) or deltas of deltas (double delta) that are accumulated here.
pub(crate) fn decode_way_nodes<R: ByteRead + ?Sized>(
    reader: &mut R,
    num_way_nodes: usize,
    double_delta: bool,
    out: &mut Vec<LatLong>,
) -> Result<()> {
    out.clear();
    if num_way_nodes == 0 {
        return Ok(());
    }
    if num_way_nodes > utils::MAX_ITEMS {
        return Err(MapforgeError::Corrupt("too many way nodes"));
    }
    out.try_reserve(num_way_nodes).map_err(|_| MapforgeError::Corrupt("out of memory for way nodes"))?;

    let mut latitude = utils::read_microdegrees(reader)?;
    let mut longitude = utils::read_microdegrees(reader)?;
    out.push(LatLong { latitude, longitude });

    let mut delta_latitude = 0f64;
    let mut delta_longitude = 0f64;
    for _ in 1..num_way_nodes {
        let lat = utils::read_microdegrees(reader)?;
        let lon = utils::read_microdegrees(reader)?;
        if double_delta {
            delta_latitude += lat;
            delta_longitude += lon;
        } else {
            delta_latitude = lat;
            delta_longitude = lon;
        }
        latitude += delta_latitude;
        longitude += delta_longitude;
        out.push(LatLong { latitude, longitude });
    }

    Ok(())
}
