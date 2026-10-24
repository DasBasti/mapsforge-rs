//! Renders a [`View`] of a map to simple drawing primitives. All visible tiles are decoded,
//! projected to screen pixels, culled and sorted into drawing order; the result is then
//! emitted to a [`DrawTarget`], which can be a C framebuffer rasterizer.
use alloc::vec::Vec;

use crate::{
    io::ByteSource,
    style::{Kind, Style, StyleTable},
    types::{LatLong, MapFile},
    view::{TileProjection, View},
    visit::{PoiInfo, TileVisitor, WayInfo},
    Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Point {
    pub x: i16,
    pub y: i16,
}

/// Receives drawing primitives in painting order (back to front).
/// Flag in the color of `fill_polygon`: fill with a 50% checkerboard of the color and the
/// background (white) instead of solid.
pub const FILL_CHECKER: u32 = 1 << 24;

pub trait DrawTarget {
    /// Fills the given rings with the even-odd rule (inner rings are holes).
    fn fill_polygon(&mut self, rings: &[&[Point]], color: u32);
    fn polyline(&mut self, points: &[Point], color: u32, width: u8);
    fn point(&mut self, at: Point, color: u32, radius: u8);
}

#[derive(Clone, Copy)]
enum Shape {
    Polygon,
    Line,
    Point,
}

/// One primitive; its points are `points[start..start + len]`, rings of polygons are
/// described by `rings[ring_start..ring_start + ring_count]`.
struct Command {
    sort_key: u32,
    shape: Shape,
    style: Style,
    start: u32,
    len: u32,
    ring_start: u32,
    ring_count: u32,
}

#[derive(Default)]
struct Scene {
    commands: Vec<Command>,
    points: Vec<Point>,
    /// lengths of the rings of polygon commands
    rings: Vec<u32>,
}

struct Screen {
    width: i32,
    height: i32,
}

struct TileCollector<'a> {
    scene: &'a mut Scene,
    styles: &'a StyleTable,
    projection: TileProjection,
    screen: &'a Screen,
    /// 4x4 sub tile cells (bit 15 = top-left) that touch the screen
    visible_cells: u16,
}

fn to_point(p: (f32, f32)) -> Point {
    Point { x: p.0.clamp(-30000.0, 30000.0) as i16, y: p.1.clamp(-30000.0, 30000.0) as i16 }
}

/// Rings are closed when the last node equals the first (up to accumulated rounding error).
fn is_closed(block: &[LatLong]) -> bool {
    match (block.first(), block.last()) {
        (Some(a), Some(b)) if block.len() > 3 => {
            let d_lat = a.latitude - b.latitude;
            let d_lon = a.longitude - b.longitude;
            d_lat > -1e-7 && d_lat < 1e-7 && d_lon > -1e-7 && d_lon < 1e-7
        }
        _ => false,
    }
}

impl TileCollector<'_> {
    /// Projects a ring, dropping points that are less than a pixel from their predecessor.
    /// Returns `(len, touches_screen)`.
    fn push_ring(&mut self, coords: &[LatLong]) -> (u32, bool) {
        let first = self.scene.points.len();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for c in coords {
            let p = to_point(self.projection.project(*c));
            if let Some(last) = self.scene.points[first..].last() {
                if *last == p {
                    continue;
                }
            }
            min_x = min_x.min(p.x as i32);
            max_x = max_x.max(p.x as i32);
            min_y = min_y.min(p.y as i32);
            max_y = max_y.max(p.y as i32);
            self.scene.points.push(p);
        }
        let visible = max_x >= 0 && max_y >= 0 && min_x < self.screen.width && min_y < self.screen.height;
        ((self.scene.points.len() - first) as u32, visible)
    }

    fn sort_key(layer: i8, style: &Style) -> u32 {
        ((layer.clamp(-5, 5) + 5) as u32) << 8 | style.order as u32
    }
}

impl TileVisitor for TileCollector<'_> {
    fn accept_way(&mut self, sub_tile_bitmap: u16) -> bool {
        sub_tile_bitmap == 0 || sub_tile_bitmap & self.visible_cells != 0
    }

    fn poi(&mut self, poi: &PoiInfo) {
        let Some(style) = self.styles.poi_style(poi.tag_ids) else { return };
        let at = to_point(self.projection.project(poi.position));
        if at.x < 0 || at.y < 0 || at.x as i32 >= self.screen.width || at.y as i32 >= self.screen.height {
            return;
        }
        let start = self.scene.points.len() as u32;
        self.scene.points.push(at);
        self.scene.commands.push(Command {
            sort_key: Self::sort_key(poi.layer, &style),
            shape: Shape::Point,
            style,
            start,
            len: 1,
            ring_start: 0,
            ring_count: 0,
        });
    }

    fn way(&mut self, way: &WayInfo) {
        let Some(style) = self.styles.way_style(way.tag_ids) else { return };
        if style.kind == Kind::Poi {
            return;
        }
        let start = self.scene.points.len();
        let ring_start = self.scene.rings.len();
        let mut visible = false;
        let mut closed = false;

        for (i, block) in way.blocks.iter().enumerate() {
            let (len, touches) = self.push_ring(block);
            if i == 0 {
                visible = touches;
                closed = is_closed(block);
            }
            if len < 2 {
                // degenerate ring after pixel snapping
                self.scene.points.truncate(self.scene.points.len() - len as usize);
                continue;
            }
            self.scene.rings.push(len);
        }

        let ring_count = self.scene.rings.len() - ring_start;
        if !visible || ring_count == 0 {
            self.scene.points.truncate(start);
            self.scene.rings.truncate(ring_start);
            return;
        }

        let shape = if style.kind == Kind::Area && closed { Shape::Polygon } else { Shape::Line };
        // lines only use their first block
        let len = match shape {
            Shape::Line => self.scene.rings[ring_start] as usize,
            _ => self.scene.points.len() - start,
        };
        self.scene.commands.push(Command {
            sort_key: Self::sort_key(way.layer, &style),
            shape,
            style,
            start: start as u32,
            len: len as u32,
            ring_start: ring_start as u32,
            ring_count: ring_count as u32,
        });
    }
}

impl<S: ByteSource> MapFile<S> {
    /// Decodes all tiles overlapping `view`, and draws them back to front into `target`.
    /// Returns the number of tiles that had to be read.
    pub fn render<D: DrawTarget>(&mut self, view: &View, styles: &StyleTable, target: &mut D) -> Result<usize> {
        let Some(sub_file) = self.sub_file_for_zoom(view.zoom) else { return Ok(0) };
        let base_zoom = self.header.zoom_interval_configuration[sub_file].base_zoom_level;
        let screen = Screen { width: view.width as i32, height: view.height as i32 };

        let mut scene = Scene::default();
        let mut tiles = 0;
        for (x, y) in view.visible_tiles(base_zoom).iter() {
            let Some(index) = self.tile_index_of(sub_file, x, y) else { continue };
            let projection = view.tile_projection(x, y, base_zoom);

            let mut collector = TileCollector {
                scene: &mut scene,
                styles,
                projection,
                screen: &screen,
                visible_cells: visible_cells(&projection, view, base_zoom),
            };
            let meta = self.visit_tile(sub_file, index, view.zoom, &mut collector)?;
            if meta.is_water {
                add_water_tile(&mut scene, &projection, view, base_zoom, styles);
            }
            tiles += 1;
        }

        emit(&mut scene, target);
        Ok(tiles)
    }
}

/// Water tiles carry no sea polygon of their own: the whole tile is water.
fn add_water_tile(scene: &mut Scene, projection: &TileProjection, view: &View, base_zoom: u8, styles: &StyleTable) {
    let span = view.tile_size as f64 * crate::math::exp2(view.zoom as i32 - base_zoom as i32);
    let (x0, y0) = (projection.origin_x, projection.origin_y);
    let corner = |x: f64, y: f64| to_point((x as f32, y as f32));
    let start = scene.points.len() as u32;
    scene.points.extend([corner(x0, y0), corner(x0 + span, y0), corner(x0 + span, y0 + span), corner(x0, y0 + span)]);
    scene.rings.push(4);
    scene.commands.push(Command {
        sort_key: 0,
        shape: Shape::Polygon,
        style: Style { kind: Kind::Area, color: styles.water_color(), width: 1, order: 0, checker: false },
        start,
        len: 4,
        ring_start: scene.rings.len() as u32 - 1,
        ring_count: 1,
    });
}

/// Bitmask of the tile's 4x4 sub tile cells that overlap the screen.
fn visible_cells(projection: &TileProjection, view: &View, base_zoom: u8) -> u16 {
    let span = view.tile_size as f64 * crate::math::exp2(view.zoom as i32 - base_zoom as i32);
    let cell = span / 4.0;
    let mut mask = 0u16;
    for row in 0..4 {
        for col in 0..4 {
            let left = projection.origin_x + col as f64 * cell;
            let top = projection.origin_y + row as f64 * cell;
            if left < view.width as f64 && left + cell > 0.0 && top < view.height as f64 && top + cell > 0.0 {
                mask |= 0x8000 >> (row * 4 + col);
            }
        }
    }
    mask
}

fn emit<D: DrawTarget>(scene: &mut Scene, target: &mut D) {
    // stable sort keeps file order inside the same layer and style order
    scene.commands.sort_by_key(|c| c.sort_key);

    let mut rings: Vec<&[Point]> = Vec::new();
    for c in &scene.commands {
        let points = &scene.points[c.start as usize..(c.start + c.len) as usize];
        match c.shape {
            Shape::Point => target.point(points[0], c.style.color, c.style.width),
            Shape::Line => target.polyline(points, c.style.color, c.style.width),
            Shape::Polygon => {
                let color = if c.style.checker { c.style.color | FILL_CHECKER } else { c.style.color };
                rings.clear();
                let mut offset = c.start as usize;
                for &len in &scene.rings[c.ring_start as usize..(c.ring_start + c.ring_count) as usize] {
                    rings.push(&scene.points[offset..offset + len as usize]);
                    offset += len as usize;
                }
                target.fill_polygon(&rings, color);
            }
        }
    }
}
