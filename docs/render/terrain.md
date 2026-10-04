# Terrain rendering

Code: `crates/sc3k-render` (`light.rs`, `palette.rs`, `terrain.rs`). See "In the remake" at the
end for what is ported so far.

How the city view draws the ground. Each cell of the map is a "dirt clod" (`cSC3DirtClod*`), a
sprite the dirt bag creates on demand. The clod draws itself as one or two Gouraud-shaded
polygons straight into the 16-bit view buffer. There are no terrain bitmaps; all colour comes
from small palette tables.

Addresses are Ghidra addresses in the Loki build. Ghidra loads these libraries at 0x10000, so
subtract 0x10000 for the ELF virtual address that `nm` and `objdump` print. Windows addresses
are given where they were checked. The Windows SIMDIRT.DLL has the same constants:
- the light vectors at 0x10020400 and 0x1002040C;
- the altitude scale 3.266 at 0x100203F0;
- the same light-index calculation at 0x100071A0–0x10007376 and 0x10016852.

## Zoom and cell size
`cSC3DirtClodFactory::cSC3DirtClodFactory` (libSimDirt 0x4BFBC) and
`cSC3CitySpriteCellMap::SetZoom` (libSimSpr 0x847E0) set four values for each zoom `z`:

| Zoom | Cell width `8 << z` | Half width | Cell height `4 << z` | Altitude step `1 << z` |
|---|---|---|---|---|
| 0 | 8 | 4 | 4 | 1 |
| 1 | 16 | 8 | 8 | 2 |
| 2 | 32 | 16 | 16 | 4 |
| 3 | 64 | 32 | 32 | 8 |
| 4 | 128 | 64 | 64 | 16 |

There are **5 zoom levels**, 0 to 4, not 3. `cSC3CitySpriteCellMap::ZoomIn`/`ZoomOut`
(libSimSpr 0x94154, 0x941BC) step between 0 and 4, and 4 is the closest view.
`cSC3CityViewIso::ZoomIn` (0xB1238) also stops at 4. At zooms above 3, `SetZoom` rounds the
view origin down to an even pixel.

One altitude unit is `1 << z` screen pixels. `SetZoom` also stores log2 of these sizes and uses
them as shifts (fields `+0x3C`–`+0x50`).

## Rotation
`cSC3CitySpriteCellMap::ActualGridToDrawGrid` (libSimSpr 0x99990) maps a cell (x, y) of a map
`X × Y` cells to the draw grid:

| Rotation | Draw cell |
|---|---|
| 0 | (x, y) |
| 1 | (Y − 1 − y, x) |
| 2 | (X − 1 − x, Y − 1 − y) |
| 3 | (y, X − 1 − x) |

The clods follow the same rule for vertices. They store each corner in slot
`(k − rotation) & 3` (see the corner order below), which is the same as rotating the vertex grid.
Vertex (i, j) of a map with `S` cells per side goes to (S − j, i) for rotation 1, and likewise for
the others.

## Projection
`ActualGridToCityPixel` (libSimSpr 0x997EC) and `DrawGridToCityPixel` (0x995C4) place draw cell
(dx, dy):
```
px = (dy − dx) · halfWidth + cellWidth / 2
py = (dy + dx) · cellHeight / 2 − altitude · altitudeStep
```
`altitude` there is a per-cell value in the sprite cell map (byte `+0x0B` of its 20-byte cell
record). For the terrain, the same projection applied to vertices gives the clod corners.
Vertex (i, j) in draw-grid coordinates with altitude `a` lands at:
```
sx = (j − i) · halfWidth
sy = (i + j) · cellHeight / 2 − a · altitudeStep
```
The draw-grid x axis therefore runs down-left on screen and y runs down-right. A cell's corners
on screen are:

| Slot | Corner | Vertex (rotation 0) | Position |
|---|---|---|---|
| 0 | left | (x + 1, y) | (0, ·) |
| 1 | top | (x, y) | (halfWidth, · − cellHeight/2) |
| 2 | right | (x, y + 1) | (cellWidth, ·) |
| 3 | bottom | (x + 1, y + 1) | (halfWidth, · + cellHeight/2) |

`·` is `−altitude · altitudeStep` of that corner, relative to the clod.
`cSC3DirtClodLand::pregetPoints` (0x48EA8) fills slot `(5 − r) & 3` from vertex (x, y),
`(6 − r) & 3` from (x, y+1), `(7 − r) & 3` from (x+1, y+1) and `(8 − r) & 3` from (x+1, y),
for rotation `r`.

`SetCityBaseRectAndCornerCoordinates` (0x8BC88) then shifts the whole city by
(halfWidth − halfWidth · X, −cellHeight/2), so that the map's left corner sits at x = 0.

The draw order of the clods is not traced. Clods overlap only on steep slopes, so drawing in
draw-grid rows of increasing `dx + dy` (back to front) is the working assumption.

## Which clod
`cSC3DirtBag::GetDirtClod` (libSimDirt 0x40AE4) picks the kind from
`CellWaterVertCount(x, y)` (0x35C80), the number of the cell's 4 corners with dirt altitude
≤ water altitude:

| Count | Clod |
|---|---|
| 0, or the caller forces land | `cSC3DirtClodLand` |
| 4 | `cSC3DirtClodWater` |
| 1–3 | `cSC3DirtClodShore` |

The map border also gets `cSC3DirtClodEdge` skirts (below).

`IsWater(x, y)` (0x359A0) is true when any corner is at or below the water. The remake's
`Terrain::is_water` tests the same thing per vertex.

The sprite manager passes a palette with the request. `SetViewID` (libSimSpr 0xA6B40) sets it to
4 for the normal view and 5 for the underground view (field `+0x74`). The underground view also
forces land clods (field `+0xD9`).

## Polygon shape (types)
`pregetPoints` turns the four corners into screen points, then the classifier at 0x46200 gives
the clod a type from 0 to 54:
1. If the top point is not above the bottom point after the ±cellHeight/2 offsets, the type is 0
   and nothing is drawn.
2. Otherwise compare the left point (`l`) and the right point (`r`) with the top and bottom
   points: 0 above top, 1 level with top, 2 between, 3 level with bottom, 4 below bottom.
3. The type is `l · 5 + r + 1`. Type 1 with the right point higher becomes 26. Type 25 with the
   left point higher becomes 27.
4. The "fold" flag adds 27. The flag is set when the top and bottom corners have the same
   altitude but the left and right corners differ.
5. All four points are moved so the anchor point of the type has y = 0.

The type table (19-byte rows, .data 0x63060) lists for each type:
- the number of pieces (1 or 2);
- for each piece, its left and right edge chains of corner indices, scanned top to bottom;
- the anchor point (byte 17);
- the lowest point (byte 18), whose y is the clod height (`GetH`).

The rows follow one rule:
- **One piece**, the whole quad, unless both side points are at or above the top point
  (`l, r ∈ {0, 1}`) or both are at or below the bottom point (`l, r ∈ {3, 4}`).
- **Two pieces** otherwise, and always when the fold flag is set: the triangles
  (left, top, bottom) and (top, right, bottom). The split is always along the top–bottom
  diagonal, never the left–right one.

`GetW` is the cell width; for edge clods it is the half width.

## Rasterizer (0x463D0)
Scanline fill of each piece:
1. The left and right chains are walked top to bottom.
2. Per scanline, the R, G and B of the two edges are interpolated linearly (in `float`) between
   the corner colours.
3. Each pixel is packed with the buffer's own 16-bit packer (`cIGZBuffer` vtable `0x1A8`).

Per-pixel additions:
- **Bump noise.** The clod passes a 1024-byte table of offsets, added to the colour bytes
  without clamping, so they wrap.
  - The start index is `(x · 0x6B9 + y · 0x757) & 0x3FF` for cell (x, y), plus `row · 0x8F`
    for each scanline.
  - Land uses three successive entries for R, G and B. Water uses one entry for all three.
- **Grid** (when on): a darkening of −16 (−8 at zooms 0 and 1). It applies to the first pixels
  of the left edges and to the scanline through the top point.

### Bump maps (`GenerateBumpMaps`, 0x48814)
Built once, from a `cRZRandom` seeded with `0xFFFFFFFF` (the clock), so they differ every run:
- **Land:** each byte is `(r mod 3) · 8 − 8`, one of −8, 0 or +8.
- **Water:** runs of 1–8 bytes of +8 or −9 (`0xF7`), each followed by 1–8 zero bytes.

## Vertex colour
Each corner's colour is cached per vertex in the factory (`+0x2C`, 0x101 entries per column)
and comes from a colour table: `GetColorInNative16Bit(palette, row, light)` (0x4EC6C).

### Land (`cSC3DirtClodLand::getColor`, 0x49060)
```
row   = 0x100 − (dirt altitude − water altitude)    (u16)
light = vertex light >> 1
```
- The row is 256 at the water line, smaller above it and larger below it.
- Palette 4 is the landscape's 32 × 512 palette (below).
- The underground view uses palette 5. There, a cell whose corners differ by more than 2 in one
  direction gets a 50% mix with `palbasic` entry `0x34`.

### Water (`cSC3DirtClodWater::getColor`, 0x498E8)
The clod surface is the water altitude (dirt bag `+0x38`) at every corner.
```
row = max(water altitude − dirt altitude, 0)        (the depth)
palette 6 (palwater.bmp, 1 × 256), light 0
```
If a city layer (city vtable `0x174`, then `0x70`) flags cell (x − 1, y − 1), the colour is mixed
50% with `palbasic` entry `0x33`. This is probably water pollution; not traced.

### Shore (`cSC3DirtClodShore`, 0x4A020, 0x4A318, 0x4A5B8)
- **Corners:** each corner sits at `max(dirt, water)`.
  - Corners above water use the land colour.
  - Corners at or below water use a marker grey (`0xA0, 0xA0, 0xA0`), hazed like any colour.
- **Fold flag:** set when the top and bottom corners are equally wet and the left and right
  corners are not. The altitudes play no part.
- **Rasterizer** (0x47088): the same scanline fill as 0x463D0, with a different pixel step.
  1. Interpolate the colour, round it, and read 3 land-noise bytes (the index advances by 3).
  2. Compute `d`, the squared RGB distance from the marker grey.
  3. Choose the pixel:

     | `d` | Pixel |
     |---|---|
     | ≥ `0x400` | the colour plus land noise |
     | `0x100` < `d` < `0x400` | `round((water · (0x400 − d) + colour · (d − 0x100)) / 0x300)` plus land noise |
     | ≤ `0x100` | the water colour plus one water-noise byte (at the advanced index) |

  - The water colour is `palwater` row 0 (depth 0), hazed. It is mixed 50% with the pollution
    tint, as for water.
  - If the second threshold is not above the first, the rasterizer falls back to 0x463D0.

The waterline therefore runs through the cell where the blend from land colour to grey crosses
the thresholds, with a short blend band in front of it.

### Zoomed-out haze
At zooms 0–2 every colour channel `c` becomes:
```
c − T[c] · (3 − z) + H[z]
```
- `T[c] = floor(c / 10 + 0.5)`. The table is at .data 0x634A0; the formula was checked against
  all 256 entries.
- `H` (.data 0x63480) is (0x31, 0x33, 0x35) for zoom 0, (0x21, 0x22, 0x23) for zoom 1 and
  (0x10, 0x11, 0x12) for zoom 2.

The colours fade 10% per zoom step towards a blue-grey. Moving between zooms below and above 3
clears the colour cache.

## Palettes
`buildColorLightTables` (0x4DB24) builds 9 tables through `RegenPalette` (0x4C9C4). Each table
has rows of 16-bit colours, one entry per light level.

| Palette | Source | Use |
|---|---|---|
| 0 | `palbasic.bmp` (1 × 53), lit by `Init_FromPaletteFile` | tints, highlights |
| 1–3 | `data1pal.bmp` … `data3pal.bmp` (64 × 32) | data views |
| 4 | landscape palette from the resource system (`SetAltitudePalette`, 0x4ED94) | land |
| 5 | `palunder.bmp` (32 × 512) | underground view |
| 6 | `palwater.bmp` (1 × 256) | water, by depth |
| 7, 8 | `paledgel.bmp`, `paledged.bmp` (1 × 512) | light and dark map-edge skirts |

- The files are under `Apps/Res/Dirt/PALETTE`. `Init_FromFile` (0x441E4) loads them as images.
- `Init_FromBuffer` (0x44DB8) copies pixel (x, y) to row y, entry x. Image row 0 is the top row.
- **Palette 4** is an image of type `62B9DA24` from `455A72B1_LandPalettes.IXF`: group
  `0x455A72B1`, instances 1–5, each a 32 × 512 BMP.
  - The `LandScapes` scheme entry names it in its second group/instance pair (see
    `docs/ui/new-city.md`).
  - `PALLAND.BMP`, `ArcticLand.bmp`, `DesertLand.bmp` and `CaribbeanLand.bmp` in the same folder
    look like loose copies.
- In the land palettes, entry 0 of a row is the darkest and entry 31 the brightest.
- **Palette rows** run from snow and rock at the top, through green just above row 256 (the water
  line), to grey-blue seabed below it.

`Init_FromPalette` (0x45B74) builds the light ramp of a 1-pixel-wide palette. Entry `i` is the
colour mixed with black by `clamp(cos((31 − i) · step) · 0.35, 0, 1)`.

## Vertex light
The light map is `cSC3DirtBag` field `+0x34`, one byte per vertex.
`calculateAndSetVertexLight` (0x4142C) fills it after `ReflectMap` in `cSC3DirtBag::Init`
(0x325C4) and again around any edited vertex (±3). For each vertex:
1. **Neighbours.** `getVertAltForLightCalc` (0x4156C) reads the dirt altitudes of the vertex and
   of its 4 neighbours. A neighbour outside the map is extrapolated as `2 · centre − opposite`,
   clamped to 0..255.
2. **Normal.** `getVertexNormal` (0x363C4) builds 4 edge vectors in (x, up, y) coordinates:
   - (0, Δ(x, y+1), 16), (16, Δ(x+1, y), 0), (0, Δ(x, y−1), −16) and (−16, Δ(x−1, y), 0);
   - each Δ is the altitude difference × `AltitudeScale`;
   - the normal is the average of the cross products of neighbouring pairs.
   - 16 is `cSC3City::CellSizeInWorldUnitsX/Z` (libSimCity 0x30ACC, 0x30AEC).
   - `AltitudeScale` (0x4124C) is `cSC3City::CellSizeInWorldUnitsY`, 3.266 (libSimCity 0x30B0C).
3. **Light index.** `getVertexLightIndex` (0x41800) runs twice, once for each light vector
   (rodata 0x61698 and 0x616A4):
   - the light vectors are (1, −1, 0) and (−1, −1, 0);
   - `d = max(dot(unit normal, −unit light), 0)`;
   - `index = trunc(acos(d) / step)`, where `step = (π · 0.5) / 31`
     (`cSC3DirtClodColorLightTable::GetLightStep`, 0x5ECD4; `mkLightIndexMax` = 31).
4. The stored value is the **larger** of the two indices.

The land clod uses `light >> 1`, so only palette entries 0–15 are reached.

**Check during rendering.** Taken literally, a flat vertex gets index 15 (entry 7). A 45° slope
facing either x direction gets 31 (entry 15, brighter), and slopes along y fall in between.
Flat ground would then be the darkest, with both sides of an x ridge equally lit. The Windows
code does the same comparison (`cmp al, bl; jb` at 0x10007368). This should be compared with the
original's screen before relying on it.

## Map edge skirts (`cSC3DirtClodEdge`)
Built by `GetDirtClodEdge` (factory 0x4C8E8). Flags byte at `+8`:
- `0x80` picks which border side of the cell the skirt is on;
- `0x40` means water is ignored (the underground view);
- `0x04` is always set.

Each skirt is half a cell wide and hangs under one border side, between its two vertices A and B.
`pregetPoints` (0x4BBB4) and `Draw` (0x4B360) give:
- the top at `max(dirt, water)` and a middle line at the dirt;
- the bottom at altitude 0, plus 4 altitude steps unless water is ignored.

Draw fills two quads:

| Quad | From → to | Index along the quad | Noise |
|---|---|---|---|
| Water | water surface → dirt; only where water is above dirt | `0` at the top → `water − dirt` at the dirt | none |
| Soil | dirt → bottom | `0x100` (`0x104` when water is ignored) → `0x104 + dirt`, capped at `0x1FF` | water bump map |

- The index is a row of a 512-entry ramp from palette 7 (`paledgel`) or 8 (`paledged`). Rows
  0–255 are water blues; rows 256 and up are soil strata.
- Both ramps are hazed at zooms 0–2.
- The ramp is picked by `(rotation if flag 0x80 else ~rotation) & 1`. In rotation 0 the
  draw-grid row `i = size` (left front side) uses the light ramp, and the column `j = size`
  (right front side) the dark one.
- Draw hands the quads to a third rasterizer at 0x48238. Ghidra does not mark it as a function,
  so it is not decompiled.

## Flora
Trees are occupants, placed when the simulation begins (`docs/sim/flora.md`) and drawn with the
sprites of the chosen flora set (`docs/render/flora.md`).

## Background (`BACK0.BMP` .. `BACK4.BMP`)
- **Which file:** `cSC3CityViewIso::SetBackgroundBitmapForZoom(zoom)` (libSimSpr Ghidra
  0xAFC8C) takes the string `res/sprites/backn.bmp` (rodata 0x14B2EC) and replaces the `n`, the
  fifth character from the end, with `'0' + zoom`.
  - It loads that file as a 16-bit buffer (colour type 4) and hands it to the sprite cell map
    with `SetBackgroundBuffer` (0x94000).
  - BACK0–BACK3 are 128×64; BACK4 is 256×128. All are 24-bit BMPs with an isometric grid.
- **Drawing:** `cSC3CitySpriteCellMap::DrawBackground` (0x8BF7C) tiles the buffer over the
  dirty rectangle.
  - The first tile starts at `−((rect.x − ox) mod w)`, `−((rect.y − oy) mod h)`.
  - (ox, oy) comes from cell-map vtable `0x130`. It is taken to be the view origin, so the
    texture scrolls with the map.
  - Without a background buffer the rectangle is filled with a flat colour.

## Camera (`cSC3CityViewIso`, `cSC3WinCityView`)
- **Focus cell:** the view keeps the map cell under the middle of the screen (+0x98/+0x9C).
- **Translate(dx, dy)** (Ghidra 0xB071C):
  - It picks the cell at the screen middle plus (dx, dy) with
    `WindowPixelToActualGrid`, trying once more with its flag set if that fails.
  - If a cell is found, it moves the cell map by (dx, dy) pixels and makes that cell the
    focus. Otherwise nothing moves, so the middle of the screen never leaves the map.
- **Zoom:**
  - `ZoomIn` (0xB1238) steps up to 4.
  - `ZoomOut` (0xB1320) steps down to 0.
  - Both change the background bitmap first, then rezoom the cell map about the focus cell.
- **Rotation:**
  - `RotateCameraCW` (0xB1408) sets the rotation to `rotation == 0 ? 3 : rotation − 1` and
    rotates about the focus cell.
  - `RotateCameraCCW` steps the other way.
- **Scroll flags:** `cSC3WinCityView` has four flags: +0x1D1 up, +0x1D2 down, +0x1D3 right and
  +0x1D4 left.
  - **Keys:** `DefOnKeyDown` (0x1260D4) sets them from SDL keys 0x111–0x114 (up, down, right,
    left).
  - **Edges:** `UpdateScroll` (0x1034D0) sets them from the pointer, using the regions of
    `updateScrollRegions` (0x1258D8).
    - The edge bands are 64 pixels at the left and right and 48 at the top and bottom.
    - Inside the inner rectangle all four flags clear.
- **Scroll step:** `paintScroll` (0x10392C) runs on every `GZPaint`.
  - One flag moves `Translate(±s, 0)` or `(0, ±s)`.
  - Two adjacent flags move diagonally, `(±s, ±s)`.
  - Opposite flags, or three or four flags, do not move.
- **Scroll speed:** s is the speed for the current zoom (+0x1B8, copied from +0x1BC..+0x1CC in
  `ZoomIn`). `Init` and `setTweakableSettings` set all five to 32.0.
- **Mouse scroll:** `SetMouseScroll`/`UpdateScroll` also have a drag mode.
  - The speed is half the pointer's distance from the anchor.
  - It has a dead zone of 12 and is capped at 80.
  - It is not ported.
- **Not found:** the zoom and rotate hotkeys.

## cSC3DirtBag fields used here
| Offset | Field |
|---|---|
| `+0x30` | dirt altitude per vertex (u8, starts at 0x7F) |
| `+0x34` | light index per vertex |
| `+0x38` | water altitude per vertex |
| `+0x44` | flora during init |
| `+0x48` | salt-water bits |

`ReflectMap`'s calls map to these (Windows slot, Loki name):

| Windows slot | Loki name | Field |
|---|---|---|
| `0x64` | `SetGlobalSeaLevel` | |
| `0x84` | `SetVertexAltitude(x, y, a)` | `+0x30` |
| `0x8C` | `SetWaterTable(x, y, level, salt)` | `+0x38` and the salt bits |
| `0x90` | `InitFloraLevel` | |

The Windows slot numbers differ from Loki's because MSVC orders overloads differently.

## In the remake
- **Clod kinds:** `TerrainScene` draws land, water and shore clods by the corner-count rule.
  - Each kind has its own pixel step (`Mode` in `terrain.rs`), following 0x463D0 and 0x47088
    above.
  - The pollution tint is left out; there are no city layers yet.
- **Edge skirts:** drawn under the two front sides after all cells.
  - The undecompiled rasterizer 0x48238 is modelled as the same scanline fill, interpolating
    the ramp row instead of RGB.
  - The soil part gets one water-noise byte per pixel.
- **View:** `Camera` in `camera.rs`.
  - It starts on the map's middle cell at zoom 4 and rotation 0; `--zoom` and `--rotate` change
    these.
  - Scrolling follows "Camera" above.
  - The game assumes 30 paints a second, so it scrolls 32 pixels every 33 ms.
  - Zoom and rotation re-centre on the focus cell at the cell's mean surface altitude.
  - Picking intersects the sea-level plane, then the surface of the cell found. The real
    `WindowPixelToActualGrid` is not ported.
  - The zoom and rotate keys are provisional: PageUp/PageDown, `+`/`-` and the wheel zoom; `,`
    and `.` rotate.
  - `sc3k-dump iso` draws a whole map.
- **Background:** `BACK<zoom>.BMP`, tiled from the view origin, under the terrain.
- **Land palette:** chosen by the dialog's `LandScapes` key through `SC3CityScheme.ini`.
- **Light:** follows "Vertex light" literally, including the open question about flat ground.
- **Bump noise:** both maps come from the terrain seed instead of the clock.
- **Rasterizer:** a generic scanline fill of each piece:
  - it intersects the piece's edges with each row instead of walking the original's left and
    right chains;
  - rows run from the top corner to the bottom corner, bottom excluded;
  - spans run from the rounded left edge to the rounded right edge, right excluded;
  - colours step by `(right − left) / (width + 1)`, as in the original.
  - Pixels can differ where the original starts a chain on a horizontal edge.
- **Output:** every pixel is reduced to RGB565, as the 16-bit back buffer would hold it.
- **Draw order:** back to front by draw-grid diagonal, then the skirts.
- **Not checked:** a side-by-side comparison with the original under wine.
