# Occupant records and sprite attributes

These two record kinds connect an occupant ID (such as a tree) to its sprites. Parsers:
`crates/sc3k-formats/src/occupant.rs` and `csattrib.rs`.

## Occupant record (`TKB1`)
- **Key:** type `FFD30C03`. Flora uses group `80F48961`, with the occupant ID as the
  instance.
- **Files:** base records are in `Res/Occupant/OccupantAttribs.IXF`. Each flora set's archive
  carries its own copies.
- **Layout:** `TKB1`, then properties until the end of the record.
  - Each property is `u32 id, u16 type, u16 count`, then `count` values.

| Type | Value |
|---|---|
| 1 | `u8` |
| 3 | `u32` |
| 7 | string: `u32` length, then the bytes |
| 8 | resource key: `u32` type, group, instance |

All flora records parse with these four types.

| Property | Meaning |
|---|---|
| `0x64` | name, e.g. "Small Deciduous Tree" |
| `0x67` | key of the sprite attributes (type `6300`) |
| `0x6D` | another key, type `2026960B` (a string) |

Base flora point at sprite attributes `6300/6400/<ID>`. The flora sets point at
`6300/<set group>/3000000 + ID`.

## Sprite attributes (`cSC3CitySpriteAttrib`, type `6300`)
- **Files:** base records are in `Res/Sprites/CSATTRIB.IXF`. The flora and building sets
  carry their own.
- **Format:** every record in the install is binary, read by `SerialReadBinary` (libSimSpr
  Ghidra 0x7B738).

```
"BIN\r"
u32 1
u32 class            GZCOM class of the sprite instance (+0x1C)
u8  ×6               stored at +0x24 +0x25 +0x26 +0x23 +0x22 +0x27
u32 flags            +0x28
u16 count            +0x20
entry[count], 15 bytes each:
  u16 index
  u32 group           sprite archive group, e.g. 9 = 00000009_Landscape.DAT
  u32 instance
  i8  scale           0; n > 0 draws n times larger, n < 0 |n| times smaller
  i16 dx, dy          used by SprAttDrawRegFrame only
```

- Flora records start `5, 4, 4, 0x14, …` and have 20 entries: 5 zooms × 4 rotations.
- The entries are in order `zoom · 4 + rotation`.
  - Evidence: the sprites grow every fourth entry.
  - `cSC3CitySpriteInstAnim::Prepare` (0xA1CCC) keeps one bit per (zoom, rotation) at
    `rotation + zoom · 4`.
- `scale` is 2 in 414 of the 965 records. `SprAttDraw` multiplies or divides the image info
  by it, and draws the picture scaled.
