//! Viewport math: which tiles are needed for a screen centered on a GPS position and how
//! tile-relative degree offsets map to screen pixels (Web-Mercator, y pointing down).
use core::f64::consts::PI;

use crate::{math, types::LatLong};

/// Tiles of at most this many degrees of latitude are projected with a linear scale taken
/// at the tile centre. Larger (coarse zoom) tiles are projected exactly per vertex.
const LINEAR_MAX_TILE_DEGREES: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub center_lat: f64,
    pub center_lon: f64,
    pub zoom: u8,
    pub width: u16,
    pub height: u16,
    /// Screen pixel the center position is drawn at, usually the middle of the screen.
    pub anchor_x: i16,
    pub anchor_y: i16,
    /// Pixel size of a tile at its own zoom level (`MapHeader::tile_size`).
    pub tile_size: u16,
}

/// Inclusive range of tile coordinates at a sub file's base zoom level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRange {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl TileRange {
    pub fn iter(self) -> impl Iterator<Item = (i32, i32)> {
        (self.y_min..=self.y_max).flat_map(move |y| (self.x_min..=self.x_max).map(move |x| (x, y)))
    }
}

fn world_pixels(zoom: u8, tile_size: u16) -> f64 {
    tile_size as f64 * (1u64 << zoom) as f64
}

/// Global pixel x of a longitude in a world `world` pixels wide.
fn lon_to_x(lon: f64, world: f64) -> f64 {
    (lon + 180.0) / 360.0 * world
}

/// Global pixel y of a latitude (degrees).
fn lat_to_y(lat: f64, world: f64) -> f64 {
    let rad = lat * PI / 180.0;
    (1.0 - math::ln(math::tan(rad) + 1.0 / math::cos(rad)) / PI) / 2.0 * world
}

impl View {
    /// View with the position in the middle of the screen.
    pub fn centered(lat: f64, lon: f64, zoom: u8, width: u16, height: u16, tile_size: u16) -> Self {
        Self {
            center_lat: lat,
            center_lon: lon,
            zoom,
            width,
            height,
            anchor_x: (width / 2) as i16,
            anchor_y: (height / 2) as i16,
            tile_size,
        }
    }

    /// Global pixel position of the screen's top-left corner.
    fn top_left(&self) -> (f64, f64) {
        let world = world_pixels(self.zoom, self.tile_size);
        (
            lon_to_x(self.center_lon, world) - self.anchor_x as f64,
            lat_to_y(self.center_lat, world) - self.anchor_y as f64,
        )
    }

    /// Tiles of a sub file with `base_zoom` that overlap the screen.
    pub fn visible_tiles(&self, base_zoom: u8) -> TileRange {
        // a base-zoom tile spans this many screen pixels at the view zoom
        let span = self.tile_size as f64 * math::exp2(self.zoom as i32 - base_zoom as i32);
        let (left, top) = self.top_left();
        let n = (1i64 << base_zoom) - 1;
        let clamp = |v: f64| (math::floor(v) as i64).clamp(0, n) as i32;
        TileRange {
            x_min: clamp(left / span),
            x_max: clamp((left + self.width as f64 - 1.0) / span),
            y_min: clamp(top / span),
            y_max: clamp((top + self.height as f64 - 1.0) / span),
        }
    }

    /// Projection for tile `x`/`y` of a sub file with `base_zoom`.
    pub fn tile_projection(&self, x: i32, y: i32, base_zoom: u8) -> TileProjection {
        let origin_lat = tile_to_lat(y as u32, base_zoom);
        let origin_lon = tile_to_lon(x as u32, base_zoom);
        let world = world_pixels(self.zoom, self.tile_size);
        let (left, top) = self.top_left();

        let origin_x = lon_to_x(origin_lon, world) - left;
        let origin_y = lat_to_y(origin_lat, world) - top;

        let scale_x = world / 360.0;
        let next_lat = tile_to_lat(y as u32 + 1, base_zoom);
        let exact = origin_lat - next_lat > LINEAR_MAX_TILE_DEGREES;
        let centre_lat = (origin_lat + next_lat) / 2.0;
        let scale_y = scale_x / math::cos(centre_lat * PI / 180.0);

        TileProjection { origin_lat, origin_x, origin_y, scale_x, scale_y, world, top, exact }
    }
}

fn tile_to_lon(x: u32, zoom: u8) -> f64 {
    x as f64 / (1u64 << zoom) as f64 * 360.0 - 180.0
}

fn tile_to_lat(y: u32, zoom: u8) -> f64 {
    let n = PI - 2.0 * PI * y as f64 / (1u64 << zoom) as f64;
    180.0 / PI * math::atan(0.5 * (math::exp(n) - math::exp(-n)))
}

/// Maps the degree offsets stored in a tile (relative to its top-left corner) to screen pixels.
#[derive(Debug, Clone, Copy)]
pub struct TileProjection {
    origin_lat: f64,
    /// screen position of the tile's top-left corner
    pub origin_x: f64,
    pub origin_y: f64,
    scale_x: f64,
    scale_y: f64,
    world: f64,
    top: f64,
    exact: bool,
}

impl TileProjection {
    /// Screen position (pixels, y down) of a tile-relative offset in degrees.
    pub fn project(&self, offset: LatLong) -> (f32, f32) {
        let x = self.origin_x + offset.longitude * self.scale_x;
        let y = if self.exact {
            lat_to_y(self.origin_lat + offset.latitude, self.world) - self.top
        } else {
            self.origin_y - offset.latitude * self.scale_y
        };
        (x as f32, y as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(zoom: u8) -> View {
        View::centered(32.6669, -16.9241, zoom, 480, 320, 256)
    }

    /// The tile containing the view centre must project the centre to the screen middle.
    fn check_centre(zoom: u8, base_zoom: u8) {
        let v = view(zoom);
        let n = (1u64 << base_zoom) as f64;
        let rad = v.center_lat * PI / 180.0;
        let x = math::floor((v.center_lon + 180.0) / 360.0 * n) as i32;
        let y = math::floor((1.0 - math::ln(math::tan(rad) + 1.0 / math::cos(rad)) / PI) / 2.0 * n) as i32;

        let p = v.tile_projection(x, y, base_zoom);
        let offset = LatLong {
            latitude: v.center_lat - tile_to_lat(y as u32, base_zoom),
            longitude: v.center_lon - tile_to_lon(x as u32, base_zoom),
        };
        let (px, py) = p.project(offset);
        assert!((px - 240.0).abs() < 0.5 && (py - 160.0).abs() < 0.5, "z{zoom}/b{base_zoom}: {px},{py}");

        let tiles = v.visible_tiles(base_zoom);
        assert!(tiles.iter().any(|t| t == (x, y)));
    }

    #[test]
    fn centre_maps_to_screen_middle() {
        check_centre(14, 14);
        check_centre(16, 14);
        check_centre(12, 14);
        check_centre(10, 5); // coarse sub file, exact projection
    }

    #[test]
    fn visible_tiles_cover_screen() {
        let r = view(14).visible_tiles(14);
        // 480x320 px at tile size 256 touches at most 3x3 tiles (2 apart)
        assert!(r.x_max - r.x_min <= 2 && r.y_max - r.y_min <= 2);
        assert!(r.iter().count() >= 1);
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::*;

    #[test]
    fn anchor_moves_position_on_screen() {
        let mut v = View::centered(32.6669, -16.9241, 16, 600, 448, 256);
        v.anchor_x = 100;
        v.anchor_y = 400;
        let n = (1u64 << 16) as f64;
        let rad = v.center_lat * PI / 180.0;
        let x = math::floor((v.center_lon + 180.0) / 360.0 * n) as i32;
        let y = math::floor((1.0 - math::ln(math::tan(rad) + 1.0 / math::cos(rad)) / PI) / 2.0 * n) as i32;
        let p = v.tile_projection(x, y, 16);
        let (px, py) = p.project(LatLong {
            latitude: v.center_lat - tile_to_lat(y as u32, 16),
            longitude: v.center_lon - tile_to_lon(x as u32, 16),
        });
        assert!((px - 100.0).abs() < 0.5 && (py - 400.0).abs() < 0.5, "{px},{py}");
    }
}
