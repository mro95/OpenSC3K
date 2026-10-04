# Flora on new maps and saved cities

The generator leaves a flora value per vertex (`terrain-gen.md`, `CreateFlora`). This page
covers how those values become tree occupants. Addresses are Loki Ghidra addresses: libSimDirt
for the dirt bag, libSimGeom for the flora layer. Port: `crates/sc3k-sim/src/flora.rs`.

## From the dirt bag to the flora layer
- **Init:** `ReflectMap` calls `cSC3DirtBag::InitFloraLevel(x, y, value)` (0x357F4). During init,
  this only stores the value in the dirt bag's flora map (+0x44).
- **SimulationBegin:** `cSC3DirtBag::SimulationBegin` (0x344F4) walks that map.
  - The loop is over cells: x outer, y inner, each from 0 to size − 1.
  - Each value of 0x20 or more calls `SetFloraDensity(cell, value >> 5)` on the flora layer
    (city vtable 0x150, flora-layer vtable 0x74). The density is 1..=7.
  - Then the map is freed.
  - Outside init, `InitFloraLevel` makes the same call at once.

## cSC3FloraLayer::SetFloraDensity (0x5C4B4)
1. An existing flora occupant on the cell is removed. Its type (`GetFloraType`) is kept.
2. If the cell `IsWater`, nothing is placed.
3. Otherwise:
   1. Pick the type with `SelectFloraType`, unless one was kept.
   2. `CreateOccupant(type, density)`.
   3. Put the occupant at (x, y) and at altitude `GetVertexAltitude(x, y)`.
   4. Insert it into the occupant manager.

## SelectFloraType (0x5C874)
`h` = `GetVertexAltitude(x, y)` − the sea level, as a byte. It uses the cell's (x, y) vertex.

| Condition | Type |
|---|---|
| wet: `IsWater` and not `IsRealWater` | 1 |
| h 1..=2 | 2 |
| h 3..=4 | 2 or 3, by `uniform(2)` |
| h 5..=7 | 3 |
| h 8..=126 | 3 or 4, by `uniform(2)` |
| otherwise | 3 |

- The random draws happen as listed even for a wet cell.
- `SetFloraDensity` returns early on water, so type 1 cannot occur on a new map.

## CreateOccupant(type, density) (0x5D5F4)
- It needs type < 5 and density < 10.
- It draws `variant = uniform(2) != 0`.
- It creates the occupant with key type `0xFFD30C03`, group `0x80F48961`, and instance
  `table[type · 20 + density · 2 + variant]`. The table is 100 `u16` at data 0x83F80
  (`flora.rs`, `OCCUPANTS`).
- `cSC3Flora::Init` (0x5D700) recovers type and density from the instance by searching the
  same table.

## Dirt-bag predicates
The vtable slots below include the two-word header, as the calls use them.

| Slot | Function | Meaning |
|---|---|---|
| 0x50 | `GetVertexAltitude` (0x4099C) | max(dirt, water) |
| 0x5C | `IsWater` (0x359A0) | some corner has water ≥ dirt |
| 0x64 | `IsRealWater` (0x35B10) | some corner has water > dirt |
| 0x68 | `GetGlobalSeaLevel` | |

## Trees of a saved city
`cSC3FloraLayer::Init(cISC3City*, cIGZDBSegment*)` (Ghidra 0x5BA84) reads the flora layer
record (`docs/formats/save.md`). Each non-zero cell goes straight to
`CreateOccupant(value >> 4, value & 0xF)`: there is no height rule and no water test. The
port is `flora::from_layer`. A type or density outside the occupant table places nothing in
the port; the original would read past the table.

## Random numbers
- `cSC3FloraLayer::StaticInit` (0x5B614) makes the layer's `cRZRandom` once per run, with
  seed `0xFFFFFFFF` (clock).
- The remake seeds it with the terrain seed for each city, so headless runs repeat.
- The Windows generator is not traced here. The calls use the ported Windows `cRZRandom`.
