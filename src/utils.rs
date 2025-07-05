use byteorder::{ReadBytesExt};

use crate::{
    Result,
};
use std::io::{BufReader, Read};

pub const LONGITUDE_MAX: f64 = 180f64;
pub const LONGITUDE_MIN: f64 = -LONGITUDE_MAX;

pub const LATITUDE_MAX: f64 = 90f64;
pub const LATITUDE_MIN: f64 = -LATITUDE_MAX;


pub fn read_vbe_u<R: Read>(reader: &mut BufReader<R>) -> Result<String> {
    let mut length = 0u32;

    let mut shift = 0;

    loop {
        let byte = reader.read_u8()?;

        length |= ((byte & 0x7F) as u32) << shift;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7
    }

    let mut string_bytes = vec![0u8; length as usize];

    reader.read_exact(&mut string_bytes)?;

    Ok(String::from_utf8(string_bytes).expect("Error parsing vbe_u"))
}

pub fn read_vbe_u_int<R: Read>(reader: &mut BufReader<R>) -> Result<usize> {
    let mut length = 0usize;

    let mut shift = 0;

    let mut byte: u8;
    loop {
        byte = reader.read_u8()?;

        if byte & 0x80 == 0 {
            break;
        }
        
        length |= ((byte & 0x7F) as usize) << shift;

        shift += 7;
    }

    Ok(length | ((byte as usize) << shift))
}

pub fn read_vbe_s_int<R: Read>(reader: &mut BufReader<R>) -> Result<isize> {
    let mut length = 0usize;

    let mut shift = 0;

    let mut byte: u8;
    loop {
        byte = reader.read_u8()?;

        if byte & 0x80 == 0 {
            // read the six data bits from the last byte
            if byte & 0x40 != 0 {
                // negative number
                length |= length | ((byte & 0x3F) as usize) << shift;
                return Ok(-(length as isize));
            } else {
                length |= ((byte & 0x7F) as usize) << shift;
                return Ok(length as isize);
            }
        }

        length |= ((byte & 0x7F) as usize) << shift;
        
        shift += 7
    }
}

pub fn read_microdegrees<R: Read>(reader: &mut BufReader<R>) -> Result<f64> {
    let microdegrees = read_vbe_s_int(reader)?;
    
    Ok(microdegrees_to_degrees(microdegrees))
}

pub fn microdegrees_to_degrees(microdegrees: isize) ->  f64 {
    microdegrees as f64 / 1_000_000.0
}

pub fn decode_poi_tag(tag_id: usize) {

}

pub fn hash_tag_parameter(key_value: &str) -> usize {
    let n = key_value.len();
    let mut hash: usize = 0;
    for i in 0..n {
        hash = hash.wrapping_add(key_value.chars().nth(i).unwrap() as usize).wrapping_mul(31_usize.wrapping_pow((n - i + 1) as u32));
    }
    hash
}