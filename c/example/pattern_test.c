/*
 * Checks the dither patterns of mfr_raster.c pixel by pixel against MFR_PATTERN_MASKS for all
 * patterns, B kinds and polygon positions, and prints the span calls per fill.
 *   cc -Iinclude -Ic c/example/pattern_test.c c/mfr_raster.c -o pattern_test && ./pattern_test
 */
#include <stdio.h>
#include "../mfr_raster.h"
static unsigned char fb[40][40]; static long calls;
static void span(void* c __attribute__((unused)), int y, int x0, int x1, uint32_t color) { calls++; for (int x = x0; x <= x1; x++) fb[y][x] = color; }
static const uint16_t M[] = MFR_PATTERN_MASKS;
int main(void) {
    mfr_raster_t r = { .ctx = 0, .span = span, .width = 40, .height = 40, .white = 1 };
    int bad = 0;
    unsigned bs[] = { MFR_B_BACKGROUND, MFR_B_INDEX(5), MFR_B_TRANSPARENT };
    for (int p = 0; p < MFR_PAT_COUNT; p++) for (int bi = 0; bi < 3; bi++) for (int off = 0; off < 4; off++) {
        for (int y = 0; y < 40; y++) for (int x = 0; x < 40; x++) fb[y][x] = 9;
        mfr_point_t pts[4] = { {3 + off, 2}, {37, 2 + off}, {36, 38}, {1, 35} };
        uint32_t len = 4; calls = 0;
        mfr_raster_fill_polygon(&r, pts, &len, 1, MFR_MIX(2, bs[bi], p));
        long px = 0;
        for (int y = 0; y < 40; y++) for (int x = 0; x < 40; x++) {
            if (fb[y][x] == 9) continue;
            px++;
            int a = (M[p] >> ((y & 3) * 4 + (x & 3))) & 1;
            int want = a ? 2 : bi == 0 ? 1 : bi == 1 ? 5 : 9;
            if (fb[y][x] != want) bad++;
        }
        if (off == 0) printf("pattern %d B %u: %ld px, %ld span calls\n", p, bs[bi], px, calls);
    }
    printf("bad pixels: %d\n", bad);
    return bad != 0;
}
