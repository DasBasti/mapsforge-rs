use std::io::{BufReader, Read};
use byteorder::{BigEndian, ReadBytesExt};
use crate::{
    types::{BoundingBox, MapHeader},
    error::MapforgeError,
    Result,
};

// masgic bytes that identify a valid Mapsforge binary map file
const MAGIC_BYTES: &str = "mapsforge binary OSM";


const MIN_SUPPORTED_VERSION: u32 = 3;



impl BoundingBox {
  
    // reads a bounding box from a binary buffer
    // using generic so that it can work with any type that implements Read
    pub fn read_from_buffer<R: Read>(reader: &mut BufReader<R>) -> Result<Self> {

        // read and convert coordinates from microdegrees to degrees
        // divide by 1_000_000 to get degrees as all latitude and longitude coordinates are stored in microdegrees (degrees × 10^6)
        let min_lat = reader.read_i32::<BigEndian>()? as f64 / 1_000_000.0;
        let min_lon = reader.read_i32::<BigEndian>()? as f64 / 1_000_000.0;
        let max_lat = reader.read_i32::<BigEndian>()? as f64 / 1_000_000.0; 
        let max_lon = reader.read_i32::<BigEndian>()? as f64 / 1_000_000.0;

        let bbox = BoundingBox {
            min_lat,
            min_lon,
            max_lat,
            max_lon,
        };

        if !bbox.is_valid() {
            return Err(MapforgeError::InvalidBoundingBox);
        }

        Ok(bbox)
    }


    // validate if bounding box coordinates are within valid ranges:
    // lat must be between -90 and 90 degree
    // log must be between -180 and 180 degree
    // minimum values must be less then or equal to maximum values
    fn is_valid(&self) -> bool {
        self.min_lat >= -90.0 && self.min_lat <= 90.0 &&
        self.max_lat >= -90.0 && self.max_lat <= 90.0 &&
        self.min_lon >= -180.0 && self.min_lon <= 180.0 &&
        self.max_lon >= -180.0 && self.max_lon <= 180.0 &&
        self.min_lat <= self.max_lat &&
        self.min_lon <= self.max_lon
    }
}



impl MapHeader {

    // reads a map header from binary buffer
    pub fn read_from_file<R: Read>(reader: &mut BufReader<R>) -> Result<Self> {

        let mut magic_buf= [0u8;20];
        reader.read_exact(&mut magic_buf)?;
        let magic = String::from_utf8_lossy(&magic_buf).trim().to_string();

        if magic != MAGIC_BYTES {
            return Err(MapforgeError::InvalidMagic);
        }
        
        let header_size = reader.read_u32::<BigEndian>()?;
        let file_version = reader.read_u32::<BigEndian>()?;

        if file_version < MIN_SUPPORTED_VERSION {
            return Err(MapforgeError::UnsupportedVersion(file_version));
        }
        
        let file_size = reader.read_u64::<BigEndian>()?;
        let creation_date = reader.read_u64::<BigEndian>()?;

        let bounding_box = BoundingBox::read_from_buffer(reader)?;

        let tile_size = reader.read_u16::<BigEndian>()?;
       
        let projection = String::from("MERCATOR"); 

        let flags = reader.read_u8()?;


        let header = MapHeader {
            magic,
            header_size,
            file_version,
            file_size,
            creation_date,
            bounding_box,
            tile_size,
            projection,
            flags,
        };

        if !header.is_valid() {
            return Err(MapforgeError::InvalidHeaderSize(header_size));
        }

        Ok(header)

    }

    pub fn is_valid(&self) -> bool {
        self.magic.trim() == MAGIC_BYTES &&
        self.header_size > 0 &&
        self.file_version >= MIN_SUPPORTED_VERSION
    }

}