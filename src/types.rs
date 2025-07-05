use std::{fs::File, io::BufReader};

#[derive(Debug,Clone, Copy)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

#[derive(Debug)]
pub struct ZoomInterval {
    pub base_zoom_level: u8,
    pub min_zoom_level: u8,
    pub max_zoom_level: u8,
    pub sub_file_start: u64,
    pub sub_file_size: u64
   
}

#[derive(Debug)]
pub struct MapHeader {
    pub magic: String,
    pub header_size: u32,
    pub file_version: u32,
    pub file_size: u64,
    pub creation_date: u64,
    pub bounding_box: BoundingBox,
    pub tile_size: u16,
    pub projection: String,
    pub flags: u8,

    // optional fields
    pub map_start_position: Option<LatLong>,
    pub start_zoom_level: Option<u8>,
    pub language_preference: Option<String>,
    pub comment: Option<String>,
    pub created_by: Option<String>,


    // tag info
    pub poi_tags: Vec<String>,
    pub way_tags: Vec<String>,
    
    // // zoom interval
    pub num_zoom_intervals: u8,
    pub zoom_interval_configuration: Vec<ZoomInterval>
   
}




#[derive(Debug)]
pub struct TileIndexHeader {
    pub debug_signature: Option<String>,
}

#[derive(Debug)]
pub struct TileIndexEntry {
    pub is_water: bool,   
    pub offset: u64,
    pub offset_abs: u64,
}

#[derive(Debug)]
pub struct MapFile {
    pub header: MapHeader,
    pub reader: BufReader<File>,  
    pub tile_indices: Vec<Vec<TileIndexEntry>>,
}

#[derive(Debug)]
pub struct Tile {
 
    pub debug_signature: Option<String>,
    

    pub zoom_table: Vec<(u32, u32)>, 
    pub first_way_offset: u32,
    
  
    pub pois: Vec<POI>,
    pub ways: Vec<Way>
}

#[derive(Debug, Default)]
pub struct POI {

    pub debug_signature: Option<String>,
    
    pub position_offset: LatLong,
    pub layer: i8,
    pub tag_ids: Vec<usize>,
    pub tags: Option<Vec<Tag>>,
    pub name: Option<String>,
    pub house_number: Option<String>, 
    pub elevation: Option<i32>
}
#[derive(Debug, Default)]
pub struct Way {

    pub debug_signature: Option<String>,
    
    pub sub_tile_bitmap: u16, 
    pub layer: i8, 
    pub tag_ids: Vec<usize>,
    pub tags: Option<Vec<Tag>>,
    pub name: Option<String>,
    pub house_number: Option<String>,
    pub reference: Option<String>,
    pub label_position: Option<LatLong>,  
    

    pub coordinate_blocks: Vec<WayCoordinateBlock>,
    

    pub double_delta_encoding: bool
}
#[derive(Debug, Default)]
pub struct WayCoordinateBlock {
    
    pub initial_position: LatLong, 
   
    pub coordinates: Vec<LatLong>
}

#[derive(Debug)]
pub struct TagMapping {
    pub poi_tags: Vec<String>,
    pub way_tags: Vec<String>
}

#[derive(Debug)]
pub struct Tag {
    pub key: String,
    pub key_code: usize,
    pub value: String,
    pub value_code: usize,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct LatLong {
    pub latitude: f64,
    pub longitude: f64,
}