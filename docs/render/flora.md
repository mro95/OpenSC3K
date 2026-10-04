# Drawing trees

Port: `crates/sc3k-render/src/flora.rs`. Placement is in `docs/sim/flora.md`; the record formats
are in `docs/formats/occupant.md` and `docs/formats/sprite.md`.

## Flora sets
- `SC3CityScheme.ini` `[FloraSets]` lists the sets.

| Key | Directory |
|---|---|
| 0 | none: the base files |
| `0xc55a6d62` | `FloraSets/c55a6d62/Flora_Desert.dat` |
| `0xc55a6d84` | `FloraSets/c55a6d84/BlackForest.dat` (conifers) |
| `0x79ef435a` | `FloraSets/79ef435a/Flora_Jungle.dat` |
| `0x39edf62b` | `FloraSets/39edf62b/Flora_North West.dat` |

- **Loading a set:** `cSC3CitySchemeMgr::SetFloraSet(key)` (libSimInit Ghidra 0x44990)
  registers `res/sprites/florasets/<key in hex>` with the resource manager, ahead of the base
  files.
  - Key 0 registers nothing, so the base files are used.
  - Each set archive holds its own occupant records, sprite attributes and sprites.
  - BlackForest overrides 27 of the 29 occupant IDs in the table. The wetland IDs 0x1B8E and
    0x1B8F fall back to the base files.
- **Base files:** the occupant records are in `OccupantAttribs.IXF` and the sprite attributes
  in `CSATTRIB.IXF`. The sprites are in `00000009_Landscape.DAT`, whose instances are
  `<ID> << 16 | index`.

## From occupant to picture
1. Occupant record (`FFD30C03/80F48961/<ID>`): property 0x67 is the sprite-attribute key.
2. Sprite attributes: entry `zoom · 4 + rotation` names the sprite (group, instance).
3. Sprite: the pixels (type 0) and the image info (type 1).

## Where a sprite goes
The cell map keeps one record per draw-grid cell (`Insert`, libSimSpr 0x85544). The record holds:
- the sprite instance;
- the occupant's altitude z (+0xB);
- the image extents above and below (+4, +6).

`GetCityPixelRectForSpriteDrawGrid` (0x9AE80) puts draw cell (i, j):

```
left = (j − i) · halfWidth          left edge of the cell's bounding box
top  = (i + j) · halfHeight − z · altitudeStep − up
```

`DrawGridToCityPixel` adds `cellWidth / 2` to `left` for the top corner. So the anchor is
(left of the cell's box, height of its top corner raised by z). `SprAttDraw` (0x7C724) then
draws the image at `(anchor.x − left, anchor.y − up)`, using the image info.

- **Check:** across all flora sprites, `left` centres the picture on the cell. For example,
  zoom 4, 84 px wide, `left` = −20 puts the middle at 62 of 128.
- **Draw order:** flora's geometry flag 0x404 includes 0x40. `InsertCellAlignedSprite` (0xA8A14)
  therefore puts the terrain clod and the tree in one record. The terrain draws first, then the
  tree.
- **In the remake:** `TerrainScene::draw_with` calls the tree hook right after each clod, in the
  same back-to-front order.

## Not done
- Trees on cells that a city later builds over, and flora growth in a running city.
- The original's dirty-rectangle display lists. The remake repaints the whole view.
