/*
 * Host example: renders a view of a .map file into a PPM image.
 *   render_ppm <file.map> <lat> <lon> <zoom> <out.ppm> [width height] [eink]
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../mfr_raster.h"

static const uint32_t EINK7_RGB[7] = { 0x000000, 0xffffff, 0x2e8b3a, 0x2850b4, 0xc81e1e, 0xf0d020, 0xe67828 };

typedef struct {
    uint8_t* rgb;
    int width, height;
    int eink;
} canvas_t;

static uint32_t to_rgb(const canvas_t* c, uint32_t color)
{
    return c->eink ? EINK7_RGB[color % 7] : color;
}

static void span(void* ctx, int y, int x0, int x1, uint32_t color)
{
    canvas_t* c = ctx;
    uint32_t rgb = to_rgb(c, color);
    for (int x = x0; x <= x1; x++) {
        uint8_t* p = c->rgb + ((size_t)y * c->width + x) * 3;
        p[0] = rgb >> 16;
        p[1] = rgb >> 8;
        p[2] = rgb;
    }
}

static mfr_raster_t raster_of(canvas_t* c)
{
    mfr_raster_t r = { c, span, c->width, c->height, c->eink ? 1 : 0xffffff };
    return r;
}

static void fill_polygon(void* ctx, const mfr_point_t* pts, const uint32_t* lens, uint32_t n, uint32_t color)
{
    mfr_raster_t r = raster_of(ctx);
    mfr_raster_fill_polygon(&r, pts, lens, n, color);
}

static void polyline(void* ctx, const mfr_point_t* pts, uint32_t n, uint32_t color, uint8_t width)
{
    mfr_raster_t r = raster_of(ctx);
    mfr_raster_polyline(&r, pts, n, color, width);
}

static void point(void* ctx, mfr_point_t at, uint32_t color, uint8_t radius)
{
    mfr_raster_t r = raster_of(ctx);
    mfr_raster_disc(&r, at.x, at.y, radius, color);
}

static ssize_t io_read(void* ctx, uint8_t* buf, size_t len)
{
    size_t n = fread(buf, 1, len, ctx);
    return n == 0 && ferror(ctx) ? -1 : (ssize_t)n;
}

static int io_seek(void* ctx, uint64_t offset)
{
    return fseeko(ctx, (off_t)offset, SEEK_SET);
}

int main(int argc, char** argv)
{
    if (argc < 6) {
        fprintf(stderr, "usage: %s file.map lat lon zoom out.ppm [width height] [eink]\n", argv[0]);
        return 2;
    }
    canvas_t canvas = { .width = argc > 7 ? atoi(argv[6]) : 480, .height = argc > 7 ? atoi(argv[7]) : 320,
        .eink = argc > 8 && strcmp(argv[8], "eink") == 0 };

    FILE* f = fopen(argv[1], "rb");
    if (!f) {
        perror("open");
        return 1;
    }
    mfr_io_t io = { f, io_read, io_seek };
    int err;
    mfr_map_t* map = mfr_open(&io, &err);
    if (!map) {
        fprintf(stderr, "mfr_open failed: %d\n", err);
        return 1;
    }

    canvas.rgb = malloc((size_t)canvas.width * canvas.height * 3);
    memset(canvas.rgb, 0xff, (size_t)canvas.width * canvas.height * 3);

    mfr_draw_t draw = { &canvas, fill_polygon, polyline, point };
    int tiles = mfr_render(map, atof(argv[2]), atof(argv[3]), (uint8_t)atoi(argv[4]), canvas.width,
        canvas.height, canvas.width / 2, canvas.height / 2, canvas.eink ? MFR_PALETTE_EINK7 : MFR_PALETTE_RGB, &draw);
    printf("tiles read: %d\n", tiles);

    FILE* out = fopen(argv[5], "wb");
    fprintf(out, "P6\n%d %d\n255\n", canvas.width, canvas.height);
    fwrite(canvas.rgb, 3, (size_t)canvas.width * canvas.height, out);
    fclose(out);

    mfr_close(map);
    fclose(f);
    return tiles < 0;
}
