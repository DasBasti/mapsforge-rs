/*
 * Minimal rasterizer for the mfr_draw_t callbacks: scanline polygon fill and thick lines on
 * top of a single `span` primitive (a horizontal run of pixels of one color). Implement
 * `span` for the framebuffer at hand; it is called with y and x0 <= x1 inside the view only
 * when clipped by mfr_raster_t.width/height.
 */
#ifndef MFR_RASTER_H
#define MFR_RASTER_H

#include "mapsforge.h"

typedef struct {
    void* ctx;
    /* fills pixels x0..x1 (inclusive) of row y, always inside 0..width/height */
    void (*span)(void* ctx, int y, int x0, int x1, uint32_t color);
    int width;
    int height;
    /* color of the background, the second color of MFR_FILL_CHECKER fills */
    uint32_t white;
} mfr_raster_t;

void mfr_raster_fill_polygon(const mfr_raster_t* r, const mfr_point_t* points,
                             const uint32_t* ring_lengths, uint32_t ring_count, uint32_t color);
void mfr_raster_polyline(const mfr_raster_t* r, const mfr_point_t* points, uint32_t count,
                         uint32_t color, uint8_t width);
void mfr_raster_disc(const mfr_raster_t* r, int cx, int cy, int radius, uint32_t color);

#endif /* MFR_RASTER_H */
