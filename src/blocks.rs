use std::io::{BufReader, Read};

use crate::{types::ZoomInterval, utils, Result};

pub fn process_poi_way_block() {

}

pub fn read_zoom_table<R: Read>(reader: &mut BufReader<R>, zoom_interval: &ZoomInterval) -> Result<Vec<(usize, usize)>> {
    let mut zoom_table= Vec::with_capacity((zoom_interval.max_zoom_level-zoom_interval.min_zoom_level) as usize);
    for zl in zoom_interval.min_zoom_level..zoom_interval.max_zoom_level + 1 {
        let pois = utils::read_vbe_u_int(reader)?;
        let ways = utils::read_vbe_u_int(reader)?;
        println!("zoomlevell {zl} has {pois} pois and {ways} ways");
        zoom_table.push((pois,ways));
    }

    Ok(zoom_table)
}