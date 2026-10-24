use alloc::vec::Vec;

use crate::{io::ByteRead, types::ZoomInterval, utils, Result};

pub fn process_poi_way_block() {

}

pub fn read_zoom_table<R: ByteRead + ?Sized>(reader: &mut R, zoom_interval: &ZoomInterval) -> Result<Vec<(usize, usize)>> {
    let mut zoom_table= Vec::with_capacity((zoom_interval.max_zoom_level-zoom_interval.min_zoom_level) as usize);
    for _ in zoom_interval.min_zoom_level..=zoom_interval.max_zoom_level {
        let pois = utils::read_vbe_u_int(reader)?;
        let ways = utils::read_vbe_u_int(reader)?;
        zoom_table.push((pois,ways));
    }

    Ok(zoom_table)
}