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