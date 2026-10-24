# Using mapsforge-rs in the IndiaNavi firmware

Target: `IndiaNavi_Firmware` (ESP-IDF via PlatformIO, ESP32-S3 N16R8 main target, 600x448 7-color
e-paper, `display_t` framebuffer, SD card through FatFs under `sd_lock()`).

## Pieces

| Piece | Where |
|---|---|
| Rust library, C API | `src/ffi.rs`, `include/mapsforge.h` (feature `ffi`) |
| Scanline polygon fill + thick lines on top of one `span` primitive | `c/mfr_raster.{h,c}` |
| Host example (PPM output, also `eink` palette) | `c/example/render_ppm.c` |

## Build the static library

`libmapsforge_rs.a` has no dependencies besides libc (`malloc`, `free`, `abort`, `mem*`) when built
without `std`; it provides its own allocator (C heap, so PSRAM when `malloc` is configured to use it)
and panic handler (abort).

```sh
# one time: espup install   (Xtensa Rust toolchain)
cargo +esp build --release -Zbuild-std=core,alloc \
    --target xtensa-esp32s3-none-elf --no-default-features --features ffi
# -> target/xtensa-esp32s3-none-elf/release/libmapsforge_rs.a
```
(Host check without the toolchain: `cargo build --release --no-default-features --features ffi`.)
Link it into the firmware as a prebuilt library, e.g. a PlatformIO `extra_scripts` step that runs the
cargo command and adds `-L... -lmapsforge_rs`, or an ESP-IDF component with
`add_prebuilt_library`. Add `include/mapsforge.h` and `c/mfr_raster.c` to the include/source path.

## Glue in the firmware (sketch, not compiled yet)

```c
// read/seek callbacks over FatFs, hold sd_lock() while rendering
static ssize_t io_read(void* ctx, uint8_t* buf, size_t len) {
    UINT n; return f_read((FIL*)ctx, buf, len, &n) == FR_OK ? (ssize_t)n : -1;
}
static int io_seek(void* ctx, uint64_t off) { return f_lseek((FIL*)ctx, off) == FR_OK ? 0 : -1; }

// span primitive on the display framebuffer; colors are color_t with MFR_PALETTE_EINK7
static void span(void* ctx, int y, int x0, int x1, uint32_t color) {
    display_rect_fill((const display_t*)ctx, x0, y, x1 - x0 + 1, 1, (color_t)color);
}
// fill_polygon/polyline/point forward to mfr_raster_* with mfr_raster_t{dsp, span, 600, 448}
// render: mfr_render(map, lat, lon, zoom, 600, 448, MFR_PALETTE_EINK7, &draw);
```

`mfr_render` can replace `map_update_tiles` + the tile `image_t`s: call it from the map screen's
render path with the GPS position (`map_position_t`) and zoom level (16 / 14 today).

## Notes / open points

- The display refreshes once a minute, so a render of 1-3 s is acceptable.
- ESP32/ESP32-S3 only have a single-precision FPU; the library uses `f64` for tile math, which is
  emulated (slow but only a few ops per vertex). Measure first, move to `f32`/fixed point if needed.
- Memory: a render keeps all projected points of the visible tiles (about 4 bytes per vertex) until
  drawing. Fine with PSRAM; for the PSRAM-less `esp32dev` board it needs a cap or per-tile drawing.
- The e-paper color mapping (`style::eink7_index`) and rule table (`style.rs`) are a first draft:
  POIs are too dense at zoom 14 and buildings are white on white.
