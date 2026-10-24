use alloc::{string::{String, ToString}, vec::Vec};

use crate::{error::MapforgeError, io::ByteRead, tag, types::{LatLong, Tag, POI}, utils, Result};

const POI_LAYER_BITMASK: u8 = 0xf0;
const POI_NUMBER_OF_TAGS_BITMASK: u8 = 0x0f;
const POI_LAYER_SHIFT: u8 = 4;
const POI_FEATURE_NAME: u8 = 0x80;
const POI_FEATURE_HOUSE_NUMBER: u8 = 0x40;
const POI_FEATURE_ELEVATION: u8 = 0x20;

pub fn process_pois<R: ByteRead + ?Sized>(reader: &mut R, pois_on_query_zoomlevel: usize, poi_tags: &[String], debug: bool) -> Result<Vec<POI>> {
    let mut pois: Vec<POI> = Vec::with_capacity(pois_on_query_zoomlevel);

    for _ in 0..pois_on_query_zoomlevel {
        let mut poi_signature = None;
        if debug {
            let mut sig = [0u8; 32];
            reader.read_bytes(&mut sig)?;
            poi_signature = Some(String::from_utf8_lossy(&sig).trim().to_string());
        }

        let mut poi: POI = POI { debug_signature: poi_signature, ..Default::default() };
            
        poi.position_offset = LatLong {
            latitude: utils::read_microdegrees(reader)?,
            longitude: utils::read_microdegrees(reader)?
        };
        
        let special_byte = reader.read_u8()?;   
        poi.layer = ((special_byte & POI_LAYER_BITMASK) >> POI_LAYER_SHIFT) as i8 - tag::LAYER_OFFSET;
        let number_of_tags = special_byte & POI_NUMBER_OF_TAGS_BITMASK;

        poi.tag_ids = Vec::with_capacity(number_of_tags as usize);
        let mut tags: Vec<Tag> = Vec::with_capacity(number_of_tags as usize);
        for _ in 0..number_of_tags {
            let tag_id = utils::read_vbe_u_int(reader)?;
            let tag_str = poi_tags.get(tag_id).ok_or(MapforgeError::InvalidTagId(tag_id))?;
            poi.tag_ids.push(tag_id);
            tags.push(tag::parse_tag(tag_str));
        }

        let flags = reader.read_u8()?;
        if flags & POI_FEATURE_NAME != 0 {
            let name = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(tag::TAG_KEY_NAME, &name));
            poi.name = Some(name);
        }
        if flags & POI_FEATURE_HOUSE_NUMBER != 0 {
            let house_number = utils::read_vbe_u(reader)?;
            tags.push(Tag::new(tag::TAG_KEY_HOUSE_NUMBER, &house_number));
            poi.house_number = Some(house_number);
        }
        if flags & POI_FEATURE_ELEVATION != 0 {
            poi.elevation = Some(utils::read_vbe_s_int(reader)? as i32);
        }
        poi.tags = Some(tags);
        pois.push(poi);
    }

    Ok(pois)
}