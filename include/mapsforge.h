/*
 * C API of mapsforge-rs: renders a Mapsforge (.map) vector map as drawing primitives.
 *
 * The caller provides file access (mfr_io) and drawing callbacks (mfr_draw); the library
 * decodes the tiles around a position, projects them to screen pixels and calls the
 * callbacks back to front.
 */
#ifndef MAPSFORGE_H
#define MAPSFORGE_H

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MFR_OK 0
#define MFR_ERR_IO -1       /* file read/seek failed */
#define MFR_ERR_FORMAT -2   /* not a (supported) Mapsforge file */
#define MFR_ERR_ARGUMENT -3
#define MFR_ERR_TAG -4      /* tag id not in the header's tag table */
#define MFR_ERR_TILE -5     /* tile outside of the map */
#define MFR_ERR_CORRUPT -6  /* implausible sizes in the file, or out of memory */

/* colors are passed to the draw callbacks as 0xRRGGBB */
#define MFR_PALETTE_RGB 0
/* colors are passed as index of the 7-color e-paper: black, white, green, blue, red,
 * yellow, orange (color_t of the firmware) */
#define MFR_PALETTE_EINK7 1

typedef struct mfr_map mfr_map_t;

typedef struct {
    int16_t x;
    int16_t y;
} mfr_point_t;

typedef struct {
    void* ctx;
    /* reads up to len bytes, returns the number read (0 = end of file) or < 0 on error */
    ssize_t (*read)(void* ctx, uint8_t* buf, size_t len);
    /* moves the read position to the absolute offset, returns 0 on success */
    int (*seek)(void* ctx, uint64_t offset);
} mfr_io_t;

typedef struct {
    void* ctx;
    /* points holds all rings back to back, ring_lengths[i] is the number of points of ring i.
     * Fill with the even-odd rule, inner rings are holes. */
    void (*fill_polygon)(void* ctx, const mfr_point_t* points, const uint32_t* ring_lengths,
                         uint32_t ring_count, uint32_t color);
    void (*polyline)(void* ctx, const mfr_point_t* points, uint32_t count, uint32_t color,
                     uint8_t width);
    void (*point)(void* ctx, mfr_point_t at, uint32_t color, uint8_t radius);
} mfr_draw_t;

/* Opens a map. Returns NULL on failure, err (optional) receives MFR_OK or MFR_ERR_*.
 * The callbacks must stay usable until mfr_close. */
mfr_map_t* mfr_open(const mfr_io_t* io, int* err);
void mfr_close(mfr_map_t* map);

/* out = { min_lat, min_lon, max_lat, max_lon } */
int mfr_bounds(const mfr_map_t* map, double out[4]);

/* Draws the width x height pixel view at the zoom level (the Web Mercator zoom of 256 px tiles)
 * through the callbacks, back to front. lat/lon appears at pixel anchor_x/anchor_y (pass
 * width / 2 and height / 2 to center it). Returns the number of tiles read or a negative
 * MFR_ERR_*. */
int mfr_render(mfr_map_t* map, double lat, double lon, uint8_t zoom, uint16_t width,
               uint16_t height, int16_t anchor_x, int16_t anchor_y, uint8_t palette,
               const mfr_draw_t* draw);

/* Short description of the last error for logs (zero terminated, valid until the next call). */
const char* mfr_last_error(void);

/* Only when the library is built without std (embedded): the application provides the heap.
 * mfr_alloc returns memory aligned to 8 bytes or NULL, mfr_free releases it. */
void* mfr_alloc(size_t size);
void mfr_free(void* ptr);

#ifdef __cplusplus
}
#endif

#endif /* MAPSFORGE_H */
