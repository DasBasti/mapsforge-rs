#[derive(Debug)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
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
    pub flags: u8
}
