#include "mfr_raster.h"

#include <stdlib.h>

#define MAX_CROSSINGS 128

static void span_clipped(const mfr_raster_t* r, int y, int x0, int x1, uint32_t color)
{
    if (y < 0 || y >= r->height || x1 < 0 || x0 >= r->width)
        return;
    if (x0 < 0)
        x0 = 0;
    if (x1 >= r->width)
        x1 = r->width - 1;
    if (x0 <= x1)
        r->span(r->ctx, y, x0, x1, color);
}

/* 50% checkerboard: pixels with even x + y get the color, all others the background */
static void span_checker(const mfr_raster_t* r, int y, int x0, int x1, uint32_t color)
{
    span_clipped(r, y, x0, x1, r->white);
    for (int x = x0 + ((x0 + y) & 1); x <= x1; x += 2)
        span_clipped(r, y, x, x, color);
}

static int compare_int(const void* a, const void* b)
{
    return *(const int*)a - *(const int*)b;
}

void mfr_raster_fill_polygon(const mfr_raster_t* r, const mfr_point_t* points,
                             const uint32_t* ring_lengths, uint32_t ring_count, uint32_t color)
{
    int checker = (color & MFR_FILL_CHECKER) != 0;
    color &= ~MFR_FILL_CHECKER;
    uint32_t total = 0;
    int min_y = r->height, max_y = -1;
    for (uint32_t i = 0; i < ring_count; i++)
        total += ring_lengths[i];
    for (uint32_t i = 0; i < total; i++) {
        if (points[i].y < min_y)
            min_y = points[i].y;
        if (points[i].y > max_y)
            max_y = points[i].y;
    }
    if (min_y < 0)
        min_y = 0;
    if (max_y >= r->height)
        max_y = r->height - 1;

    int crossings[MAX_CROSSINGS];
    for (int y = min_y; y <= max_y; y++) {
        int n = 0;
        const mfr_point_t* ring = points;
        for (uint32_t k = 0; k < ring_count; k++) {
            uint32_t len = ring_lengths[k];
            for (uint32_t i = 0; i < len && n < MAX_CROSSINGS; i++) {
                /* edges implicitly close the ring */
                const mfr_point_t a = ring[i];
                const mfr_point_t b = ring[(i + 1) % len];
                if ((a.y <= y && b.y > y) || (b.y <= y && a.y > y)) {
                    int x = a.x + (int)(((int32_t)(y - a.y) * (b.x - a.x)) / (b.y - a.y));
                    crossings[n++] = x;
                }
            }
            ring += len;
        }
        qsort(crossings, n, sizeof(int), compare_int);
        for (int i = 0; i + 1 < n; i += 2)
            (checker ? span_checker : span_clipped)(r, y, crossings[i], crossings[i + 1], color);
    }
}

void mfr_raster_disc(const mfr_raster_t* r, int cx, int cy, int radius, uint32_t color)
{
    for (int dy = -radius; dy <= radius; dy++) {
        int dx = 0;
        while ((dx + 1) * (dx + 1) + dy * dy <= radius * radius + radius)
            dx++;
        span_clipped(r, cy + dy, cx - dx, cx + dx, color);
    }
}

static void line1(const mfr_raster_t* r, int x0, int y0, int x1, int y1, uint32_t color)
{
    int dx = abs(x1 - x0), sx = x0 < x1 ? 1 : -1;
    int dy = -abs(y1 - y0), sy = y0 < y1 ? 1 : -1;
    int err = dx + dy;
    for (;;) {
        span_clipped(r, y0, x0, x0, color);
        if (x0 == x1 && y0 == y1)
            break;
        int e2 = 2 * err;
        if (e2 >= dy) {
            err += dy;
            x0 += sx;
        }
        if (e2 <= dx) {
            err += dx;
            y0 += sy;
        }
    }
}

/* the part of the segment inside an enlarged view, so very long off-screen segments stay cheap */
static int segment_visible(const mfr_raster_t* r, mfr_point_t a, mfr_point_t b)
{
    return !((a.x < 0 && b.x < 0) || (a.y < 0 && b.y < 0) || (a.x >= r->width && b.x >= r->width)
        || (a.y >= r->height && b.y >= r->height));
}

void mfr_raster_polyline(const mfr_raster_t* r, const mfr_point_t* points, uint32_t count,
                         uint32_t color, uint8_t width)
{
    for (uint32_t i = 0; i + 1 < count; i++) {
        mfr_point_t a = points[i], b = points[i + 1];
        if (!segment_visible(r, a, b))
            continue;
        if (width <= 1) {
            line1(r, a.x, a.y, b.x, b.y, color);
            continue;
        }
        /* a quad of the line's width around the segment, round caps for wide lines */
        float dx = b.x - a.x, dy = b.y - a.y;
        float len = dx * dx + dy * dy;
        if (len > 0) {
            float inv = (width / 2.0f) / __builtin_sqrtf(len);
            float nx = -dy * inv, ny = dx * inv;
            mfr_point_t quad[4] = {
                { (int16_t)(a.x + nx), (int16_t)(a.y + ny) },
                { (int16_t)(b.x + nx), (int16_t)(b.y + ny) },
                { (int16_t)(b.x - nx), (int16_t)(b.y - ny) },
                { (int16_t)(a.x - nx), (int16_t)(a.y - ny) },
            };
            uint32_t ring_len = 4;
            mfr_raster_fill_polygon(r, quad, &ring_len, 1, color);
        }
        if (width >= 3) {
            mfr_raster_disc(r, a.x, a.y, width / 2, color);
            mfr_raster_disc(r, b.x, b.y, width / 2, color);
        }
    }
}
