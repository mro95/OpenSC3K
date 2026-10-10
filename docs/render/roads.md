# Drawing roads

Port so far: `crates/sc3k-render/src/roads.rs` loads one tile and draws layer 0 of a frame;
`sc3k-dump road-tile <id> <zoom> <out.png>` shows a tile's four rotations. Drawing roads in the
city is not ported yet.

This page traces how a network tile id becomes a picture; the tile ids come
from the tiling rules (`docs/formats/tiling.md`), and the network occupant from
`docs/sim/transit.md`. The record formats are in `docs/formats/occupant.md` and
`docs/formats/sprite.md`.

The chain is the same as for trees (`docs/render/flora.md`), with a different occupant key and
sprite archive. There is no road set: every record comes from the base files.

## From tile id to picture
1. **Occupant key.** `cSTNetworkOcc::Init(tile, rotation)` (libSimNtwrk Ghidra 0x5BE00):
   - asks `AsSC3ResourceKey` (0x5BFC4) for the key `E223741F-A317745F-<tile>`;
   - stores the tile id at +0x18 and the rotation in the top two bits of +0x17;
   - passes the key to `cSC3Occupant::Init(cGZResourceKey&)`, which loads the occupant record.

   The keys the tile set reader builds (`GrokFileTileSet`) are therefore occupant keys.
2. **Occupant record** (`E223741F/A317745F/<tile>`, `TKB1`) in `Res/Occupant/OccupantAttribs.IXF`.
   - The file holds 390 network records: roads, highways, rail, subway, pipes, power lines,
     bridges, tunnels and ramps.
   - Property 0x64 is the name, for example "Road" for tile 29.
   - Property 0x67 is the sprite-attribute key `6300/6400/<id>`.
3. **Sprite attributes** in `Res/Sprites/CSATTRIB.IXF`. Each entry names a sprite (group,
   instance).
   - The instance is `<id> << 16 | index`, with `<id>` the sprite-attribute id, not the tile id.
   - Group 5 is `00000005_Roads.DAT`. Group 0xB is `0000000B_Utilities.DAT`; road tiles use it
     for the Harbour Bridge, two Golden Gate pieces, the bridge ramps and one road ending.
4. **Sprite:** the pixels (type 0) and the image info (type 1), as for trees.

### The sprite-attribute id is not always the tile id
Follow property 0x67; do not build the key from the tile id. Ten records point elsewhere:

| Tile ids | Sprite-attribute id |
|---|---|
| 0x3AC5, 0x3AC6 (on-ramp intersections) | 0x27 |
| 0x3AC7, 0x3AC8 | 0x2B |
| 0x3EBC | 0x3A |
| 0x3EC1 | 0x14D |
| 0x46A4 to 0x46A7 | 0x3B2E to 0x3B31 |

## Rotation
A network tile has a rotation of 0 to 3 (`cSTNetworkOcc::GetRotationFlags`, 0x5C14C: the top
two bits of +0x17). It is the same field as `cSC3Occupant::GetRotationFlag` (exe 0x0822B554,
top two bits of the occupant's +0x13).

- **Into the sprite:** `cSC3Occupant::SetupSpriteInst` (exe 0x08227E18) creates the sprite
  instance and calls its `SetRotationOffset` (vtable 0x2C) with that rotation.
- **Choosing the frame:** `cSC3CitySpriteInst::zoom_and_compass_to_frame_no` (exe 0x0821DCF8):

  ```
  frame = zoom · 4 + ((rotation offset + view rotation) & 3)
  ```

  The tile's rotation adds to the camera's. Trees have offset 0, which is why
  `zoom · 4 + rotation` is enough for them.

## Sprite classes
The sprite attributes' class (+0x1C) is the GZCOM class that `SprAttCreateSprInst` (libSimSpr
Ghidra 0x7BD1C) creates, with interface 0x7100. `RZGetCOMDllDirector_SimSpr` registers the
factories:

| Class | Factory | Road tiles |
|---|---|---|
| 0x7300 | `cSPCOMDirector::Create_cSC3CitySpriteInst` | plain pieces, crossings |
| 0x7312 | `cSPCOMDirector::Create_cSC3CitySpriteInstLayered` | bridges, tunnel entrances, avenue, onramps |
| 0x672 | `cSC3CitySpriteInstSpecialFactory::UseCityCSMTerrain` | most road pieces, road endings |
| 0x59D | `…SpecialFactory::SlopedOneLow` | 11202 |
| 0x59E | `…SpecialFactory::SlopedOneHigh` | 11201 |
| 0x59F | `…SpecialFactory::SlopedTwoLow` | 32 |
| 0x5A0 | `…SpecialFactory::SlopedTwoHigh` | 31 |

The classes 0x672 and 0x59D–0x5A0 involve the terrain under the tile and are not traced
(`cSC3CitySpriteInstUseCSMTerrain`, `cSC3CitySpriteInstUseCSMTerrainSloped<…>`).

## Layers (the 40-entry records)
The header bytes after the class are, in file order (`SerialReadBinary`, 0x7B738, and the
`SprAtt…Count` getters):

| Byte | Field | Getter | Road records |
|---|---|---|---|
| 0 | +0x24 | `SprAttGetZoomCount` | 5 |
| 1 | +0x25 | `SprAttGetRotCount` | 4 |
| 2 | +0x26 | `SprAttGetZoomSetCount` | 4 |
| 3 | +0x23 | `SprAttGetAnimSetCount` | 20: frames per layer |
| 4 | +0x22 | `SprAttGetAnimFrameCount` | 1 or 2: the number of layers |
| 5 | +0x27 | `SprAttGetRegRectCount` | 0 |

Then the `u32` flags (+0x28).

- **Creating the layers:** `SprAttCreateSprInst` creates the first instance. If flags bit 0 is
  set, it creates a second one of the same class, calls `SetLayer(1)` on it and hangs it on the
  first with `SetNext`. If bit 1 is also set, a third gets `SetLayer(2)` and hangs on the
  second.
  - Every 40-entry road record has bit 0 set and bit 1 clear: two layers.
  - Every 20-entry road record has bit 0 clear, including the 0x7312 ones: one layer.
- **Frame of a layer:** `cSC3CitySpriteInstLayered::SetLayer(n)` (0xA4164) stores
  `n · SprAttGetAnimSetCount()` (vtable 0x74), so `n · 20`. Its
  `zoom_and_compass_to_frame_no` (0xA41A0) is

  ```
  frame = n · 20 + zoom · 4 + ((rotation offset + view rotation) & 3)
  ```

- **Rotation:** `cSC3CitySpriteInstLayered::SetRotationOffset` (0xA3DD0) passes the offset on
  to the next layer.
- **What the layers hold** (frames 16 and 36, zoom 4, compared by eye):
  - Layer 0 is the whole picture.
  - Layer 1 is the part that stands in front of traffic: the wires of Wire Crossing Road (67),
    the front railing of a bridge (87), the front of a tunnel arch (63).
  - For the Avenue (11204), every layer-1 sprite is an empty 4 × 4 image.

  Without vehicles, layer 0 alone gives the right picture.

## The road tile set
`ROAD_GRND_Set.txt` lists 109 tile ids.

- **No occupant record:** 11600, 11609, 17000, 17001, 18025, 18026, 18054, 18055. These ids are
  in no IXF file of the install.
- **No sprite attributes under the tile id:** the eight above, and 15045 to 15048, which reach
  theirs through property 0x67 (table above).
- **Sprite attributes of the other 97:** all have header dimensions `5, 4, 4, 20, …`, entries
  in index order, and instances `<id> << 16 | index`.

| Entries | Groups | Tiles |
|---|---|---|
| 20 | 5 | 44 |
| 40 | 5 | 32 |
| 40 | 0xB | 14 |
| 20 | 0xB | 7 |

## Open questions
- **When layer 1 draws.** The cell map gets the chained instance; where it puts layer 1 in the
  draw order (after vehicles?) is not traced.
- **The terrain classes** 0x672 and 0x59D–0x5A0: what they take from the terrain.
- **Placement on the cell and draw order** relative to the terrain clod and trees.
- **The other networks.** Only `ROAD_GRND_Set.txt` was checked against the records.
