pub mod error;
pub mod header;
pub mod tile;
pub mod types;
pub mod utils;
pub mod tag;
pub mod blocks;
pub mod poi;
pub mod way;

pub use error::MapforgeError;
pub use types::{BoundingBox, MapHeader};

pub mod prelude {
    pub use crate::error::MapforgeError;
    pub use crate::types::{BoundingBox, MapHeader};
    pub use crate::Result;
}


pub type Result<T> = std::result::Result<T, MapforgeError>;
