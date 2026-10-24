//! C API, see `include/mapsforge.h`.
use alloc::{boxed::Box, vec::Vec};
use core::ffi::{c_int, c_void};

use crate::{
    error::MapforgeError,
    io::{ByteRead, ByteSource},
    render::{DrawTarget, Point},
    style::{eink7_index, StyleTable},
    types::MapFile,
    view::View,
    Result,
};

pub const MFR_OK: c_int = 0;
pub const MFR_ERR_IO: c_int = -1;
pub const MFR_ERR_FORMAT: c_int = -2;
pub const MFR_ERR_ARGUMENT: c_int = -3;

/// Colors are passed as 0xRRGGBB.
pub const MFR_PALETTE_RGB: u8 = 0;
/// Colors are passed as index of the 7-color e-paper: black, white, green, blue, red,
/// yellow, orange (the firmware's `color_t`).
pub const MFR_PALETTE_EINK7: u8 = 1;

fn error_code(e: &MapforgeError) -> c_int {
    match e {
        MapforgeError::Source => MFR_ERR_IO,
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
}

struct CTarget {
    draw: MfrDraw,
    palette: u8,
    points: Vec<Point>,
    lengths: Vec<u32>,
}

impl CTarget {
    fn color(&self, rgb: u32, is_line: bool) -> u32 {
        if self.palette == MFR_PALETTE_EINK7 {
            eink7_index(rgb, is_line) as u32
        } else {
            rgb
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
        (self.draw.fill_polygon)(self.draw.ctx, self.points.as_ptr(), self.lengths.as_ptr(), self.lengths.len() as u32, self.color(color, false));
    }

    fn polyline(&mut self, points: &[Point], color: u32, width: u8) {
        (self.draw.polyline)(self.draw.ctx, points.as_ptr(), points.len() as u32, self.color(color, true), width);
    }

    fn point(&mut self, at: Point, color: u32, radius: u8) {
        (self.draw.point)(self.draw.ctx, at, self.color(color, false), radius);
    }
}

pub struct MfrMap {
    map: MapFile<CSource>,
    styles: StyleTable,
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
            let styles = StyleTable::from_header(&map.header);
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

/// Draws the `width` x `height` pixel view centered on `lat`/`lon` at `zoom` through the
/// callbacks, back to front. Returns the number of tiles read or a negative `MFR_ERR_*`.
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
    palette: u8,
    draw: *const MfrDraw,
) -> c_int {
    if map.is_null() || draw.is_null() {
        return MFR_ERR_ARGUMENT;
    }
    let map = &mut *map;
    let view = View { center_lat: lat, center_lon: lon, zoom, width, height, tile_size: map.map.header.tile_size };
    let mut target = CTarget { draw: *draw, palette, points: Vec::new(), lengths: Vec::new() };
    match map.map.render(&view, &map.styles, &mut target) {
        Ok(tiles) => tiles as c_int,
        Err(e) => error_code(&e),
    }
}

#[cfg(not(feature = "std"))]
mod runtime {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::c_void;

    extern "C" {
        fn malloc(size: usize) -> *mut c_void;
        fn free(ptr: *mut c_void);
        fn abort() -> !;
    }

    /// Forwards to the C heap (on ESP-IDF this can live in PSRAM).
    struct CAllocator;

    unsafe impl GlobalAlloc for CAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // malloc is aligned for any primitive type, which is all this crate needs
            malloc(layout.size()) as *mut u8
        }
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            free(ptr as *mut c_void)
        }
    }

    #[global_allocator]
    static ALLOCATOR: CAllocator = CAllocator;

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo) -> ! {
        unsafe { abort() }
    }
}
