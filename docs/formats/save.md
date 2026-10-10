# Saved cities and terrains

Ports:
- `crates/sc3k-formats/src/segment.rs`: the compressed DB segment.
- `crates/sc3k-formats/src/serial.rs`: serial records.
- `crates/sc3k-sim/src/load.rs`: the dirt bag, flora layer and network records.

Inspect with `sc3k-dump iso-file <file> <zoom> <out.png>`.

A saved city (`.sc3`) and a terrain (`.sct`) have the same structure. The game also writes
starter towns (`.st3`) and scenarios (`.SNR`) this way. Only the ground is read so far: the
terrain and the trees.

## Container: `cRZDBSegmentCompressed`
- **Code:** the Loki demo's `cRZDBSegmentCompressed` (`sc3u_demo.x86` 0x0833D404 onwards).
  The Windows copy is in `GZResourceD.dll`; `StartCompressionGroup` is at 0x1000F5C4.
- **The file:** an ordinary [IXF container](ixf.md). A few records are stored as is; for
  example, in the `.sct` files the city record starts with `City Data Start`.
- **Packed records:** the rest are packed into one or more records that the description
  record lists.
- **Lookup:** a record is looked up in the container first, then in the packed segments in
  listed order (`DoOpenRecord`, demo 0x0833E290).

### Description record `035F62A4-035F62A4-00000000`
`kCompressedRecordHeaderKey`, read by `readCompressDescRecord` (demo 0x0833FBB0):

```
u32     method          0x65 or 0x67
i32     count
key[count]              type, group, instance of each packed record
```

All 21 terrains and 14 cities in the install list one record, `035F62A4-035F62A4-00000001`.

### Packed record
`cRZDBSegmentMemory::serial_read` (demo 0x08340F90):

```
u32     method          0x65 decoderef, 0x67 cRZFastCompression3
string  version         u32 length + bytes: "0.90"
u32     packed size
u32     unpacked size
bytes   packed          u32 size of the whole block, then a QFS stream (qfs.md)
```

- **Decoding:** both methods skip the 4-byte size. Method 0x65 calls `decoderef` on the
  stream directly; method 0x67 goes through `cRZFastCompression3::DecompressData`, which
  skips it itself.
- **Size check:** the unpacked size must match exactly.

### Unpacked memory segment
`construct_keylist_from_segment_memory` (demo 0x083412E0):

```
u32     count
u32     directory offset
...     records
{u32 type, u32 group, u32 instance, u32 offset}[count]   at the directory offset
```

- **Skipped entries:** entries whose offset is not below the directory are skipped. Of equal
  keys, the first wins.
- **Record size:** a record has no stored size. `DoOpenRecord` gives it everything from its
  offset to the end of the segment memory, and the reader stops where its fields end. The
  port cuts a record at the next record's offset.
- **Boston, MA.sct:** 58 records in 1,521,434 bytes, packed to 226,734.

## Serial records
A layer reads its record field by field through `cIGZDBSerialRecord`. Fields are
little-endian and unaligned.

| Field | Loki slot | Windows slot | Bytes |
|---|---:|---:|---|
| `GetFieldUint8` | 0x1C | 0x18 | 1 |
| `GetFieldVoid(dst, n)` | 0x20 | 0x14 | n |
| `GetFieldUint16` | 0x2C | 0x28 | 2 |
| `GetFieldUint32` | 0x3C | 0x38 | 4 |
| `GetFieldFloat32` | 0x44 | 0x40 | 4 |
| `GetFieldString` | 0x5C | 0x54 | u32 length, then the bytes |
| `GetFieldResKey` | 0x60 | | type, group, instance |

### Version header
`cSLAutoSaveRecordVersionInfo` (demo 0x0823D924; SIMDIRT.DLL 0x1001D17B) reads it first:

```
u16     version
u8      flags           bit 0: "<Layer> Start" / "<Layer> End" marker strings around the fields
                        bit 1: one more byte follows
u8                      only with bit 1; read and ignored
u32     0xDEADBEEF
```

- **No guard:** if the guard is missing, the record is reopened and read from byte 0 as
  version 0 without markers.
- **Examples:**
  - The `.sct` files have `03 00 03 00 EF BE AD DE`: version 3, with markers and the extra
    byte.
  - `Madison, WI.sc3` has `03 00 02 00 …`: no markers.

## Dirt bag `206C6E7C-21737DE5-00000000`
`cSC3DirtBag::Init(cISC3City*, cIGZDBSegment*)`: libSimDirt Ghidra 0x33564, SIMDIRT.DLL
0x10004A00. `Save` is at Ghidra 0x33C20 and SIMDIRT.DLL 0x10004D90.

```
header          version 3
string          "DirtBag Start"               with markers
f32             altitude scale, 3.266         into the static mkAltScale
u8[X+1][Z+1]    vertex altitude
u8              sea level
u8[X+1][Z+1]    water level per vertex
u32[X·Z/32]     blocked cells: bit (x + z·X) % 32 of word (x + z·X) / 32
string          "DirtBag End"                 with markers
```

- **Cell maps:** a `cRZCellMap<u8>` is written one column per x, highest x first. Each
  column holds Z+1 bytes for y = 0 upwards.
- **Size:** X and Z are the city's cell counts (`cISC3City` vtable 0xCC/0xD0 on Windows).
  The record does not hold them. The port takes the square size that fits the record's
  length: 64, 128, 192 or 256 in the install.
- **Blocked cells:** `BlockCell` (Ghidra 0x4204C) and `IsCellBlocked`.
- **Not saved:**
  - Salt water: `RecalcSaltWater` rebuilds it. Not ported; the port reads it as fresh.
  - The generator's flora map.
- **Vertex light:** after reading, `Init` computes the light of every vertex
  (`calculateAndSetVertexLight`, SIMDIRT.DLL 0x10007010).
- **Checked:** `tools/diffcheck/run.py ground` runs the Windows `Init` on every `.sct` and
  `.sc3` in the install and compares altitude, water, sea level and vertex light. All match.

## Flora layer `406B1196-80AB8AB0-00000000`
`cSC3FloraLayer::Init(cISC3City*, cIGZDBSegment*)` (libSimGeom Ghidra 0x5BA84):

```
header          version > 2 (versions up to 2 hold no trees)
string          "Flora Layer Start"           with markers
u32 X, u32 Z    must equal the city's cell counts
u8[X][Z]        one byte per cell, column x = X − 1 first: type << 4 | density, 0 = none
string          "Flora Layer End"             with markers
```

- **Placement:** each non-zero cell becomes `CreateOccupant(type, density)` (Ghidra 0x5D5F4),
  the same call a new map's trees go through (`docs/sim/flora.md`). The tree's z is the
  dirt bag's `GetVertexAltitude(x, y)`. Loaded trees go on water cells too: only
  `SetFloraDensity` skips water.
- **Counts:** Boston has 647 trees, Madison 9,862.

## Networks `206C6E7C-2147C2DD-0000000[0-3]`
Roads, rail, highways, power lines, pipes and subways. The network types and occupant managers
are in `../sim/transit.md`.

- **Port:** `read_network_layer` reads the three records into `transit::Networks`, one
  `CellMap<Option<NetworkTile>>` per manager. It keeps the tile id, the rotation and the
  altitude, and drops the flags. `read_ground` calls it and gives empty networks when there
  is no header record, as in a terrain (`.sct`).
- **Skipped tiles:** a tile outside the city is skipped.
- **Not ported:** the occupant classes.

- **Reading:** `cSTTransitLayer::Init(cISC3City*, cIGZDBSegment*)` (libSimNtwrk Ghidra 0x68980).
- **Writing:** `Save` (0x68490), which calls `WriteOccupants` (0x68764) once per occupant
  manager.

### Header, instance 0
```
header          version 4 (no markers in any install file)
u32             surface count       roads, rail, highways, power lines, on-ramps
u32             plumbing count      pipes
u32             subway count
u32             persist version     1
```

- **Version:** with any other version, `Init` reads nothing and still succeeds.
- **Instances 1 to 3:** one record per occupant manager, in the order surface, plumbing,
  subway.
- **Empty manager:** a record is only read when its count is not 0. `WriteOccupants` writes
  none for an empty manager.

### Occupant records, instances 1 to 3
```
header          version 4; Init does not check it
{u32, u32}[count]
```

Each pair is a `cSC3TransitBlockPersistInfo` (`SerialRead` 0x5CDA8, `SerialWrite` 0x5CD54).
`SerialRead` fails unless the persist version is 1.

| Bits | Field | Accessor | `cSTNetworkOcc` field |
|---|---|---|---|
| word 0, 0–10 | cell x | `GetLocationInCells` (0x5C91C) | +0x14 bits 0–10 |
| word 0, 11–21 | cell y | | +0x14 bits 11–21 |
| word 0, 22–29 | altitude | | +0x16 bits 6–13 |
| word 0, 30–31 | rotation | `GetRotationFlag` (0x5C980) | +0x17 bits 6–7 |
| word 1, 0–15 | tile id (int 0x6355941D) | `GetAttribKey` (0x5C95C) gives `E223741F-A317745F-<tile>` | +0x18 |
| word 1, 16 | has teleport data (0x6355941E) | `HasTeleportData` | +0x1A bit 1 |
| word 1, 17–18 | teleport direction (0x6355941F) | | +0x1A bits 2–3 |
| word 1, 19–26 | teleport distance (0x63559420) | | +0x1A bits 4–11 |
| word 1, 27 | needs repair (0x63559421) | `GetNeedsRepair` | +0x1A bit 0 |
| word 1, 28 | the sim may remove it | `GetCanSimRemoveFlag` | written from +0x13 bit 1, read into bit 0 |
| word 1, 29 | the user may remove it | `GetCanUserRemoveFlag` | written from +0x13 bit 0, read into bit 1 |

- **The remove flags swap.** `Save` writes +0x13 bit 1 as the sim flag
  (`SetRemoveFlags(bit 1, bit 0)`, persist vtable 0x44). `Init` reads the sim flag
  (vtable 0x28) back into bit 0. So the two flags change places on every save and load.
- **The effect:** none on the install files. There both flags are set, or both are clear.

The hex numbers in brackets are the ids that `GetIntData`/`SetIntData` (0x5CB74/0x5CC1C) use
for the word-1 fields. `cSTNetworkOcc::Save` (0x5B284) fills the persist info and writes the
teleport fields only when the occupant has teleport data.

### Loading an occupant
For each pair, `Init` calls `cSTTransitLayer::CreateOccupant(persist info)` (vtable 0xF0,
0x9290C). It then inserts the occupant into that record's manager (vtable 0x44).

1. **Class:** `getSubType(tile)` (0x91894) asks `isRoad`, `isHighway`, `isRail`, `isSubway`,
   `isPipe` and `isPower` (layer vtable 0x124 to 0x138). It combines the answers into a class
   for `cSTNetworkOcc::CreateNetworkOccupant` (0x5A504):

   | Value | Class | Tile is in |
   |---|---|---|
   | 1 | `cSTRoadOcc` | road |
   | 2 | `cSTHighwayOcc` | highway |
   | 3 | `cSTRailOcc` | rail |
   | 4 | `cSTSubwayOcc` | subway |
   | 5 | `cSTRoadRailOcc` | road and rail |
   | 6 | `cSTRoadHighwayOcc` | road and highway |
   | 7 | `cSTRailHighwayOcc` | rail and highway |
   | 8 | `cSTSubwayRailOcc` | subway and rail |
   | 9 | `cSTPipeOcc` | pipe |
   | 10 | `cSTWireOcc` | power |
   | 11 | `cSTPowerRoadOcc` | road and power |
   | 12 | `cSTPowerRailOcc` | rail and power |
   | 13 | `cSTPowerHighwayOcc` | highway and power |
   | 14 | `cSTOnRampRoadOcc` | road and highway, tile 0x3B1A to 0x3B25 |
   | 15 | `cSTOnRampHwyOcc` | highway only, tile 0x3B1A to 0x3B25 |

   `CreateNetworkOccupant` also knows bridge and tunnel classes (16 and up). `getSubType`
   never returns them, so loaded bridges and tunnels get the class of their network.
2. **Fields:** `cSTNetworkOcc::Init(cISC3City*, cISC3OccPersistInfo*)` (0x5B064, occupant
   vtable 0x24) calls `Init(tile, rotation)` (0x5BE00), which loads the occupant record (see
   `../render/roads.md`). It then copies the location, the remove flags, the repair flag and
   the teleport data into the fields in the table above.

### What the install files hold
Checked on every `.sc3`, `.SNR` and `.st3` in the install.

- **Counts:** every record holds exactly the header's count, with no bytes left over.
- **One per cell:** no record has two occupants on the same cell. A cell can still have one
  surface, one plumbing and one subway occupant.
- **Altitude:** in the same units as the dirt bag's vertex altitude. Most occupants are at
  the lowest corner of their cell. Some are higher than every corner, for example bridges
  (18050, 18053) and tiles 60 and 16063.
- **Flags:** almost every occupant has word 1 bits 28 and 29 set (`0x3000` in the high half).
  - `Sacramento, CA.sc3` has 61 road tiles that neither the sim nor the user may remove, and
    one tile (9051) that needs repair.
  - Teleport data comes in pairs on tile 64, for example in `Mount Herrang.sc3`:
    - `0x3031`: direction 0, distance 6;
    - `0x3035`: direction 2, distance 6.

    It probably links the two ends of a tunnel; this is not traced.

## Not read yet
- **The city record:** name, date, funds and the scheme keys (landscape, flora set,
  buildings). The port shows a loaded city with the default landscape and flora set, so
  Madison's trees come out as palms.
- **Other layers:** zones, buildings, pollution and the other city layers. The
  segment holds about 58 records; the
  [census](ixf.md) lists their keys.
