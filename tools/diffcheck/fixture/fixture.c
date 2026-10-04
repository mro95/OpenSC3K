/* A stand-in for SIMDIRT.DLL used by tools/diffcheck/selftest.py: cRZRandom written from
 * docs/sim/random.md, plus probes of the harness itself (imports, x87 results, fs:[0]).
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
