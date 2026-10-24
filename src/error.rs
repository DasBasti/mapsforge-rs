use core::fmt;

#[derive(Debug)]
pub enum MapforgeError {
    #[cfg(feature = "std")]
    Io(std::io::Error),
    /// The byte source failed or ended early (no_std sources).
    Source,
    InvalidMagic,
    UnsupportedVersion(u32),
    InvalidBoundingBox,
    InvalidHeaderSize(u32),
    InvalidIndexSignature,
    InvalidTagId(usize),
    InvalidTileIndex,
    /// Implausible sizes in the file, or out of memory while decoding it.
    Corrupt(&'static str),
}

impl fmt::Display for MapforgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Source => write!(f, "Byte source read or seek failed"),
            Self::InvalidMagic => write!(f, "Invalid magic bytes, expected 'mapsforge binary OSM'"),
            Self::UnsupportedVersion(v) => {
                write!(f, "Unsupported file version: {v}, expected version 3 or higher")
            }
            Self::InvalidBoundingBox => write!(f, "Invalid bounding box values"),
            Self::InvalidHeaderSize(s) => write!(f, "Invalid header size: {s}"),
            Self::InvalidIndexSignature => write!(f, "Invalid tile index signature"),
            Self::InvalidTagId(id) => {
                write!(f, "Tag id {id} is out of range of the header tag table")
            }
            Self::InvalidTileIndex => write!(f, "Requested tile is outside of the map"),
            Self::Corrupt(why) => write!(f, "Corrupt map data or out of memory: {why}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MapforgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for MapforgeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
