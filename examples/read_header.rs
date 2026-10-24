use std::fs::File;
use std::io::BufReader;
use mapsforge_rs::MapHeader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("../garmin-img/tests/assets/Freizeitkarte_MADEIRA.map")?;
    let mut reader = BufReader::new(file);
    
    let header = MapHeader::read_from_file(&mut reader)?;
    println!("Header: {:#?}", header);
    
    Ok(())
}