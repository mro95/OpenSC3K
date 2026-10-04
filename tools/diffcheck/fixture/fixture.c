/* A stand-in for SIMDIRT.DLL and SIMBABLD.DLL used by tools/diffcheck/selftest.py: cRZRandom
 * written from docs/sim/random.md, a cRZFastCompression3 look-alike with the QFS decoder
 * written from docs/formats/qfs.md, and probes of the harness itself (imports, x87 results,
 * fs:[0]).
 * Built with clang + lld-link for 32-bit Windows, x87 only (-mno-sse), no C runtime. */
typedef unsigned int u32;
typedef unsigned long long u64;

__declspec(dllimport) double sqrt(double);
__declspec(dllimport) void *__cdecl operator_new(u32) __asm__("??2@YAPAXI@Z");
__declspec(dllimport) int not_stubbed(void);

int _fltused; /* the C runtime normally defines this when floating point is used */

typedef struct { u32 state, dstate; } Rng;

/* noinline keeps each method a real call, as in the original, so the trace hooks see them. */
#define EXPORT __declspec(dllexport) __attribute__((noinline))

EXPORT void __thiscall rng_seed(Rng *r, u32 seed) { r->state = seed; r->dstate = seed; }

EXPORT u32 __thiscall rng_next(Rng *r) {
    u64 p = (u64)r->state * 0x41C64E6Du + 0x3039u;
    r->state = (u32)p;
    return (u32)(p >> 16);
}

EXPORT u32 __thiscall rng_uniform(Rng *r, u32 n) {
    if (n == 0) return 0;
    if (n < 0xFFFFFFu) return rng_next(r) % n;
    for (;;) { u32 v = rng_next(r); if (v <= n) return v; }
}

EXPORT int __thiscall rng_range(Rng *r, int lo, int hi) {
    return (int)rng_uniform(r, (u32)(hi - lo)) + lo;
}

EXPORT int __thiscall rng_gaussian_fast(Rng *r, int lo, int hi) {
    int half = (hi - lo) / 2;
    int a = (int)rng_uniform(r, (u32)half), b = (int)rng_uniform(r, (u32)half);
    int m = a < b ? a : b;
#ifdef BROKEN
    if (rng_uniform(r, 3)) m = -m;   /* a deliberate bug, to show the checker catches it */
#else
    if (rng_uniform(r, 2)) m = -m;
#endif
    return lo + half + m;
}

EXPORT int __thiscall rng_min_of_two(Rng *r, int lo, int hi) {
    u32 n = (u32)(hi - lo);
    int a = (int)rng_uniform(r, n), b = (int)rng_uniform(r, n);
    return (a < b ? a : b) + lo;
}

static const union { u64 bits; double d; } scale = { 0x3DF0000000000007ull };

EXPORT double __thiscall rng_double(Rng *r) {
    if (r->dstate == 0) r->dstate = 0x12345678u;
    r->dstate *= 0x278DDE6Du;
    return (double)r->dstate * scale.d;
}

EXPORT double __thiscall rng_double_range(Rng *r, double lo, double hi) {
    return rng_double(r) * (hi - lo) + lo;
}

/* Probes. */
EXPORT double probe_sqrt(double x) { return sqrt(x); }
EXPORT u32 probe_new(u32 n) { u32 *p = operator_new(n); p[0] = 0xC0FFEE; return (u32)p; }
EXPORT int probe_missing(void) { return not_stubbed(); }
EXPORT u32 probe_fs(void) { u32 v; __asm__("movl %%fs:0x18, %0" : "=r"(v)); return v; }
/* Outermost calls rng_seed, rng_gaussian_fast, rng_double_range; the nested rng_uniform and
 * rng_double calls must not show up in a trace. */
EXPORT u32 probe_trace(Rng *r) {
    rng_seed(r, 7);
    int g = rng_gaussian_fast(r, -3, 9);
    double d = rng_double_range(r, 1.0, 2.0);
    return (u32)g + (d > 1.5);
}

/* cRZFastCompression3. The compressor only writes literals: enough for a round trip. */
typedef unsigned char u8;
typedef struct { void **vtable; u32 refs; } Qfs;

EXPORT u32 __thiscall qfs_query_interface(Qfs *q, u32 iid, void **out) { *out = q; return 1; }
EXPORT u32 __thiscall qfs_add_ref(Qfs *q) { return ++q->refs; }
EXPORT u32 __thiscall qfs_release(Qfs *q) { return --q->refs; }

EXPORT u32 __thiscall qfs_max_length(Qfs *q, u32 n) { return n + n / 112 + 16; }

EXPORT u32 __thiscall qfs_length(Qfs *q, const u8 *s) {
    u32 width = (s[0] & 0x80) ? 4 : 3, at = 2 + ((s[0] & 1) ? width : 0), n = 0;
    for (u32 i = 0; i < width; i++) n = n << 8 | s[at + i];
    return n;
}

EXPORT u32 __thiscall qfs_compress(Qfs *q, const u8 *src, u32 n, u8 *dst, u32 *out_len) {
    u32 o = 0, i = 0;
    dst[o++] = 0x10; dst[o++] = 0xFB;
    dst[o++] = (u8)(n >> 16); dst[o++] = (u8)(n >> 8); dst[o++] = (u8)n;
    while (n - i >= 4) {
        u32 k = (n - i) / 4 * 4;
        if (k > 112) k = 112;
        dst[o++] = (u8)(0xE0 | (k - 4) >> 2);
        for (u32 j = 0; j < k; j++) dst[o++] = src[i++];
    }
    dst[o++] = (u8)(0xFC | (n - i));
    while (i < n) dst[o++] = src[i++];
    *out_len = o;
    return 1;
}

EXPORT u32 __thiscall qfs_decompress(Qfs *q, const u8 *s, u32 len, u8 *dst, u32 *out_len) {
    u32 width = (s[0] & 0x80) ? 4 : 3, at = 2 + ((s[0] & 1) ? width : 0) + width;
    u32 cap = *out_len, o = 0;
    if (len < 5 || s[1] != 0xFB) return 0;
    for (;;) {
        u32 b0, lit, copy = 0, off = 0;
        if (at >= len) return 0;
        b0 = s[at];
        if (b0 < 0x80) {
            lit = b0 & 3; copy = ((b0 & 0x1C) >> 2) + 3; off = ((b0 & 0x60) << 3) + s[at + 1] + 1;
            at += 2;
        } else if (b0 < 0xC0) {
            lit = s[at + 1] >> 6; copy = (b0 & 0x3F) + 4;
            off = ((s[at + 1] & 0x3F) << 8) + s[at + 2] + 1;
            at += 3;
        } else if (b0 < 0xE0) {
            lit = b0 & 3; copy = ((b0 & 0x0C) << 6) + s[at + 3] + 5;
#ifdef BROKEN
            off = ((b0 & 0x10) << 12) + (s[at + 1] << 8) + s[at + 2];   /* a deliberate bug */
#else
            off = ((b0 & 0x10) << 12) + (s[at + 1] << 8) + s[at + 2] + 1;
#endif
            at += 4;
        } else {
            lit = b0 < 0xFC ? ((b0 & 0x1F) << 2) + 4 : b0 & 3;
            at += 1;
        }
        if (o + lit + copy > cap || at + lit > len) return 0;
        for (u32 j = 0; j < lit; j++) dst[o++] = s[at++];
        if (off > o) return 0;
        for (u32 j = 0; j < copy; j++, o++) dst[o] = dst[o - off];
        if (b0 >= 0xFC) break;
    }
    *out_len = o;
    return o == cap;
}

__declspec(dllexport) void *qfs_vtable[] = {
    (void *)qfs_query_interface, (void *)qfs_add_ref, (void *)qfs_release, (void *)qfs_compress,
    (void *)qfs_decompress, (void *)qfs_max_length, (void *)qfs_length,
};
