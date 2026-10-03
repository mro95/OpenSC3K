# Terrain generator (cSC3DirtGenerator)

Code: `crates/sc3k-sim/src/dirt.rs`. Preview: `sc3k-dump terrain <seed> <size> <out.png> [difficulty]`.

The port follows the Windows SIMDIRT.DLL, because its `cRZRandom` differs from the Loki one
(`docs/sim/random.md`). Most helpers are unnamed in the Windows build. They were matched to the
Loki `libSimDirt.so` names by call order in `GenerateRandom` and by structure:

| Function | Windows | Loki |
|---|---|---|
| `SetDifficulty(int)` | 0x10017375 | 0x51FAC |
| `Init(x, y)` | 0x1001742C | 0x4F3A4 |
| `ReflectMap(cISC3DirtBag*)` | 0x10017683 | 0x4F8D0 |
| `AltitudeSubdivision` | 0x100177B7 | 0x4FAAC |
| `ChangeTerrainProfile` | 0x10017AF6 | 0x50070 |
| `GenerateRandom(seed, hills, water, trees, flags)` | 0x10017C2D | 0x5024C |
| `FindSaltWater` | 0x100181D6 | 0x5101C |
| `CreateFlora` | 0x10018427 | 0x513FC |
| `FixShores` | 0x100185C8 | 0x51674 |
| `CreateRivers` | 0x100187DA | 0x51A7C |
| `traceDepth` | 0x10018B57 | 0x52234 |
| `Bezier2D4Controls<cRZPoint>` | 0x10018C30 | |
| blend of 2 values | 0x10017A09 | |
| blend of 4 values | 0x10017A69 | |

Windows vtable slots, with no 2-slot header: 0x0C `Init`, 0x10 `Shutdown`, 0x14 `SetDifficulty`,
0x18 `IsReady`, 0x1C `GenerateRandom`.

## Object layout (Windows)
| Offset | Field |
|---|---|
| `+0x04`, `+0x08` | vertex counts X, Y: the map size + 1 |
| `+0x10` | sea level (u8) |
| `+0x14` | altitude per vertex, `cRZCellMap<u8>` |
| `+0x18` | water surface per vertex, `cRZCellMap<u8>` |
| `+0x1C` | flora per vertex, `cRZCellMap<u8>` |
| `+0x20` | salt-water flag per vertex, a bit map (20 bytes: vtable, X, Y, words per column, columns) |
| `+0x24` | a byte written from the high byte of the second argument; not read by the generator |
| `+0x28` | difficulty from `SetDifficulty` |

On Windows a `cRZCellMap<u8>` (0x10018BA6) is 16 bytes: vtable, X, Y, then column pointers at
`+0x0C`. `Init` builds the four maps at `size + 1` vertices per side and calls `Shutdown` first if
the generator is already ready.

## Arguments
The New City dialog calls `GenerateRandom(0xFFFFFFFF, 0x40, 0x40, 0x40, 0x24)` (see
`docs/sim/new-city.md`). The seed `0xFFFFFFFF` means "seed from the clock". The names below
describe what each argument does; the original names are unknown.

- `hills` sets the altitude bands (below). A larger value gives hillier land.
- `water` sets the sea level share, the river depth and how far salt water reaches.
- `trees` sets the number of tree clumps and their radius.
- `flags`:

  | Bit | Meaning |
  |---|---|
  | 0x01 | sea along y = 0 |
  | 0x02 | sea along x = max |
  | 0x04 | sea along y = max |
  | 0x08 | sea along x = 0 |
  | 0x10 | river from x = 0 to x = max |
  | 0x20 | river from y = 0 to y = max |
  | 0x40 | low centre point, and more water |
  | 0x80 | raised centre point (a mountain) |

The dialog's `0x24` gives one sea edge at y = max and a river running into it.

## GenerateRandom
Each step uses the shared RNG (SIMDIRT 0x10025BC0) in this order. The order matters for
reproducing a seed.

1. `Seed(seed)`. Fill the altitude, water and flora maps and the salt bits with 0.
2. **Control points.** The map is split into quarters, giving a 5 × 5 grid of points: point `k`
   is at x = `(k % 5) · qx`, y = `(k / 5) · qy`, where `qx` = (vertices X − 1) >> 2.
   - Each flag bit selects three 25-bit masks (tables at 0x10024858, 0x10024878, 0x10024898):

     | Bit | Low points | x midpoints | y midpoints |
     |---|---|---|---|
     | 0 | `0x1F` | `0x0F` | 0 |
     | 1 | `0x01084210` | 0 | `0x00084210` |
     | 2 | `0x01F00000` | `0x00F00000` | 0 |
     | 3 | `0x00108421` | 0 | `0x00008421` |
     | 6 | `0x00001000` | 0 | 0 |

   - Every point gets `blend(base, base, 16, 15)`. `base` is `0x60`, or `0x60 − size/4` for a
     low point.
   - With 0x80, the centre (2qx, 2qy) is then set to `blend(p, p, 16, 15)` with
     `p = (size/4)·2 + 0x60`.
   - For each x-midpoint bit, the vertex halfway to the next point in x becomes the average of
     the two points. The same applies in y. This keeps sea edges straight.
3. `AltitudeSubdivision(0, 0, X − 1, Y − 1, min(X − 1, Y − 1), 8)`.
4. **Sea level.** Count a histogram of all altitudes.
   - `k` = 1, plus 2 per sea-edge bit (0–3), plus 3 for 0x40.
   - The target count is `vertices × max(water, 4) × k / 0x600`. If `water` > 0x7F, it is
     instead `vertices × 9 / 10`. The minimum is 1.
   - The sea level is the lowest altitude at which the cumulative count reaches the target.
   - The water map is then filled with the sea level. Every vertex has the same water surface.
   - A running "lowest altitude − 1" is also kept here, but the target search overwrites it.
5. **Band tops.** `f = 1 − √(hills / 256 · 0.5)`; with 0x80 the divisor is 128 instead of 256.
   - The targets are `trunc(n·f)`, `trunc(n·f³)` and `trunc(n·f⁴)`, where `n` is the vertex
     count.
   - The original computes these in x87 extended precision with `_ftol`.
   - `l1` is the first altitude above the sea level at which the cumulative count (starting
     from 0) reaches the first target. `l2` and `l3` repeat this from the band below. A band
     that never fills gives 0xFF.
   - Loki uses `float` and rounds the first two targets instead of truncating.
6. `ChangeTerrainProfile(sea + 2, l1, l2, l3)`. Its return value is the "even" flag.
7. `FixShores(flags)`, `CreateRivers(flags, water)`, `FindSaltWater(flags, water)`,
   `CreateFlora(trees)`.
8. If "even" is set, every altitude `a > sea + 2` becomes `((a − sea − 2) & 0xFE) + sea + 2`.

### Blend (0x10017A09, 0x10017A69)
`blend(values…, range, jitter)`:
- If `range < 2`, return the first value.
- Otherwise `lo = max − jitter` and `hi = min + jitter`.
- If `lo < hi`, the result is `RandomSint32RangeGaussianFast(lo, hi + 1)`. Otherwise it is
  `(lo + hi) / 2`.
- The result is clamped to 0x10..0xF0.

The 2-value and 4-value versions differ only in how many values they take.

### AltitudeSubdivision(x0, y0, x1, y1, range, depth)
Midpoint displacement. An altitude of 0 means "not set yet".

1. `mx = (x0 + x1) >> 1`. If `|x1 − x0|` is odd and `uniform(2) ≠ 0`, `mx` += 1. `my` is
   computed the same way.
2. Fill each unset point with `jitter = range − 1`, in this order:
   - (x0, my) from the corners (x0, y0) and (x0, y1);
   - (x1, my) from (x1, y0) and (x1, y1);
   - (mx, y0) from (x0, y0) and (x1, y0);
   - (mx, y1) from (x0, y1) and (x1, y1);
   - (mx, my) as a 4-value blend of the four edge midpoints.
3. If `depth` > 0, recurse with `range >> 1` and `depth − 1` into these quarters, in order:
   (x0, y0, mx, my), (x0, my, mx, y1), (mx, y0, x1, my), (mx, my, x1, y1).
   - The original turns the last call into a loop.

### ChangeTerrainProfile(lo, l1, l2, l3)
The difficulty (`+0x28`: 1, 2 or 3, anything else counts as 1) picks a row of the table at
0x10024828:

| Difficulty | d1 | d2 | d3 | even |
|---|---|---|---|---|
| 1 easy | 12 | 6 | 3 | yes |
| 2 medium | 10 | 5 | 3 | no |
| 3 hard | 6 | 3 | 2 | no |

Set `t1 = (l1 − lo)/d1 + lo` and `t2 = (l2 − l1)/d2 + t1`, as bytes. Each altitude `a ≥ lo`
becomes:

| Range of `a` | New altitude |
|---|---|
| `a < l1` | `(a − lo)/d1 + lo` |
| `a < l2` | `(a − l1)/d2 + t1` |
| `a < l3` | `(a − l2)/d3 + t2` |
| otherwise | `a + ((l3 − l2)/d3 + t2 − l3)` |

All divisions are signed. Easy terrain is therefore the flattest.

### FixShores(flags)
For each sea edge, walk along the edge, starting with `run = 3`. At each edge vertex above the
sea level:
1. Draw `r = GaussianFast(0, 7)`. Move `run` one step towards `r`.
2. Walk `run` vertices inwards from the edge. The vertex `i` steps in (counting from 0) is
   capped at `sea − run + i + 3`.

The edges are handled in bit order 0, 1, 2, 3.

### CreateRivers(flags, water)
Only runs with 0x10 or 0x20.
1. `depth = trunc(√X · 0.5) · (water + 0x40) / 0x80`. If that is ≥ the sea level, use
   sea − 1. Both the start depth and the end depth start at `depth`.
2. With 0x20:
   - Draw the start x and end x with `RandomSint32RangeUniform(X/4, 3X/4)`, in that order.
   - Draw the control points c1 = (`range(X/8, 7X/8)`, `range(Y/8, 7Y/8)`) and c2 the same way.
   - The curve runs from (start x, 0) through c1 and c2 to (end x, Y − 1).
   - If sea bit 0 is set, the start depth becomes `max(sea − alt(start), depth)`. If sea bit 2
     is set, the end depth becomes `max(sea − alt(end), depth)`.
3. With 0x10, the same is done across x, from (0, ya) to (X − 1, yb), using sea bits 3 and 1.
   If both river bits are set, this second curve replaces the first.
4. `Bezier2D4Controls` (0x10018C30) produces 1024 points.
   - Point `i` is at `t = (i + 1)/1024`, where `t` is accumulated by adding `1/1024` each step.
   - The weights are `(1−t)³`, `3t − 6t² + 3t³`, `3t² − 3t³` and `t³`, truncated with `_ftol`.
5. For each point that differs from the previous one:
   - Compute `d = low byte of (((end − start) · i) >> 10, unsigned) + start`.
   - Then, for the directions (−1, 0), (1, 0), (0, −1), (0, 1), call
     `traceDepth(x, y, dx, dy, range(0, 3) + d)`.

### traceDepth(x, y, dx, dy, depth)
1. Start with `level = sea − depth` (a byte).
2. Repeat:
   - Lower the vertex to `level` if it is higher.
   - Step by (dx, dy).
   - Raise `level` by 2 while it is below the sea level, otherwise by 1.
3. Stop when the step leaves the map or `level` reaches 0xFC.

### FindSaltWater(flags, water)
1. Clear the salt bits.
2. For each sea edge and each line perpendicular to it, walk inwards from the edge. Mark up to
   `water · X >> 8` vertices as salt.
   - Each vertex where `water < altitude − 1` counts as land.
   - The walk stops after the fifth land vertex. Land vertices before that stop are marked too.

### CreateFlora(trees)
1. Compute:
   - `clumps = trees · X · Y >> 16`;
   - `rmax = trees >> 2`, or 2 if that is less than 3.
2. For each clump:
   1. Draw `cx = uniform(X)`, `cy = uniform(Y)` and `r = GaussianFast(0, rmax)`.
   2. The point count is `GaussianFast(0, r²)`, or 1 if `r ≤ 1`.
   3. For each point:
      - Draw `d = uniform(r)` (0 if `r ≤ 1`).
      - Draw `a = RandomDoubleRangeUniform(0, 6.2832)`. The constant at 0x10020C28 is 6.2832,
        not 2π.
      - The point is (`trunc(cos a · d) + cx`, `trunc(sin a · d) + cy`).
      - If the point is inside the map and its altitude is above its water level, set its flora
        to `range_min_of_two(0, n)`. `n` is 0x80 on salt vertices and 0x100 elsewhere.

### ReflectMap(cISC3DirtBag*)
Only runs if `IsReady`. It hands the result to the dirt bag through these vtable slots (Windows
offsets):

| Slot | Call |
|---|---|
| `0x64` | `SetSeaLevel(sea)` |
| `0x84` | `(x, y, altitude)`, for every vertex |
| `0x8C` | `(x, y, water level, salt bit)`, for every vertex |
| `0x90` | `(x, y, flora)`, for every vertex |

The remake returns these maps as `Terrain`. The dirt bag itself (altitude scale, how flora
becomes trees) is not ported yet.

## Precision
The original computes the band targets, the Bézier curve and the flora angles on the x87 stack
in extended precision. The remake uses `f64`. Results can differ only where a value lands within
rounding error of an integer before truncation.

The remake has not been compared with the original for a fixed seed, because the original seeds
from the clock.
