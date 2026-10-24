use crate::{types::Tag, utils};

pub const KEY_VALUE_SEPERATOR: char = '=';
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
