use std::io::{BufReader, Read};

use byteorder::ReadBytesExt;

use crate::{tag, types::{LatLong, Tag, POI}, utils, Result};

const POI_LAYER_BITMASK: u8 = 0xf0;
const POI_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const POI_LAYER_SHIFT: u8 = 4;

pub fn process_pois<R: Read>(reader: &mut BufReader<R>, pois_on_query_zoomlevel: usize, poi_tags: &Vec<String>, debug: bool) -> Result<Vec<POI>> {
    let mut pois: Vec<POI> = Vec::with_capacity(pois_on_query_zoomlevel);

    for _ in 0..pois_on_query_zoomlevel {
        if debug {
            let mut sig = [0u8; 16];
            reader.read_exact(&mut sig)?;
            let poi_sig = String::from_utf8_lossy(&sig).trim().to_string();

            println!("POI signature: {poi_sig}");
        } else {
            println!("Skip POI signature")
        }

        let mut poi: POI = Default::default();
            
        poi.position_offset = LatLong {
            latitude: utils::read_microdegrees(reader)?,
            longitude: utils::read_microdegrees(reader)?
        };
        
        let special_byte = reader.read_u8()?;   
        poi.layer = ((special_byte & POI_LAYER_BITMASK) >> POI_LAYER_SHIFT) as i8;
        let number_of_tags = special_byte & POI_NUMBER_OF_TAGS_BITMASK;

        poi.tag_ids = Vec::with_capacity(number_of_tags as usize);
        let mut tags: Vec<Tag> = Vec::with_capacity(number_of_tags as usize);
        for _ in 1..number_of_tags {
            let tag_id = utils::read_vbe_u_int(reader)?;
            if tag_id < poi_tags.len() {
                let tag_str = poi_tags[tag_id].clone();
                let kv: Vec<&str> = tag_str.split_terminator(tag::KEY_VALUE_SEPERATOR).collect();
                let tag = Tag::new(kv[0], kv[1]);
                poi.tag_ids.push(tag_id);
                tags.push(tag);
            } else {
                panic!("tag_id > list of tags!");
            }
        }
        poi.tags = Some(tags);
        pois.push(poi);
    }

    Ok(pois)
}