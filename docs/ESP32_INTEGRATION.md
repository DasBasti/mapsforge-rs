# Using mapsforge-rs in the IndiaNavi firmware

Target: `IndiaNavi_Firmware` (ESP-IDF via PlatformIO, ESP32-S3 N16R8 main target, 600x448 7-color
e-paper, `display_t` framebuffer, SD card through FatFs under `sd_lock()`).

## Pieces

| Piece | Where |
|---|---|
| Rust library, C API | `src/ffi.rs`, `include/mapsforge.h` (feature `ffi`) |
| Scanline polygon fill + thick lines on top of one `span` primitive | `c/mfr_raster.{h,c}` |
| Host example (PPM output, also `eink` palette) | `c/example/render_ppm.c` |

## Integration in the firmware (done, branch `feature/vector-map` of IndiaNavi_Firmware)

- `tools/mapsforge_rs.py` (PlatformIO `extra_scripts`) runs
  `rustup run esp cargo build --release -Zbuild-std=core,alloc --target xtensa-esp32s3-none-elf --no-default-features --features ffi`
  and links `libmapsforge_rs.a`. PlatformIO does not run CMake custom targets, hence the script.
- `components/mapsforge/CMakeLists.txt` compiles `c/mfr_raster.c`, adds the include paths and defines
  `VECTOR_MAP`. Without the checkout it is an empty component and the firmware uses tiles.
- `src/esp32/vector_map.c`: FatFs read/seek callbacks, `span` = `display_rect_fill(.., 1 row)`,
  `mfr_render(.., MFR_PALETTE_EINK7)`. The file is opened per render under `sd_lock()`.
- `map_screen.c`: if `//MAPS/map.map` exists the `RL_MAP` item draws the vector map, anchored at
  `(box.left + 256 + pos_x, box.top + 256 + pos_y)` which is where the tile map puts the position,
  so marker, track and waypoints still fit. The tile loaders are not attached then.
- Needs `espup` (Xtensa Rust toolchain "esp"). Verified: S3 debug/release and esp32dev debug build and link.
  Not yet run on a device.

## Notes / open points

- The display refreshes once a minute, so a render of 1-3 s is acceptable.
- ESP32/ESP32-S3 only have a single-precision FPU; the library uses `f64` for tile math, which is
  emulated (slow but only a few ops per vertex). Measure first, move to `f32`/fixed point if needed.
- Memory: a render keeps all projected points of the visible tiles (about 4 bytes per vertex) until
  drawing. Fine with PSRAM; for the PSRAM-less `esp32dev` board it needs a cap or per-tile drawing.
- Colors and rules: see `docs/COLOR_SCHEMA.md` (inks and dither mixes, `MFR_PAT_*` in
  `mapsforge.h`). Dithered fills cost more `span` calls (rural z14 view: ~160k instead of ~60k,
  city: fewer than before); `span` is still called with a plain palette index.
