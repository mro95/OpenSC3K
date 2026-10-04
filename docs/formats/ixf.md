# IXF container

The single archive format behind nearly every SC3U data file: sprite banks (`.DAT`),
resource indexes (`.IXF`), saves (`.sc3`), terrains (`.sct`), starter towns (`.st3`),
scenarios (`.SNR`), Building Architect output (`.bld`) and `Apps/SC3.cfg`/`SC3Net.cfg`.

Parser: `crates/sc3k-formats/src/ixf.rs`. Inspect with `sc3k-dump list <file>`.

## Layout (little-endian)

```
u8[4]   magic = D7 81 C3 80          (u32 0x80C381D7)
record[] index, 20 bytes each:
  u32 group_id
  u32 instance_id
  u32 type_id
  u32 offset      absolute file offset of the payload
  u32 size        payload size in bytes
terminator record (see below)
padding / payloads
```

### Terminator
| Form | Seen in |
|---|---|
| five `0x00000000` dwords | 588 files — the normal case |
| five `0xFFFFFFFF` dwords | 58 files — some localized `Res/Text/<LANG>/*.IXF`, `Apps/BARender/BAT.DAT` |
| none: file is just the 4-byte magic | 5 empty `Sc3HolidaysStringsPetitionerTickerY.IXF` |

### Observations (whole-install census, 651 containers, 197,592 records)
- No duplicate TGI within any file.
- No payload starts with a QFS/RefPack signature (`10 FB`): entries are uncompressed at
  the container level. Payload formats compress on their own (UI images: `image.md`).
- The index region is pre-allocated: payloads typically begin ~18–20 KB after the
  terminator (e.g. first payload at offset 20484 = 4 + 1024×20, i.e. room for 1024 records).
- 155 distinct groups. Run `sc3k-dump census` for the full table.

## Resource addressing
Records hold a GZ resource key (`cGZResourceKey`, `{type, group, instance}` as in SimCity 4)
stored in the order **group, instance, type**: `cGZDBSegmentIndexedFile::WriteNewIndexEntryForKey`
(Loki `0x08330770`) writes `key[1], key[2], key[0], offset, size`.
- **type** is constant per kind of resource: `62B9DA24` UI image, `2026960B` string.
- **group** usually matches the archive's file-name prefix (`85535958_CitySchemeUI.ixf`).
- **instance** is the id the code asks for. Code mostly passes (instance, group), e.g.
  `cSC3Buffer(0x22729921, 0x82B9B75C, …)` for the title background, and leaves the type implied;
  `cSC3CitySchemeMgr::GetLandScapeIconKey` builds a full key `{62B9DA24, 85535958, 1003}`.

The index region holds `((n >> 10) * 5 + 5) * 0x1000` bytes for `n` records (20 KB per 1024),
so payloads start at 20484 in small archives.

## Group families (initial labelling)

| Group | Meaning | Source files |
|---|---|---|
| `00000002`–`0000000B`, `00000010`, `00000014`, `00000015` | Sprite banks; the group matches the `.DAT` filename prefix (Residential, Commercial, Industrial, Roads, Vehicles, People, Other, Landscape, Landmarks, Utilities, Smoke, CityObjects, Boats) | `Res/Sprites/*.DAT` |
| `54AACF44`, `552D8025`, `5544F8BD`, `654FCB99`, `1000484E`… | Unlimited add-on sprite banks, same naming rule | `Res/Sprites/*.DAT` |
| `547B7DF5`, `5432D60E` | Building-set sprites (European/Asian sets) | `Res/Sprites/BuildingSets/*/` |
| `A1096A4F`, `029541F4`, `82E0074C`, `C2498F0E`, `225ADC6B`, … | String tables (buildings, windows, query, ticker, advisors, petitioners) | `Res/Text/<LANG>/*.IXF` |
| `01F58D3D` | `TXT!`-prefixed text records (building families: `LandUse`, `BuildingCount`, `Key: T,G,I,…`) | `Res/BUILDFAM.IXF` |
| `A25C7520`, `825C6289` | Per-language sim attribute tables | `Res/SSimData/<LANG>/*.IXF` |
| `530E2EE6`, `730E2EE6`, `035F62A4`, … | Save-game chunks | `Cities/*.sc3` |

## Save chunks (`.sc3`)
Saves and terrains keep most of their records QFS-packed inside the `035F62A4` records: a
compressed DB segment, specified in [`save.md`](save.md). Many save payloads start with the
version header `u16 version, u8 flags, [u8], u32 0xDEADBEEF` (e.g. `01 00 02 00 EF BE AD DE`);
the `0xDEADBEEF` is a serialization guard.
