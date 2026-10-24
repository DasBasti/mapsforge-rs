#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod error;
pub mod io;
pub mod math;
pub mod header;
pub mod tile;
pub mod types;
pub mod utils;
pub mod tag;
pub mod blocks;
pub mod poi;
pub mod way;
pub mod visit;
pub mod view;
pub mod style;
pub mod render;
#[cfg(feature = "ffi")]
pub mod ffi;

pub use error::MapforgeError;
pub use types::{BoundingBox, MapHeader};

pub mod prelude {
    pub use crate::error::MapforgeError;
    pub use crate::types::{BoundingBox, MapHeader};
    pub use crate::Result;
}


pub type Result<T> = core::result::Result<T, MapforgeError>;
