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
#define MFR_ERR_STYLE -7    /* invalid stylesheet, mfr_last_error() has the line and reason */

/* colors are passed to the draw callbacks as 0xRRGGBB */
#define MFR_PALETTE_RGB 0
/* colors are passed as index of the 7-color e-paper: black, white, green, blue, red,
 * yellow, orange (color_t of the firmware) */
#define MFR_PALETTE_EINK7 1

/*
 * Patterned colors (dither mixes). With MFR_PALETTE_EINK7 the color passed to the callbacks
 * (fill_polygon, polyline and point) is
 *
 *   bits  0..7   color A, palette index
 *   bits  8..23  0
 *   bits 24..27  pattern id MFR_PAT_*: which pixels get color A, the others get color B
 *   bits 28..31  color B: 0 = the background (white, MFR_FILL_CHECKER compatible),
 *                1..7 = palette index + 1, 15 = transparent (pixels of B are not painted,
 *                for overlays such as wetland over forest)
 *
 * A plain palette index (pattern 0) is a solid color, so callbacks that ignore the high bits
 * still get a usable color (A) with MFR_COLOR_A(). Patterns are 4x4 cells anchored at the
 * absolute screen pixel (x & 3, y & 3), so neighbouring polygons and tiles join seamlessly.
 *
 * With MFR_PALETTE_RGB the low 24 bits are 0xRRGGBB. Opaque mixes are passed pre-blended as
 * the solid average color (high bits 0); transparent overlays keep pattern id and B = 15 with
 * A as RGB, so a rasterizer can stipple them (or ignore the high bits and paint A solid).
 */
#define MFR_COLOR_A(c) ((c) & 0x00ffffffu)
#define MFR_COLOR_PATTERN(c) (((c) >> 24) & 0x0fu)
#define MFR_COLOR_B(c) (((c) >> 28) & 0x0fu)
#define MFR_B_BACKGROUND 0u
#define MFR_B_TRANSPARENT 15u
#define MFR_B_INDEX(index) ((uint32_t)(index) + 1u)
/* patterned color from palette index a, B code b (MFR_B_*) and pattern id */
#define MFR_MIX(a, b, pattern) ((uint32_t)(a) | ((uint32_t)(pattern) << 24) | ((uint32_t)(b) << 28))

#define MFR_PAT_SOLID 0    /* A only */
#define MFR_PAT_50 1       /* A on even x + y (checkerboard) */
#define MFR_PAT_25 2       /* A on even x and even y: square grid of dots */
#define MFR_PAT_75 3       /* B on even x and odd y (P50 and P25 are subsets) */
#define MFR_PAT_12 4       /* A at (0,0) and (2,2) of the 4x4 cell: diagonal grid of dots */
#define MFR_PAT_6 5        /* A at (0,0) of the 4x4 cell: sparse dots */
#define MFR_PAT_HLINES 6   /* A on rows with y % 4 == 0: horizontal hatching, 25% */
#define MFR_PAT_DIAG 7     /* A where (x + y) % 4 == 0: diagonal hatching, 25% */
#define MFR_PAT_COUNT 8

/* 4x4 masks of the patterns: bit (x & 3) of nibble (y & 3) set = color A */
#define MFR_PATTERN_MASKS { 0xffff, 0xa5a5, 0x0505, 0xafaf, 0x0401, 0x0001, 0x000f, 0x2481 }

/* flag in the color of fill_polygon: fill with a 50% checkerboard of the color and the
 * background (= MFR_PAT_50 with B = background, kept for compatibility) */
#define MFR_FILL_CHECKER 0x01000000u

typedef struct mfr_map mfr_map_t;
typedef struct mfr_style mfr_style_t;
typedef struct mfr_scene mfr_scene_t;

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
    /* optional (may be NULL): the name of a place (city, town, village...) at its position, called
     * after all shapes, rank 1 (city) to 4 (hamlet) in this order, so names that do not fit can be
     * skipped. text is UTF-8 and not zero terminated. */
    void (*label)(void* ctx, mfr_point_t at, const char* text, uint32_t len, uint8_t rank);
} mfr_draw_t;

/* Opens a map. Returns NULL on failure, err (optional) receives MFR_OK or MFR_ERR_*.
 * The callbacks must stay usable until mfr_close. */
mfr_map_t* mfr_open(const mfr_io_t* io, int* err);
void mfr_close(mfr_map_t* map);

/* out = { min_lat, min_lon, max_lat, max_lon } */
int mfr_bounds(const mfr_map_t* map, double out[4]);

/* Creation date of the map file in milliseconds since 1970 (from its header), 0 if map is NULL. */
uint64_t mfr_created(const mfr_map_t* map);

/*
 * Stylesheets: which tags are drawn how (colors, widths, draw order, symbols, labels, what is
 * hidden). Without one the built-in hiking schema is used. The text format is documented in
 * src/stylesheet.rs; styles/hiking.style is the built-in one to start from, e.g.
 *
 *   way highway=primary* : line color=red width=2 order=road casing=1 shrink
 *   way natural=wood*    : area color=green/75 order=forest
 *   poi place=city       : label rank=1
 */

/* Parses a stylesheet of len bytes (UTF-8, not necessarily zero terminated; not needed after
 * the call). Returns NULL on failure, err (optional) receives MFR_ERR_STYLE or
 * MFR_ERR_ARGUMENT. */
mfr_style_t* mfr_style_parse(const char* text, size_t len, int* err);
/* The built-in stylesheet. NULL if out of memory. */
mfr_style_t* mfr_style_builtin(void);
void mfr_style_free(mfr_style_t* style);

/* Renders map with style from now on (NULL = built-in). The style can be freed afterwards, a
 * new one replaces the old. Returns MFR_OK. */
int mfr_set_style(mfr_map_t* map, const mfr_style_t* style);

/* Draws the width x height pixel view at the zoom level (the Web Mercator zoom of 256 px tiles)
 * through the callbacks, back to front. lat/lon appears at pixel anchor_x/anchor_y (pass
 * width / 2 and height / 2 to center it). Returns the number of tiles read or a negative
 * MFR_ERR_*. */
int mfr_render(mfr_map_t* map, double lat, double lon, uint8_t zoom, uint16_t width,
               uint16_t height, int16_t anchor_x, int16_t anchor_y, uint8_t palette,
               const mfr_draw_t* draw);

/*
 * Several maps on one screen: the shapes of all maps are collected into a scene and drawn
 * together in one drawing order, so the land of one map lies below the forests and roads of the
 * other where they overlap. A map can be closed right after it was collected, the scene keeps
 * only screen points.
 *
 *   mfr_scene_t* scene = mfr_scene_new();       // keep it, its memory is reused
 *   for each map: open, mfr_scene_collect(scene, map, ...), close
 *   mfr_scene_draw(scene, MFR_PALETTE_EINK7, &draw);   // draws and empties the scene
 */
mfr_scene_t* mfr_scene_new(void);
void mfr_scene_free(mfr_scene_t* scene);

/* Adds the shapes of map in the view (as mfr_render) to the scene. Returns the number of tiles
 * read or a negative MFR_ERR_*; the shapes collected before an error stay in the scene. */
int mfr_scene_collect(mfr_scene_t* scene, mfr_map_t* map, double lat, double lon, uint8_t zoom,
                      uint16_t width, uint16_t height, int16_t anchor_x, int16_t anchor_y);

/* Draws the collected shapes back to front through the callbacks, then the place names, and
 * empties the scene. Returns MFR_OK. */
int mfr_scene_draw(mfr_scene_t* scene, uint8_t palette, const mfr_draw_t* draw);

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
