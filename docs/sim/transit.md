# Transport and utility networks

Roads, highways, rail, subway, power lines and pipes. All of them live in `SIMNTWRK.DLL`
(Loki libSimNtwrk). Nothing is ported yet. The format of the tiling rule files is in
`../formats/tiling.md`.

## Addresses and decompiling
- **libSimNtwrk:** Ghidra addresses are the `tools/loki/symbols/libSimNtwrk.so.tsv` address +
  0x10000.
- **Rule classes:** the `cTile*` classes live in the executable (`sc3u_demo.x86`), whose Ghidra
  addresses match its TSV.
- **Allocator splits:** Ghidra cuts many functions in two at the inlined STL node allocator. It
  makes a separate `FUN_` at the code after it, and turns the jump there into a call. To read
  such a function whole, decompile it in a read-only session after:
  1. removing the functions that start inside its range;
  2. clearing the flow overrides on its instructions;
  3. regrowing its body with `CreateFunctionCmd.fixupFunctionBody`.
- **PIC switches:** Ghidra does not resolve them. The tables below were read from the binary:
  - the GOT base is `ebx` = 0xB6FD4 in file addresses;
  - each target is `ebx − table[i]`.

## Network types
The `int` network argument used throughout:

| Value | Network | Occupant manager |
|---|---|---|
| 1 | road | surface (+0x3C) |
| 2 | rail | surface |
| 3 | power | surface |
| 4 | highway | surface |
| 5 | pipe | plumbing (+0x40) |
| 6 | subway | subway (+0x44) |
| 8 | on-ramp | surface |

### Static tables
For each network the layer keeps these static tables. They are filled in `StaticInit`, which is
not traced.
- **Rule families:** `mp<Net>Simple`, `mp<Net>Complex`, and `mp<Net>Eval` (the `final` file).
- **Tile sets:** `mp<Net>Set`, `mp<Net>CvrtSimple`, `mp<Net>CvrtComplex`, `mp<Net>Prot`.
- **Occupant tests:**
  - `mp<Net>Test`: the occupant belongs to this network.
  - `mpNot<Net>Test`.
  - `mp<Net>EditTest`: the occupant may be replaced. Road, rail and highway use it; power uses
    `mpPowrEditTest`; pipe and subway use `mpTreeTest` instead.
  - `mpNetwTest`: any network occupant.

## Layer fields used below

| Offset | Object | Calls |
|---|---|---|
| +0x2C | city geometry | vtable 0xD4/0xD8 cell counts X/Y |
| +0x30 | dirt bag | 0x50 and 0x54 vertex altitudes; 0x5C `IsWater` (as in `flora.md`); 0xB8/0xBC sizes |
| +0x34 | a city layer | vtable 0x40 `(x, y, 0)` is called on every cell a tile is inserted into. Probably clears the zone; not traced. |
| +0x38 | budget | 0x18 funds (`i64`), 0x1C spend, 0x60 price of an item. The road price is item 16000. |
| +0x3C / +0x40 / +0x44 | occupant managers | surface, plumbing, subway |
| +0x48 | edge connections | vtable 0x60 `(net, x, y)`: whether a cell on the map edge connects off the map. Probably neighbour cities; not traced. |
| +0x68 | debug flag | `EngineersDebug` |

Occupant-manager vtable slots used:

| Slot | Call |
|---|---|
| 0x44 | insert |
| 0x4C | remove |
| 0x6C | in bounds |
| 0x80 | occupant at coordinate |
| 0x84 | occupant at (x, y) |

Occupant vtable slots used:

| Slot | Call |
|---|---|
| 0x44 | test flag. Flag 0x400 is not identified. |
| 0x5C | occupant type. 5 is a tree (`emptyOrTree`). |
| 0x7C | may be removed |
| 0xAC | orientation; its +0x24 gives the rotation |
| 0xF4 | set position |

A `cSC3CityCoord` is `(x << 8, y << 8, altitude << 8)`. The cell is the coordinate `>> 8`.

## Placing a road, top down

```
cCTPlaceRoad::ExecuteOnMouseMove / ExecuteOnMouseDrag  → CanPlaceRoad: price only
cCTPlaceRoad::ExecuteLineDraw                          → cSLCityViewLineAlgorithm::DrawLine
cCTPlaceRoad::ExecuteOnMouseUp → PlaceRoad (vt 0x9C) → PlaceNetwork(sel, 1, …)
    → CanPlaceRoad (price)
    → PlaceNetworkEx, once per batch of up to 0x500 selected cells
        → bridges / FullDoPrepare / tunnels / flattening / edge connection
        → EditNetwork(cells, 1, bounds)
            → selectSloped + InsertNetwork          (slopes)
            → SolveNetwork mode 8   (SimpleRules)   until nothing changes
            → SolveNetwork mode 24  (ComplexRules)  up to 5 rounds
            → SolveNetwork mode 4   (final)         once
                → CreateNeighborhood → cTileEvaluator::Lookup → InsertNetwork
```

### The tool: `cCTPlaceRoad` (0x5E7E4 onwards)
- **`Execute` (0x5E7E4):**
  1. Fetches the transit layer, the surface occupant manager and the dirt bag from the city.
  2. Asks the layer for three occupant tests (`CreateOccupantTest`, IID `A1C085DB`, kinds 0x1C,
     0x10, 0x16).
  3. Builds a `cSLCityViewLineAlgorithm`:
     `Setup(0, occman, test1C, test10, test16, dirtbag, 0, 3, 1, 1, 1, price(16000))`.
- **`ExecuteOnMouseMove` / `ExecuteOnMouseDrag` (0x61940 / 0x6199C):** `CanPlaceRoad` stores the
  price at +0x34, then `CheckBudget` runs.
- **`ExecuteLineDraw` (0x619F8):** runs `DrawLine`. If it fails, the flag at +0x38 is set.
- **`ExecuteOnMouseUp` (0x5EB10):**
  - It reports an error through service 0xFA2 (sound or message): code 0x20 when the +0x38 flag
    is set, 0x1E when the +0x39 flag is set (probably not enough money).
  - Otherwise it calls `PlaceRoad`, and reports 0xE if that fails.

### `CanPlaceRoad` (0x69D44)
- **Price:** it reads up to 0x201 selected cells. It also reads each cell's selection state but
  does not use it. It counts the cells that have no occupant or hold one that fails
  `mpRoadTest`. The price is `price(16000) × count`.
- **Return value:** always 1. It never rejects; whether a cell is valid comes from `DrawLine`.

### Line drawing: `cSLCityViewLineAlgorithm`
- **`Setup` (0x63360):** stores its arguments:

| Offset | Value |
|---|---|
| +0x38 | highway mode |
| +0x3C | occupant manager |
| +0x40, +0x44, +0x48 | the three tests |
| +0x54 | dirt bag |
| +0x58 | budget |
| +0x5C, +0x60 (`3` for roads, water checked) | |
| +0x64 | |
| +0x65 | slopes allowed |
| +0x94 | price per cell |

- **`addCell(x, y, invalid)` (0x504F8):**
  - It appends the cell and sets its selection state: 0x23 valid, 0x24 invalid. An invalid
    cell already in the list is not added twice.
  - There is a global cap of 0x200 cells.
  - `PlaceNetworkEx` later places only cells in state 0x23.
- **`canEdit(x, y)` (0x612E4):** false if the cell holds an occupant that fails test +0x40, or
  holds one while the cell is water (water is checked when +0x60 ≠ 0). An empty cell can be
  edited.
- **`canCrossStraightOnly(x, y, dx, dy)` (0x613E8):**
  - The cell must be flat, or sloped along the step direction (`isDirtSlopedZ` / `X`).
  - An occupant must then pass test +0x48 or +0x44, chosen by direction.
- **`emptyOrTree` (0x5A2D0):** no occupant, or an occupant of type 5.
- **`DrawLine(start, end, …)` (0x50604):** walks from start to end one edge step at a time; a
  road has no diagonal steps. Partly traced:
  - **Axis:** it steps along the axis with the larger remaining distance. On a sloped cell it
    is held to the slope direction.
  - **Validity:** a cell is valid if `canEdit`, or `canCrossStraightOnly` for the step.
    - Once a cell is invalid, every later cell is too.
    - Leaving the bounding box of start and end also invalidates.
  - **Water:** consecutive water cells form a run. A second run makes the line invalid, so a
    road crosses at most one stretch of water.
  - **Affordability:** the cost is (number of 0x23 cells) × the price per cell. If it is above
    the funds, every cell is set to 0x24.
  - **Highways:** highway mode (+0x38) widens the line to 2 cells. That path is not traced.

### `PlaceNetwork(sel, net, &cost, &bounds, &coord)` (0x6BEC4)
1. It calls the network's `CanPlace…` for the price:

   | vtable | Function |
   |---|---|
   | 0x80 | road |
   | 0x84 | highway |
   | 0x88 | rail |
   | 0x8C | subway |
   | 0x90 | pipe |
   | 0x94 | power |
   | 0x98 | on-ramp |

2. It reads the selection in batches of 0x500 cells.
3. It calls `PlaceNetworkEx` on each batch and stops at the first failure.

### `PlaceNetworkEx(occman, sel, net, cost, &bounds, cells, n, &coord)` (0x6C268)
1. **Cell list:** keeps the cells the view marks 0x23, as list L, and remembers the first two
   and the last two. If L is empty, it returns.
2. **Funds:** if the funds are below `cost`, it fails.
3. **Highway direction:** for a highway, the direction (dx, dy) is the second cell minus the
   first.
4. **Start of the line:** for the first cell, and the one after it along (dx, dy), it removes
   an occupant with flag 0x400 if that occupant may be removed. If both cells are then free and
   not both sloped, it calls `FlattenCellIfYouCan` (vt 0x120).
5. **Bridges (0x6CAED switch):**
   - Road, rail and power call `PrepareAndPlaceBridges` (vt 0x14C) when the selection has more
     than 3 cells. Highway calls `PrepareAndPlaceBridgesforHway` (vt 0x150). Pipe and subway
     have none.
   - The bridge cells are removed from L, and the bridge cost is added.
   - If the bridge step returns true, the function exits here; what happens then is not traced.
6. **Prepare:** `FullDoPrepare` (vt 0xC8). Extra cells it returns are appended to L.
7. **Tunnels:** a highway calls `PrepareAndPlaceTunnelforHighways` (vt 0x140); any other
   network calls `PrepareAndPlaceTunnel` (vt 0x144). Either one rewrites the end of L.
8. **End of the line:** without a tunnel, it flattens the last cell and the one before it,
   like the start.
9. **Edge connection:** `PrepareAndPlaceConnection` (vt 0x13C).
10. **Solve:** `EditNetwork(copy of L, net, bounds)`.
11. **Highway roads:** for a highway, `AddRefreshTilesToTileList` collects the road cells
    around it, and `EditNetwork(…, 1, …)` re-solves them.
12. **Pay:** it spends `cost` (budget vt 0x1C).

Not traced: `FullDoPrepare`, `DoPrepare`, `FlattenCellIfYouCan`, the bridge and tunnel
routines, and `PrepareAndPlaceConnection`. They shape the terrain or handle special cells. For a
first port of roads on flat land, steps 4–9 can be skipped.

### `EditNetwork(cells, net, &bounds)` (0x6EA48)
**Setup:** it picks the network's families, tests and occupant manager. There are two flags:
- **`replace` (`b11`):** road, rail and highway use the edit test; it makes `InsertNetwork`
  clear the +0x34 layer.
- **`water` (`b12`):** pipe and subway; tiles may go on water.

Highways also get the (dx, dy) of the first two cells.

1. **Region:**
   1. Around each placed cell it takes a window of radius 5 (road, highway, subway) or 2
      (others).
   2. In it, it marks the cells that are being placed or already hold this network.
   3. It flood-fills 8-connected from the placed cell through the marked cells, up to 32
      rounds.
   4. The filled cells form the region, which is sorted and made unique.
2. **Intersections:** every placed cell that already holds any network occupant goes through
   `InsertNetworkIntersection`. The cells it handles are set aside as done.
3. **Rip-up:** every region cell that holds this network and is not protected goes into the
   work list W, and its occupant is removed (if it may be). For power, protected cells go to
   the complex list C instead.
4. **Work list:** the placed cells are added to W, which is sorted and made unique. Cells that
   hold protected tiles, and the done cells, are dropped.
5. **Lists:** the W cells that are sloped (`!isDirtFlat && isDirtSloped`) go into S. Every W
   cell is added to the pending list P and to C. The bounds of W are written out. S is removed
   from W.
6. **Slopes:** for each cell in S, `selectSloped` gives one tile, and `InsertNetwork` places it.
   - A flag alternates from cell to cell. Its start depends on the sign of dx + dy. Only
     highways use it, and the lane check overrides it.
   - The placed cells are removed from W.
7. **Simple pass:** it runs `SolveNetwork(W, P, …, Simple, mode 8)` and removes the solved
   cells from W. It repeats while W shrinks.
8. **Complex pass:** it runs `SolveNetwork(C, P, …, Complex, mode 24)` and removes the solved
   cells from W. It repeats while anything was solved, at most 5 times.
9. **Cleared cells:** cells that a solution set to "empty" are added back to W.
10. **Final pass:** `SolveNetwork(W, P, …, Eval, mode 4)` runs once.

### `SolveNetwork(list, pending, done, cleared, family, occman, test, notTest, editTest, replace, water, mode, net)` (0x71854)
For each cell of `list` that is flat (`isDirtFlat`):
1. `CreateNeighborhood(cell, mode)` builds a neighbourhood, and `cTileEvaluator::Lookup` finds
   solutions in `family`.
2. **Checks:** every solution names a cell (`pos` relative to this one). If that cell is in
   bounds, it must be flat. If it holds an occupant, the solution is rejected when either:
   - the occupant fails both `test` and `editTest`;
   - for a network other than power, the occupant is protected and the solution is not
     `(id 0, rot 5)`.

   If any solution is rejected, the cell is skipped.
3. **Placing:** otherwise `InsertNetwork` places all solutions.
4. **Bookkeeping:** each solution cell goes to `cleared` if the solution is `(0, rot 0)`, and to
   `done` otherwise.

### Neighbourhoods: `CreateNeighborhood(…, cell, mode, net)` (0x90374)
A `cTileNeighborhood` holds a 32-bit key and a list of identities `(id, pos, rot)`.

**Modes 8 (simple) and 4 (final):**
- **Window:** 3×3 around the cell, clamped to the map.
- **This network's occupants:** a neighbour at meta position `p < mode` sets key bit `p`.
  - Mode 4 for road, rail or subway also asks the neighbour's `cSTNetworkOcc::GetConnectivity`
    (0x5C204) whether it connects on the side facing this cell. Kind: road 0, rail 1, subway 2.
    The sides are meta 0→2, 1→3, 2→0, 3→1.
  - A subway occupant with flag 0x40100 sets the bit directly.
- **Identities:** every occupant in the window adds one via `InitTileIdentity`.
- **Pending cells:** any cell of `pending` within 2 cells counts as this network. It sets bit
  `p < mode` if the cell is empty or holds something that fails `test`.
- **Map edges:** on an edge cell (x = 0, y = 0, x = w − 2 or y = h − 2), the bit pointing off
  the map is set if the edge connection (+0x48, vt 0x60) allows it.

So the mode-8 key is the 8-neighbour mask (bits 0–7), and the mode-4 key is the 4-neighbour
mask (bits 0–3). See the meta grid in `tiling.md`.

**Mode 24 (complex):**
- **Window:** 5×5. Every cell adds an identity, and the key stays 0. All complex rules use the
  wildcard key 0x100.
- **Off the map:** `(0, pos, 5)`. It becomes rot 1 if `pos` is the edge direction and the edge
  connects.
- **Empty cell:** `(0, pos, 5)`. It becomes rot 1 if the cell is in `list`, the cells being
  solved.
- **Occupied cell:** `InitTileIdentity`. If that gives `(0, 5)` and the cell is in `list`, it
  becomes rot 1.

### `InitTileIdentity` (0x93670)
- **This network's occupant (`test`):**
  - `id` = the instance of its occupant key, low 16 bits.
  - `rot` = its rotation.
  - Both go through `convert(mode, net, …)`, then `pos` = the cell's meta position.
- **Occupant passing `notTest`:** `(0, rot 5)` if it has flag 0x400, otherwise `(0, rot 0)`.
- **Anything else:** the identity stays `(0, 0x1F, 0)`, then gets its meta position.

`convert` (0x91AEC) looks the pair up in the network's convert set:
- `CvrtSimple` (`_Convert.txt`) for modes 4 and 8;
- `CvrtComplex` (`_Complex_Convert.txt`) for mode 24.

The last matching entry wins.

### Matching (exe)
- **Identity match:** `cTileIdentity::Match` (0x08240038) needs the same `pos` and `id`, and
  either the same `rot` or a rule `rot` of 0 against a cell `rot` of 0 or 5. So in a rule:
  - `(0, pos, 0)` means "not this network" (empty or other);
  - `(0, pos, 5)` means empty;
  - `(0, pos, 1)` means "about to become this network".
- **Rule:** `cTileRule::Match` (0x0823E824) holds if every condition is in the neighbourhood.
  A rule with no conditions always matches. The solutions are copied out.
- **Lookup:** `cTileEvaluator::Lookup` (0x0824071C) tries families, then sets, then rules, in
  file order, and stops at the first match. A set takes part when its key is 0x100 or equals
  the neighbourhood key.

### `InsertNetwork(occman, cell, solutions, test, editTest, replace, water)` (0x71DD0)
- **Water:** without `water`, it fails on a water cell (`isDirtWater` = dirt bag `IsWater`).
- **For each solution,** at the cell it names, if that cell is in bounds:
  - **id 0:** remove the occupant there, if it may be removed.
  - **Otherwise:**
    1. Create the tile: `CreateOccupant(&occ, class C14F8955, id, rot)` (vt 0x58).
    2. Place it at the cell and at altitude = dirt bag vt 0x54 at (x, y).
    3. Insert it. If that fails because something is there that passes `test` or `editTest`
       and may be removed, remove that and insert again.
    4. On success with `replace`, call the +0x34 layer's vt 0x40 `(x, y, 0)`.
- **Return value:** the result of the last insert.

### `InsertNetworkIntersection` (0x720C4)
Turns an existing tile into a crossing when another network is drawn across it.

- **When:** the occupant must pass `test` or `editTest` and must not be protected.
- **Existing kind:** read from its tile id:

  | Tile id | Kind |
  |---|---|
  | 29, 11204 | road (1) |
  | 44 | rail (2) |
  | 14, 15, 92 | power (3) |
  | 73 | highway (4) |
  | 15051 | 5 |

- **New kind:** the network being drawn. A highway with dy ≠ 0 counts as kind 5.
- **Table:** the crossing comes from a 6×6 table at data 0xA0720, indexed `[a][b]`:
  - with the orientation flag set, `a` = new kind and `b` = existing kind;
  - otherwise `a` = existing kind and `b` = new kind.

  The flag comes from the existing tile's rotation (highways: the drag direction). Entries
  are packed tile values:

  | | road | rail | power | hwy | hwy (5) |
  |---|---|---|---|---|---|
  | road | | 69 r0 | 67 r0 | 11258 r2 | 15098 r2 |
  | rail | 69 r1 | | 71 r0 | 16006 r2 | 16007 r2 |
  | power | 67 r1 | 71 r1 | | 15036 r2 | 15037 r2 |
  | hwy | 11258 r0 | 16006 r0 | 15036 r0 | | |
  | hwy (5) | 15098 r0 | 16007 r0 | 15037 r0 | | |

- **Placing:** the old occupant is removed and the crossing inserted. The altitude comes from
  dirt bag vt 0x50.

### `selectSloped(cell, net, list, &id, &rot, flag)` (0x91490)
1. **Shape:** compares the cell's corner heights relative to corner (x, y) with 8 shapes at
   0xA07C0. The corners are in the order (x, y), (x+1, y), (x+1, y+1), (x, y+1).

   | k | Shape | Meaning |
   |---|---|---|
   | 0 | 0, −1, −1, 0 | falls 1 towards x+1 |
   | 1 | 0, 0, −1, −1 | falls 1 towards y+1 |
   | 2 | 0, 1, 1, 0 | rises 1 towards x+1 |
   | 3 | 0, 0, 1, 1 | rises 1 towards y+1 |
   | 4–7 | as 0–3 | by 2 |

2. **No match:** the function fails.
3. **Tile:** it returns `table[k]`, a packed value, from the network's table:

   | Net | Table | Tiles for k = 0–7 |
   |---|---|---|
   | road | 0xA07E0 | 11202 r0, 11201 r0, 11202 r2, 11201 r2, 32 r0, 31 r0, 32 r2, 31 r2 |
   | rail | 0xA0800 | 16008, 16009, 16008 r2, 16009 r2, 46, 47, 46 r2, 47 r2 |
   | subway | 0xA0820 | 11616, 11617, …, 11614, 11615, … |
   | pipe | 0xA0840 | 11650, 11649, …, 11648, 11647, … |
   | power | 0xA0860 | 11262 r0–r3, 11263 r0–r3 |
   | power with pylon | 0xA0880 | 18081, 18080, …, 18083, 18082, … |
   | highway | 0xA08A0 / 0xA08C0 | two lane variants |

   - **Power:** uses the pylon table when (x + y) mod 5 = 0, or unless both cells along the
     slope are in `list`.
   - **Highway:** uses 0xA08C0 when the cell beside it, across the slope, is in `list`, and
     0xA08A0 otherwise.

### Terrain predicates
- **`getDirtWaterVertexAlt` (0x91374):** the four corner heights from dirt bag vt 0x54,
  relative to corner (x, y).
- **`isDirtFlat` (0x93400):** all four corners are equal.
- **`isDirtSloped` (0x93460):** a single ramp (two pairs of equal corners) with a rise of at
  most 2.
- **`isDirtWater` (0x93564):** dirt bag `IsWater`.

### Other helpers

| Function | Address | What it does |
|---|---|---|
| `isProtected(net, id)` | 0x91A54 | `id` is in the `_Protected.txt` list |
| `isRoad(id)` etc. | 0x939D0 | `id` is in the `_Set.txt` list |
| `SameNet(net, occ)` | 0x98084 | `mp<Net>Test` |
| `getTileID(occ)` | 0x93C08 | `cSTNetworkOcc::TileId` (IID `41658D28`), else the occupant key's instance |
| `getTileRuleID(occ)` | 0x93964 | `id << 8 \| rot` |
| `mustStaySurfOcc(net, coord)` | 0x6C0F0 | the surface occupant is not a tree and is shared with another network, or is a protected tile of `net` |

## `cSTTransitLayer` vtable
Read from the relocations of `__vt_15cSTTransitLayer` (data 0xA4540). The slots that are used
above:

| Slot | Function |
|---|---|
| 0x58 | `CreateOccupant(void**, uint, uint, uint)` |
| 0x5C | `CreateOccupantTest` |
| 0x80–0x98 | `CanPlaceRoad`, `Highway`, `Rail`, `Subway`, `Pipe`, `Power`, `Onramp` |
| 0x9C–0xB4 | `PlaceRoad`, `Highway`, `Rail`, `Subway`, `Pipe`, `Power`, `Onramp` |
| 0xC8 | `FullDoPrepare` |
| 0xF4 | `PlaceNetwork` |
| 0xF8 | `PlaceNetworkEx` |
| 0x120 | `FlattenCellIfYouCan` |
| 0x124–0x138 | `isRoad`, `isHighway`, `isRail`, `isSubway`, `isPipe`, `isPower` |
| 0x13C | `PrepareAndPlaceConnection` |
| 0x140 | `PrepareAndPlaceTunnelforHighways` |
| 0x144 | `PrepareAndPlaceTunnel` |
| 0x14C | `PrepareAndPlaceBridges` |
| 0x150 | `PrepareAndPlaceBridgesforHway` |
| 0x154 | `SameNet` |
| 0x158 | `myPrepareForRoad` |
| 0x160 | `mustStaySurfOcc` |

## Saving and loading (not traced)

| Function | Address |
|---|---|
| `cSTTransitLayer::Init(cISC3City*, cIGZDBSegment*)` | 0x68980 |
| `cSTTransitLayer::Save` | 0x68490 |
| `cSTTransitLayer::WriteOccupants` | 0x68764 |
| `cSTNetworkOcc::SerialRead` / `SerialWrite` | 0x5BEB4 / 0x5BF40 |
| `cSTNetworkOcc::Init(cISC3City*, cISC3OccPersistInfo*)` | 0x5B064 |

## Still open
- **Static setup:** `StaticInit`, which builds the occupant tests and loads the tables.
- **Terrain and special cells:** `FullDoPrepare` / `DoPrepare`, `FlattenCellIfYouCan`, bridges,
  tunnels, edge connections.
- **`DrawLine` details:** the step-by-step rules, and highway mode.
- **Unknown flags and objects:** occupant flag 0x400, the +0x34 layer call, the +0x48
  edge-connection object.
