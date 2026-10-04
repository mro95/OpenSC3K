# Saved cities and terrains

Ports:
- `crates/sc3k-formats/src/segment.rs`: the compressed DB segment.
- `crates/sc3k-formats/src/serial.rs`: serial records.
- `crates/sc3k-sim/src/load.rs`: the dirt bag and flora layer records.

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

## Not read yet
- **The city record:** name, date, funds and the scheme keys (landscape, flora set,
  buildings). The port shows a loaded city with the default landscape and flora set, so
  Madison's trees come out as palms.
- **Other layers:** zones, buildings, networks, pollution and the other city layers. The
  segment holds about 58 records; the
  [census](ixf.md) lists their keys.
