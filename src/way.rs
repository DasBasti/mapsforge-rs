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
        way.layer = ((special_byte & WAY_LAYER_BITMASK) >> WAY_LAYER_SHIFT) as i8;
        let number_of_tags = special_byte & WAY_NUMBER_OF_TAGS_BITMASK;

        way.tag_ids = Vec::with_capacity(number_of_tags as usize);
        let mut tags: Vec<Tag> = Vec::with_capacity(number_of_tags as usize);
        for _ in 0..number_of_tags {
            let tag_id = utils::read_vbe_u_int(reader)?;
            let tag_str = way_tags.get(tag_id).ok_or(MapforgeError::InvalidTagId(tag_id))?;
            way.tag_ids.push(tag_id);
            tags.push(tag::parse_tag(tag_str));
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
                let block = if way.double_delta_encoding {
                    decode_way_nodes_double_delta(reader, num_way_nodes)?
                } else {
                    decode_way_nodes_single_delta(reader, num_way_nodes)?
                };
                way.coordinate_blocks.push(block);
            }
        }
        ways.push(way);
    }

    Ok(ways)
}

fn clamp_longitude(longitude: f64) -> f64 {
    // Decoding near the international date line can return values slightly outside
    // [-180°, 180°] due to calculation precision
    if longitude < utils::LONGITUDE_MIN && (utils::LONGITUDE_MIN - longitude) < 0.001 {
        utils::LONGITUDE_MIN
    } else if longitude > utils::LONGITUDE_MAX && (longitude - utils::LONGITUDE_MAX) < 0.001 {
        utils::LONGITUDE_MAX
    } else {
        longitude
    }
}

fn decode_way_nodes_double_delta<R: ByteRead + ?Sized>(reader: &mut R, num_way_nodes: usize) -> Result<WayCoordinateBlock> {
    let mut block: WayCoordinateBlock = Default::default();
    if num_way_nodes == 0 {
        return Ok(block);
    }
    block.coordinates.reserve(num_way_nodes);

    let mut latitude = utils::read_microdegrees(reader)?;
    let mut longitude = utils::read_microdegrees(reader)?;
    block.initial_position = LatLong { latitude, longitude };
    block.coordinates.push(block.initial_position);

    let mut previous_single_delta_latitude = 0f64;
    let mut previous_single_delta_longitude = 0f64;
    for _ in 1..num_way_nodes {
        let single_delta_latitude = utils::read_microdegrees(reader)? + previous_single_delta_latitude;
        let single_delta_longitude = utils::read_microdegrees(reader)? + previous_single_delta_longitude;

        latitude += single_delta_latitude;
        longitude = clamp_longitude(longitude + single_delta_longitude);

        previous_single_delta_latitude = single_delta_latitude;
        previous_single_delta_longitude = single_delta_longitude;

        block.coordinates.push(LatLong { latitude, longitude });
    }

    Ok(block)
}

fn decode_way_nodes_single_delta<R: ByteRead + ?Sized>(reader: &mut R, num_way_nodes: usize) -> Result<WayCoordinateBlock> {
    let mut block: WayCoordinateBlock = Default::default();
    if num_way_nodes == 0 {
        return Ok(block);
    }
    block.coordinates.reserve(num_way_nodes);

    let mut latitude = utils::read_microdegrees(reader)?;
    let mut longitude = utils::read_microdegrees(reader)?;
    block.initial_position = LatLong { latitude, longitude };
    block.coordinates.push(block.initial_position);

    for _ in 1..num_way_nodes {
        latitude += utils::read_microdegrees(reader)?;
        longitude = clamp_longitude(longitude + utils::read_microdegrees(reader)?);
        block.coordinates.push(LatLong { latitude, longitude });
    }

    Ok(block)
}
