//! C API, see `include/mapsforge.h`.
use alloc::{boxed::Box, vec::Vec};
use core::ffi::{c_int, c_void};

use crate::{
    error::MapforgeError,
    io::{ByteRead, ByteSource},
    render::{DrawTarget, Point, Scene},
    style::{paint, Icon, StyleTable},
    stylesheet::Stylesheet,
    types::MapFile,
    view::View,
    Result,
};

pub const MFR_OK: c_int = 0;
pub const MFR_ERR_IO: c_int = -1;
pub const MFR_ERR_FORMAT: c_int = -2;
pub const MFR_ERR_ARGUMENT: c_int = -3;
pub const MFR_ERR_TAG: c_int = -4;
pub const MFR_ERR_TILE: c_int = -5;
pub const MFR_ERR_CORRUPT: c_int = -6;
pub const MFR_ERR_STYLE: c_int = -7;

/// Colors are passed as 0xRRGGBB.
pub const MFR_PALETTE_RGB: u8 = 0;
/// Colors are passed as index of the 7-color e-paper: black, white, green, blue, red,
/// yellow, orange (the firmware's `color_t`), with dither patterns in the high bits (see
/// `MFR_PAT_*` in `include/mapsforge.h`).
pub const MFR_PALETTE_EINK7: u8 = 1;

static mut LAST_ERROR: [u8; 96] = [0; 96];

/// Remembers a short description of the last error for `mfr_last_error`.
fn remember_text(text: core::fmt::Arguments) {
    use core::fmt::Write;
    struct Buf<'a>(&'a mut [u8], usize);
    impl Write for Buf<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.1 + 1 < self.0.len() {
                    self.0[self.1] = b;
                    self.1 += 1;
                }
            }
            Ok(())
        }
    }
    // single threaded use, the C side renders from one task
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(LAST_ERROR) };
    buf.fill(0);
    let _ = Buf(buf, 0).write_fmt(text);
}

fn remember(e: &MapforgeError) {
    match e {
        MapforgeError::Corrupt(why) => remember_text(format_args!("corrupt: {why}")),
        MapforgeError::InvalidTagId(id) => remember_text(format_args!("tag id {id}")),
        other => remember_text(format_args!("{other:?}")),
    }
}

/// Description of the last error as a zero terminated string, for logs.
#[no_mangle]
pub extern "C" fn mfr_last_error() -> *const core::ffi::c_char {
    core::ptr::addr_of!(LAST_ERROR) as *const core::ffi::c_char
}

fn error_code(e: &MapforgeError) -> c_int {
    remember(e);
    match e {
        MapforgeError::Source => MFR_ERR_IO,
        MapforgeError::Corrupt(_) => MFR_ERR_CORRUPT,
        MapforgeError::InvalidTagId(_) => MFR_ERR_TAG,
        MapforgeError::InvalidTileIndex => MFR_ERR_TILE,
        #[cfg(feature = "std")]
        MapforgeError::Io(_) => MFR_ERR_IO,
        _ => MFR_ERR_FORMAT,
    }
}

/// File access provided by the C side. `read` returns the number of bytes read (0 = end
/// of file) or a negative value on error; `seek` returns 0 on success.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MfrIo {
    pub ctx: *mut c_void,
    pub read: extern "C" fn(ctx: *mut c_void, buf: *mut u8, len: usize) -> isize,
    pub seek: extern "C" fn(ctx: *mut c_void, offset: u64) -> c_int,
}

const BUFFER_SIZE: usize = 4096;

/// [`ByteSource`] over C callbacks with a read buffer, so the many small reads of the
/// parser don't each cross the FFI boundary.
struct CSource {
    io: MfrIo,
    buf: Vec<u8>,
    /// file offset of `buf[0]`
    buf_start: u64,
    cursor: u64,
}

impl CSource {
    fn new(io: MfrIo) -> Self {
        Self { io, buf: Vec::with_capacity(BUFFER_SIZE), buf_start: 0, cursor: 0 }
    }

    fn raw_read(&mut self, at: u64, out: *mut u8, len: usize) -> Result<usize> {
        if (self.io.seek)(self.io.ctx, at) != 0 {
            return Err(MapforgeError::Source);
        }
        let n = (self.io.read)(self.io.ctx, out, len);
        if n <= 0 {
            return Err(MapforgeError::Source);
        }
        Ok(n as usize)
    }

    fn refill(&mut self) -> Result<()> {
        self.buf.clear();
        self.buf.resize(BUFFER_SIZE, 0);
        let ptr = self.buf.as_mut_ptr();
        let n = match self.raw_read(self.cursor, ptr, BUFFER_SIZE) {
            Ok(n) => n,
            Err(e) => {
                self.buf.clear();
                return Err(e);
            }
        };
        self.buf.truncate(n);
        self.buf_start = self.cursor;
        Ok(())
    }
}

impl ByteRead for CSource {
    fn read_bytes(&mut self, out: &mut [u8]) -> Result<()> {
        let mut done = 0;
        while done < out.len() {
            let in_buf = self.cursor >= self.buf_start && self.cursor < self.buf_start + self.buf.len() as u64;
            if !in_buf {
                if out.len() - done >= BUFFER_SIZE {
                    // large read, bypass the buffer
                    let n = self.raw_read(self.cursor, out[done..].as_mut_ptr(), out.len() - done)?;
                    done += n;
                    self.cursor += n as u64;
                    continue;
                }
                self.refill()?;
            }
            let offset = (self.cursor - self.buf_start) as usize;
            let n = (self.buf.len() - offset).min(out.len() - done);
            out[done..done + n].copy_from_slice(&self.buf[offset..offset + n]);
            done += n;
            self.cursor += n as u64;
        }
        Ok(())
    }
}

impl ByteSource for CSource {
    fn seek_to(&mut self, pos: u64) -> Result<()> {
        self.cursor = pos;
        Ok(())
    }

    fn position(&mut self) -> Result<u64> {
        Ok(self.cursor)
    }
}

/// Drawing callbacks implemented by the C side. Colors depend on the palette given to
/// `mfr_render`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MfrDraw {
    pub ctx: *mut c_void,
    /// `points` holds all rings back to back, `ring_lengths[i]` is the number of points of
    /// ring `i`. Fill with the even-odd rule so inner rings become holes.
    pub fill_polygon: extern "C" fn(
        ctx: *mut c_void,
        points: *const Point,
        ring_lengths: *const u32,
        ring_count: u32,
        color: u32,
    ),
    pub polyline: extern "C" fn(ctx: *mut c_void, points: *const Point, count: u32, color: u32, width: u8),
    pub point: extern "C" fn(ctx: *mut c_void, at: Point, color: u32, radius: u8),
    /// optional: names of places, UTF-8 (not zero terminated)
    pub label: Option<extern "C" fn(ctx: *mut c_void, at: Point, text: *const u8, len: u32, rank: u8)>,
    /// optional: draws an icon (`MFR_ICON_*`) centered on a point; without it the symbols are drawn
    pub icon: Option<extern "C" fn(ctx: *mut c_void, at: Point, icon: u8)>,
}

struct CTarget {
    draw: MfrDraw,
    palette: u8,
    points: Vec<Point>,
    lengths: Vec<u32>,
}

impl CTarget {
    /// Converts a paint code to the palette. EINK7 is the native encoding. RGB gets opaque
    /// mixes as their average color; transparent overlays keep pattern and B (= 15) with A as
    /// RGB, so a pattern-aware rasterizer can still stipple them.
    fn color(&self, code: u32) -> u32 {
        if self.palette == MFR_PALETTE_EINK7 {
            code
        } else if paint::is_transparent(code) {
            paint::rgb(code) | (code & 0xff00_0000)
        } else {
            paint::rgb(code)
        }
    }
}

impl DrawTarget for CTarget {
    fn fill_polygon(&mut self, rings: &[&[Point]], color: u32) {
        self.points.clear();
        self.lengths.clear();
        for ring in rings {
            self.points.extend_from_slice(ring);
            self.lengths.push(ring.len() as u32);
        }
        (self.draw.fill_polygon)(self.draw.ctx, self.points.as_ptr(), self.lengths.as_ptr(), self.lengths.len() as u32, self.color(color));
    }

    fn polyline(&mut self, points: &[Point], color: u32, width: u8) {
        (self.draw.polyline)(self.draw.ctx, points.as_ptr(), points.len() as u32, self.color(color), width);
    }

    fn point(&mut self, at: Point, color: u32, radius: u8) {
        (self.draw.point)(self.draw.ctx, at, self.color(color), radius);
    }

    fn label(&mut self, at: Point, text: &str, rank: u8) {
        if let Some(label) = self.draw.label {
            label(self.draw.ctx, at, text.as_ptr(), text.len() as u32, rank);
        }
    }

    fn icon(&mut self, at: Point, icon: Icon) -> bool {
        match self.draw.icon {
            Some(draw) => {
                draw(self.draw.ctx, at, icon as u8);
                true
            }
            None => false,
        }
    }
}

pub struct MfrMap {
    map: MapFile<CSource>,
    styles: StyleTable,
}

/// A parsed stylesheet, see [`Stylesheet`].
pub struct MfrStyle(Stylesheet);

/// Parses a stylesheet (the text format of `src/stylesheet.rs`, `styles/hiking.style`) from
/// `len` bytes of UTF-8. Returns null on failure with `MFR_ERR_STYLE` in `err` (optional) and
/// the line and reason in `mfr_last_error`. The text is not needed after the call.
///
/// # Safety
/// `text` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn mfr_style_parse(text: *const u8, len: usize, err: *mut c_int) -> *mut MfrStyle {
    let set_err = |code: c_int| {
        if !err.is_null() {
            *err = code;
        }
    };
    if text.is_null() {
        set_err(MFR_ERR_ARGUMENT);
        return core::ptr::null_mut();
    }
    let parsed = core::str::from_utf8(core::slice::from_raw_parts(text, len))
        .map_err(|_| remember_text(format_args!("style is not UTF-8")))
        .and_then(|t| Stylesheet::parse(t).map_err(|e| remember_text(format_args!("{e}"))));
    match parsed {
        Ok(sheet) => {
            set_err(MFR_OK);
            Box::into_raw(Box::new(MfrStyle(sheet)))
        }
        Err(()) => {
            set_err(MFR_ERR_STYLE);
            core::ptr::null_mut()
        }
    }
}

/// The built-in hiking stylesheet, as a starting point to change. Null if out of memory.
#[no_mangle]
pub extern "C" fn mfr_style_builtin() -> *mut MfrStyle {
    Box::into_raw(Box::new(MfrStyle(Stylesheet::builtin())))
}

/// # Safety
/// `style` must come from `mfr_style_parse`/`mfr_style_builtin` (or be null) and not be used
/// afterwards. Maps keep working: `mfr_set_style` copies what it needs.
#[no_mangle]
pub unsafe extern "C" fn mfr_style_free(style: *mut MfrStyle) {
    if !style.is_null() {
        drop(Box::from_raw(style));
    }
}

/// Renders `map` with `style` from now on (NULL = the built-in one). The style may be freed
/// afterwards.
///
/// # Safety
/// `map` must be valid, `style` valid or null.
#[no_mangle]
pub unsafe extern "C" fn mfr_set_style(map: *mut MfrMap, style: *const MfrStyle) -> c_int {
    if map.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let map = &mut *map;
    map.styles = match style.as_ref() {
        Some(style) => StyleTable::from_header(&style.0, &map.map.header),
        None => StyleTable::from_header(&Stylesheet::builtin(), &map.map.header),
    };
    MFR_OK
}

/// Opens a map. Returns null on failure; `err` (optional) receives an `MFR_ERR_*` code.
///
/// # Safety
/// `io` must point to valid callbacks that stay usable until `mfr_close`.
#[no_mangle]
pub unsafe extern "C" fn mfr_open(io: *const MfrIo, err: *mut c_int) -> *mut MfrMap {
    let set_err = |code: c_int| {
        if !err.is_null() {
            *err = code;
        }
    };
    if io.is_null() {
        set_err(MFR_ERR_ARGUMENT);
        return core::ptr::null_mut();
    }
    match MapFile::from_source(CSource::new(*io)) {
        Ok(map) => {
            set_err(MFR_OK);
            let styles = StyleTable::from_header(&Stylesheet::builtin(), &map.header);
            Box::into_raw(Box::new(MfrMap { map, styles }))
        }
        Err(e) => {
            set_err(error_code(&e));
            core::ptr::null_mut()
        }
    }
}

/// # Safety
/// `map` must come from `mfr_open` (or be null) and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mfr_close(map: *mut MfrMap) {
    if !map.is_null() {
        drop(Box::from_raw(map));
    }
}

/// Writes `min_lat, min_lon, max_lat, max_lon` of the map to `out[4]`.
///
/// # Safety
/// `map` must be valid and `out` point to four doubles.
#[no_mangle]
pub unsafe extern "C" fn mfr_bounds(map: *const MfrMap, out: *mut f64) -> c_int {
    if map.is_null() || out.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let b = (*map).map.header.bounding_box;
    *out.add(0) = b.min_lat;
    *out.add(1) = b.min_lon;
    *out.add(2) = b.max_lat;
    *out.add(3) = b.max_lon;
    MFR_OK
}

/// Draws the `width` x `height` pixel view with `lat`/`lon` at pixel `anchor_x`/`anchor_y`
/// (the middle of the screen if centered) at `zoom` through the callbacks, back to front. Returns the number of tiles read or a negative `MFR_ERR_*`.
///
/// # Safety
/// `map` must be valid, `draw` point to valid callbacks.
#[no_mangle]
pub unsafe extern "C" fn mfr_render(
    map: *mut MfrMap,
    lat: f64,
    lon: f64,
    zoom: u8,
    width: u16,
    height: u16,
    anchor_x: i16,
    anchor_y: i16,
    palette: u8,
    draw: *const MfrDraw,
) -> c_int {
    if map.is_null() || draw.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let map = &mut *map;
    let view = View { center_lat: lat, center_lon: lon, zoom, width, height, anchor_x, anchor_y, tile_size: map.map.header.tile_size };
    let mut target = CTarget { draw: *draw, palette, points: Vec::new(), lengths: Vec::new() };
    match map.map.render(&view, &map.styles, &mut target) {
        Ok(tiles) => tiles as c_int,
        Err(e) => error_code(&e),
    }
}

/// Creation date of the map file in milliseconds since 1970 (from its header), 0 if `map` is null.
/// Two maps of the same region can be told apart by it.
///
/// # Safety
/// `map` must be valid or null.
#[no_mangle]
pub unsafe extern "C" fn mfr_created(map: *const MfrMap) -> u64 {
    if map.is_null() {
        return 0;
    }
    (*map).map.header.creation_date
}

/// Shapes of one screen collected from several maps, see [`Scene`].
pub struct MfrScene(Scene);

/// An empty scene. Null if out of memory.
#[no_mangle]
pub extern "C" fn mfr_scene_new() -> *mut MfrScene {
    Box::into_raw(Box::new(MfrScene(Scene::new())))
}

/// # Safety
/// `scene` must come from `mfr_scene_new` (or be null) and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mfr_scene_free(scene: *mut MfrScene) {
    if !scene.is_null() {
        drop(Box::from_raw(scene));
    }
}

/// Adds the shapes of `map` in the view (same parameters as `mfr_render`) to `scene`. The map
/// can be closed afterwards. Returns the number of tiles read or a negative `MFR_ERR_*`; on an
/// error the shapes collected before stay in the scene.
///
/// # Safety
/// `scene` and `map` must be valid.
#[no_mangle]
pub unsafe extern "C" fn mfr_scene_collect(
    scene: *mut MfrScene,
    map: *mut MfrMap,
    lat: f64,
    lon: f64,
    zoom: u8,
    width: u16,
    height: u16,
    anchor_x: i16,
    anchor_y: i16,
) -> c_int {
    if scene.is_null() || map.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let map = &mut *map;
    let view = View { center_lat: lat, center_lon: lon, zoom, width, height, anchor_x, anchor_y, tile_size: map.map.header.tile_size };
    match map.map.collect(&view, &map.styles, &mut (*scene).0) {
        Ok(tiles) => tiles as c_int,
        Err(e) => error_code(&e),
    }
}

/// Draws all collected shapes back to front through the callbacks, then empties the scene for
/// the next screen. Returns `MFR_OK`.
///
/// # Safety
/// `scene` must be valid, `draw` point to valid callbacks.
#[no_mangle]
pub unsafe extern "C" fn mfr_scene_draw(scene: *mut MfrScene, palette: u8, draw: *const MfrDraw) -> c_int {
    if scene.is_null() || draw.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let scene = &mut (*scene).0;
    let mut target = CTarget { draw: *draw, palette, points: Vec::new(), lengths: Vec::new() };
    scene.draw(&mut target);
    scene.clear();
    MFR_OK
}

#[cfg(not(feature = "std"))]
mod runtime {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::c_void;

    extern "C" {
        /// Provided by the application: memory aligned to 8 bytes, null if out of memory.
        fn mfr_alloc(size: usize) -> *mut c_void;
        fn mfr_free(ptr: *mut c_void);
        fn abort() -> !;
    }

    /// Forwards to the application's heap, so it decides where the memory comes from (on the
    /// ESP32 PSRAM, which keeps the scarce internal RAM free for DMA buffers).
    struct AppAllocator;

    unsafe impl GlobalAlloc for AppAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if layout.align() > 8 {
                return core::ptr::null_mut();
            }
            mfr_alloc(layout.size()) as *mut u8
        }
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            mfr_free(ptr as *mut c_void)
        }
    }

    #[global_allocator]
    static ALLOCATOR: AppAllocator = AppAllocator;

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo) -> ! {
        unsafe { abort() }
    }
}
