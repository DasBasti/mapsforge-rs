#[derive(Debug)]
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
    pub map_start_position: Option<(f64,f64)>,
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
