use std::{io::{BufReader, Read}};

use byteorder::ReadBytesExt;

use crate::{tag::{self, TAG_KEY_HOUSE_NUMBER, TAG_KEY_NAME, TAG_KEY_REF}, types::{LatLong, Tag, Way, WayCoordinateBlock}, utils, Result};

const WAY_LAYER_BITMASK: u8 = 0xf0;
const WAY_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const WAY_LAYER_SHIFT: u8 = 4;
const WAY_FEATURE_NAME: u8 = 0x80;
const WAY_FEATURE_HOUSE_NUMBER: u8 = 0x40;
const WAY_FEATURE_REF: u8 = 0x20;
const WAY_FEATURE_LABEL_POSITION: u8 = 0x10;
const WAY_FEATURE_DATA_BLOCKS_BYTE: u8 = 0x08;
const WAY_FEATURE_DOUBLE_DELTA_ENCODING: u8 = 0x04;

pub fn process_ways<R: Read>(reader: &mut BufReader<R>, ways_on_query_zoomlevel: usize, way_tags: &Vec<String>, debug: bool) -> Result<Vec<Way>> {
    let mut ways: Vec<Way> = Vec::with_capacity(ways_on_query_zoomlevel);

    for _ in 0..ways_on_query_zoomlevel {
        if debug {
            let mut sig = [0u8; 16];
            reader.read_exact(&mut sig)?;
            let way_sig = String::from_utf8_lossy(&sig).trim().to_string();

            println!("Way signature: {way_sig}");
        } else {
            println!("Skip way signature")
        }

        let mut way: Way = Default::default();
    
        let way_data_size = utils::read_vbe_u_int(reader)?;
        println!("way_data_size: {way_data_size}");

        println!("TODO: check if BigEndian!");
        let sub_tile_bitmap = reader.read_u16::<byteorder::BigEndian>()?;
        println!("sub_tile_bitmap: {sub_tile_bitmap:016b}");
        way.sub_tile_bitmap = sub_tile_bitmap;

        let special_byte = reader.read_u8()?;   
        way.layer = ((special_byte & WAY_LAYER_BITMASK) >> WAY_LAYER_SHIFT) as i8;
        let number_of_tags = special_byte & WAY_NUMBER_OF_TAGS_BITMASK;

        way.tag_ids = Vec::with_capacity(number_of_tags as usize);
        let mut tags: Vec<Tag> = Vec::with_capacity(number_of_tags as usize);
        for _ in 1..number_of_tags {
            let tag_id = utils::read_vbe_u_int(reader)?;
            if tag_id < way_tags.len() {
                let tag_str = way_tags[tag_id].clone();
                let kv: Vec<&str> = tag_str.split_terminator(tag::KEY_VALUE_SEPERATOR).collect();
                let tag = Tag::new(kv[0], kv[1]);
                way.tag_ids.push(tag_id);
                tags.push(tag);
            } else {
                panic!("tag_id > list of tags!");
            }
        }

        let flags = reader.read_u8()?;
        println!("flags: {flags:08b}");
        
        if flags & WAY_FEATURE_NAME != 0 {
            // 1. bit: flag for existence of a way name as a string.
            let name = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_NAME, &name));
            println!("name {name}");
        }
        
        if flags & WAY_FEATURE_HOUSE_NUMBER != 0 {
            // 2. bit: flag for existence of a house number as a string.
            let house_number = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_HOUSE_NUMBER, &house_number));
            println!("house number {house_number}");
        }
        
        if flags & WAY_FEATURE_REF != 0 {
            // 3. bit: flag for existence of a reference as a string
            let ref_str = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(TAG_KEY_REF, &ref_str));
            println!("reference {ref_str}");
        }

        way.tags = Some(tags);
        
        if flags & WAY_FEATURE_LABEL_POSITION != 0 {
            // 4. bit: flag for existence of a label position
            //         geo coordinate difference to the first way node in 
            //         microdegrees as 2 × VBE-S INT, in the order lat-diff,
            //         lon-diff.
            way.label_position = Some(LatLong {
                latitude: utils::read_microdegrees(reader)?,
                longitude: utils::read_microdegrees(reader)?
            });
        }
        
        // 5. bit: flag for existence of number of way data blocks field
        let number_of_way_data_blocks = if flags & WAY_FEATURE_DATA_BLOCKS_BYTE != 0 {
            //     case 1: field exists, more than one block
            println!("Read number of way data blocks from file");
            utils::read_vbe_u_int(reader)?
        } else {
            //     case 0: field does not exist, number of blocks is one
            1
        };
        println!("number of way data blocks {number_of_way_data_blocks}");
        
        // 6. bit: flag indicating encoding of way coordinate blocks
        let way_coordinate_single_delta_encoding = if flags & WAY_FEATURE_DOUBLE_DELTA_ENCODING != 0 {
            //     case 1: double delta encoding
            println!("use double delta encoding");
            false
        } else {
            //     case 0: single delta encoding
            println!("use singe delta encoding");
            true
        };
        // 7.-8. bit: reserved for future use
        
        // read way data blocks
        for i in 0..number_of_way_data_blocks {
            println!("Way data block {i}");
            let num_way_coordinates = utils::read_vbe_u_int(reader)?;
            if num_way_coordinates == 0 {
                println!("skip empty data_block");
                continue;
            }
            println!("  number of way coordinate blocks {num_way_coordinates}");
            let num_way_nodes = utils::read_vbe_u_int(reader)?;
            for _ in 0..num_way_coordinates {
                let way_coordinate_block = if way_coordinate_single_delta_encoding {
                    decode_way_nodes_single_delta(reader, num_way_nodes)?
                } else {
                    decode_way_nodes_double_delta(reader, num_way_nodes)?
                };        
                println!("TODO: way nodes coordinates as degrees from tile top/left {way_coordinate_block:?}");
            }
        }
        ways.push(way);
    }

    Ok(ways)
}

fn decode_way_nodes_double_delta<R: Read>(reader: &mut BufReader<R>, num_way_nodes: usize) -> Result<WayCoordinateBlock> {
    let mut way_coordinate_block: WayCoordinateBlock = Default::default();
    
    way_coordinate_block.coordinates.reserve(num_way_nodes);
    let mut way_node_latitude = utils::read_microdegrees(reader)?;
    let mut way_node_longitude = utils::read_microdegrees(reader)?;
    way_coordinate_block.initial_position = LatLong {
        latitude: way_node_latitude,
        longitude: way_node_longitude
    };
    let previous_single_delta_latitude = 0f64;
    let previous_single_delta_longitude = 0f64;
    for _ in 1..num_way_nodes {
        let double_delta_latitude = utils::read_microdegrees(reader)?;
        let double_delta_longitude = utils::read_microdegrees(reader)?;

        let single_delta_latitude = double_delta_latitude + previous_single_delta_latitude;
        let single_delta_longitude = double_delta_longitude + previous_single_delta_longitude;

        way_node_latitude += single_delta_latitude;
        way_node_longitude += single_delta_longitude;

        // Decoding near international date line can return values slightly outside valid [-180°, 180°] due to calculation precision
        if way_node_longitude < utils::LONGITUDE_MIN
            && (utils::LONGITUDE_MIN - way_node_longitude) < 0.001 {
        way_node_longitude = utils::LONGITUDE_MIN;
        } else if way_node_longitude > utils::LONGITUDE_MAX
                && (way_node_longitude - utils::LONGITUDE_MAX) < 0.001 {
            way_node_longitude = utils::LONGITUDE_MAX;
        }
        way_coordinate_block.coordinates.push(LatLong { 
            latitude: way_node_latitude, 
            longitude: way_node_longitude 
        });
    }

    Ok(way_coordinate_block)
}

fn decode_way_nodes_single_delta<R: Read>(reader: &mut BufReader<R>, num_way_nodes: usize) -> Result<WayCoordinateBlock> {
    let mut way_coordinate_block: WayCoordinateBlock = Default::default();

    way_coordinate_block.initial_position = LatLong {
        latitude: utils::read_microdegrees(reader)?,
        longitude: utils::read_microdegrees(reader)?
    };
    way_coordinate_block.coordinates.push(way_coordinate_block.initial_position);
    for _ in 1..num_way_nodes {
        // geo coordinates of the remaining way nodes stored as differences to the previous 
        // way node in microdegrees as 2 × VBE-S INT in the order lat-diff, lon-diff using 
        // either single or double delta encoding (see below).
        way_coordinate_block.coordinates.push(LatLong {
            latitude: utils::read_microdegrees(reader)?,
            longitude: utils::read_microdegrees(reader)?
        });
    }

    Ok(way_coordinate_block)
}
