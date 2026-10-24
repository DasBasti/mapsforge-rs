use alloc::string::ToString;

use crate::{io::ByteRead, types::Tag, utils, Result};

pub const KEY_VALUE_SEPERATOR: char = '=';
/// Layers are stored as 0..=10 with 5 meaning layer 0.
pub const LAYER_OFFSET: i8 = 5;
pub const TAG_KEY_NAME: &str = "name";
pub const TAG_KEY_HOUSE_NUMBER: &str = "addr:housenumber";
pub const TAG_KEY_REF: &str = "ref";

impl Tag {
    pub fn new(key: &str, value: &str) -> Tag {
        Tag {
            key_code: utils::hash_tag_parameter(key),
            key: key.to_string(),
            value_code: utils::hash_tag_parameter(value),
            value: value.to_string(),
        }
    }
}

/// Splits a header tag string like `highway=bus_stop` into a [`Tag`].
/// Tags without a value (no `=`) get an empty value.
pub fn parse_tag(tag_str: &str) -> Tag {
    match tag_str.split_once(KEY_VALUE_SEPERATOR) {
        Some((key, value)) => Tag::new(key, value),
        None => Tag::new(tag_str, ""),
    }
}

/// Tags of newer map files can have a value like `%i` (`maxspeed=%i`): the real value is then
/// stored after the ids of all tags of the feature, in the order of the ids. Returns the type letter: `b` byte, `h` short,
/// `i` int, `f` float, `s` string.
pub fn inline_value_kind(tag: &str) -> Option<u8> {
    let at = tag.find("=%")?;
    tag.as_bytes().get(at + 2).copied()
}

/// Reads past the inline value of a tag, see [`inline_value_kind`].
pub fn skip_inline_value<R: ByteRead + ?Sized>(reader: &mut R, kind: u8) -> Result<()> {
    let mut scratch = [0u8; 4];
    match kind {
        b'b' => reader.read_bytes(&mut scratch[..1]),
        b'h' => reader.read_bytes(&mut scratch[..2]),
        b'i' | b'f' => reader.read_bytes(&mut scratch),
        b's' => {
            let length = utils::read_vbe_u_int(reader)?;
            // skip the bytes in chunks, no allocation
            let mut left = length;
            while left > 0 {
                let n = left.min(scratch.len());
                reader.read_bytes(&mut scratch[..n])?;
                left -= n;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_values_are_skipped() {
        assert_eq!(inline_value_kind("maxspeed=%i"), Some(b'i'));
        assert_eq!(inline_value_kind("highway=primary"), None);

        let data = [1u8, 2, 3, 4, 5, 0x03, b'a', b'b', b'c', 9];
        let mut r = &data[..];
        skip_inline_value(&mut r, b'i').unwrap();
        skip_inline_value(&mut r, b'b').unwrap();
        skip_inline_value(&mut r, b's').unwrap();
        assert_eq!(r, &[9]);
    }
}
