# Terraforming (cSC3DirtBag, cSC3ConvexContour)

Code: `crates/sc3k-sim/src/terraform.rs`. Checked by `tools/diffcheck/run.py ground`
(`docs/re-notes/diffcheck.md`).

| Function | Windows | Loki (Ghidra) |
|---|---|---|
| `CanRaiseTerrain(bounds, out, cost)` | 0x10009CD0 | 0x3870C |
| `CanLowerTerrain(bounds, out, cost)` | 0x1000A210 | 0x38D2C |
| `CanLevelTerrain(bounds, level, out, cost)` | 0x1000A750 | 0x3934C |
| `GetOptimalLevel(bounds, &level, out, cost)` | 0x1000AC50 | 0x398DC |
| `RaiseTerrain(bounds, out, cost, free)` | 0x1000BC90 | 0x3AB28 |
| `LowerTerrain(bounds, out, cost, free)` | 0x1000C130 | 0x3B3F8 |
| `LevelTerrain(bounds, level, out, cost, free)` | 0x1000C5D0 | 0x3BCC8 |
| `FunctionFilterTestPassed` | 0x10009C40 | 0x41D88 |
| `ResetChanges` | 0x1000DED0 | 0x41DDC |
| `MarkVertexChanged` | 0x1000DF40 | 0x3E3B0 |
| `MarkCellChanged` | 0x1000DF10 | 0x41E28 |
| `ChangedByLastOperation` | 0x1000DFE0 | 0x41E58 |
| `SetVertexAltitude(x, z, a)` | 0x10005DF0 | 0x34EB0 |
| `SetWaterTable(x1, z1, x2, z2, w)` | 0x100060D0 | 0x352D0 |
| `notifyCellUpdate` | 0x10006370 | 0x35680 |
| contour `addPri` | 0x100097A0 | 0x41C64 |
| contour `GrowAndFix` | 0x100076E0 | 0x3658C |
| contour `Bounds` | 0x10007C90 | 0x36B20 |
| contour `addSec` | 0x10009830 | 0x381F4 |
| contour `addConnection` | 0x10009AC0 | 0x3843C |
| contour `needsAdjustment` | 0x10009920 | 0x382EC |
| contour `outOfBounds` | inlined into `GrowAndFix` | 0x41D1C |

Windows vtable slots are the Loki ones minus 8, except overloads: `SetVertexAltitude(x, z, a)`
is 0x84 and the rectangle one 0x80; `SetWaterTable` with a rectangle is 0x88.

## Bounds and costs
- A `cSC3CityBounds` is six i32: x1, z1, y1, x2, z2, y2, in 8.8 fixed point. The operations
  take the cell part (`>> 8`) and edit the vertices from (x1, z1) to (x2, z2), both included.
- The costs come from the city's simulator (`cISC3City` slot 0x15C, its `GetValue` at 0x58):
  value 9 per step raised, 10 per step lowered, 8 per step levelled. Each step of the
  contour adds `(v10·down + v9·up) >> 2`; the rectangle's own vertices add `v·n >> 2` once.
  All in u32, wrapping.
- With `free` false the edits spend the cost (simulator slot 0x14).

## The operation
1. `ResetChanges`, updates off on the change sender.
2. The rectangle's edge goes into the contour clockwise from (x1, z1): top without its last
   vertex, right, bottom backwards, left backwards. Each edge vertex gets its target: one
   step up (not past 255), down (not below 0), or the level. The estimates fail here when two
   edge vertices in a row are not neighbours or differ by more than 4.
3. `GrowAndFix` repeats while it succeeds and some new vertex had to move. Each round builds
   the next ring just outside the current one; a new vertex is brought within 4 of the ring
   vertices next to it. The edits set the moved vertices after each round; the estimates
   only count them.
4. `Bounds` of the last ring, grown by one towards 0 and clamped to the map, is the
   reported area.
5. The rectangle's inner vertices get their targets.
6. The edits only: the water table inside the reported area goes to the sea level, the light
   is recomputed there (3 vertices wider), updates come back on and each changed cell posts a
   message.
7. The estimates return `FunctionFilterTestPassed`: true when no filter is installed.

`GetOptimalLevel` is the rounded mean of the edge's altitudes, `(n/2 + sum) / n`, followed by
`CanLevelTerrain` to it.

## The contour (GrowAndFix)
Two static rings of up to 1024 vertices, each six bytes: x and z (i16), the altitude, and a
keep flag. Edge vertices are kept; vertices the contour adds are not.

For each vertex of the current ring, the direction from the previous vertex and from the next
one are numbered 0 to 7. The difference of the two (mod 8) is the turn, and it decides how
many neighbours to try: 6 for 0, 2 for 4, 4 for 6 (stepping one at a time), none otherwise,
8 when the ring is one vertex. A turn of 2 keeps the vertex itself. Each neighbour that is on
the map and needs to move (or follows one that did) joins the next ring; otherwise the ring
keeps the current vertex there. `addConnection` inserts a vertex when the next ring would
otherwise skip a step.

The direction numbering has a quirk in both builds: a step towards lower z with lower x is 5,
the same as a straight step towards lower z, where 4 would follow the pattern. No ring of the
check's 2000 operations has that step, so it may never matter.

`needsAdjustment` takes the highest and lowest altitude among the current vertex, the next
ring's last vertex, and the current ring's vertices i − 3 to i + 2 that touch the new vertex.
The index wraps as unsigned, so near the start of a ring whose length is not a power of two
it does not wrap to the end. The new vertex moves up to `highest − 4` or down to `lowest + 4`.

Once the next ring holds more than 1020 vertices, `GrowAndFix` gives up: the operation ends
with the rings as they were.

## Quirks
- A single-vertex rectangle has no edge and so no contour; `Bounds` of an empty ring is
  0, 0, 0, 0. The edit sets the vertex but relights only around 0, 0, which leaves its
  light stale.
- The change bits are one per cell (+0x40: vtable, X, Z, words per x, rows of words).
  `MarkVertexChanged` marks the up to four cells around a vertex; reading an altitude that
  differs from the target marks it too, even in an estimate.
