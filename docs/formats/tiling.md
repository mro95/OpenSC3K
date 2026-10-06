# Network tiling rules

Text files in `Res/TilingRules/` that tell the transport and utility networks (`SIMNTWRK.DLL`)
which tile to put in a cell given its neighbours. This page covers how the files are read, not
how the rules are applied; for that see `../sim/transit.md`.

Addresses are Loki Ghidra addresses. The readers of `cSTTransitLayer` are in libSimNtwrk; the
rule classes (`cTileEvaluator`, `cTilingFamily`, `cTileRule`, …) are linked into the executable
(`sc3u_demo.x86`). In the Ghidra project, libSimNtwrk addresses are the
`tools/loki/symbols/libSimNtwrk.so.tsv` address + 0x10000; executable addresses match the TSV.

## Files

One set per network. The prefix is `ROAD`, `HWAY`, `RAIL`, `SUBW`, `POWR` or `PIPE`, and the
middle part is always `_GRND_`.

| File | Reader | Contents |
|---|---|---|
| `<NET>_GRND_Set.txt`, `DIAG_Set.txt` | `GrokFileTileSet` (0x77CF4) | tile set |
| `<NET>_GRND_Convert.txt`, `<NET>_GRND_Complex_Convert.txt` | `GrokFileConvertSets` (0x8FBE4) | convert set |
| `<NET>_GRND_Protected.txt`, `<NET>_GRND_Bridges.txt`, `Collapse.txt` | `GrokFileProtectedSets` (0x9003C) | protected set |
| `<NET>_GRND_SimpleRules.txt`, `<NET>_GRND_ComplexRules.txt`, `<NET>_GRND_final.txt` | `cTileEvaluator::AddFamiliesFromFile` (exe 0x0823FD70) | rule family |

- **Name case:** the case of the names varies between the DLL and the install, for example
  `Road_GRND_Protected.txt` in the DLL and `ROAD_GRND_Protected.txt` on disk, and
  `HWAY_GRND_COMPLEXRULES.txt` beside `ROAD_GRND_ComplexRules.txt`. Open them case-insensitively.
- **Bridges:** the DLL names a `_Bridges` file for `ROAD`, `HWAY`, `RAIL` and `POWR`.
- **Not loaded:** the install also has `*_SlopeRULES.txt`, `*_Exits.txt`,
  `ROAD_GRND_FinalRules.txt`, `networkIntesection.txt` and `LandfillRules*.txt`.
  `SIMNTWRK.DLL` names none of them.
  - `ROAD_GRND_FinalRules.txt` is written in a brace syntax. The rule reader would ignore
    every token in it.
  - The `SlopeRULES` files use the rule grammar below and parse cleanly with it.

## Lexing
All readers share it:
- **Buffer:** the file is read into a zeroed 0x20000-byte buffer, at most 0x1FFFF bytes.
- **Tokens:** `strtok` with the delimiters space, `,`, `\n` and `\t`. `{`, `}` and `\r` are not
  delimiters.
- **Numbers:** decimal `strtol`, which stops at the first non-digit, so `88}\r` reads as 88.
  - The Windows `GrokFileTileSet` (`SIMNTWRK.DLL` 0x1001746E) uses `atoi` instead. MSVCRT's
    `atoi` does not check for overflow and wraps at 32 bits, where `strtol` clamps to
    0x7FFFFFFF. No file the DLL loads has a number that large; `ROAD_Exits.txt` does.

### Packed tile values
Convert sets and rules store a tile and its rotation in one number. `cTileID::Convert`
(exe 0x0823FEE0) splits it:

```
id  = v >> 8
rot = v & 0xFF
```

The reverse (0x0823FEFC) is `id << 8 | rot`. Example: `2873603` is tile 11225, rotation 3.

## Tile set
`GrokFileTileSet` fills a `vector<cGZResourceKey>`.
- **Tokens:** each one has its leading non-digits skipped and is cut at the first non-digit after
  that. A token that then starts with a digit is one entry. So `{29,` and `18070}` both count.
- **Entry:** the occupant key `E223741F-A317745F-<n>`.

`ROAD_GRND_Set.txt` lists 108 tiles.

## Convert set
`GrokFileConvertSets` fills a `vector<tSTTileConvert>`. A `tSTTileConvert` is 16 bytes:
`u32 from_id, u8 from_rot, u32 to_id, u8 to_rot`, each split with `cTileID::Convert`.

```
count
from,to
...
```

- **Count:** read, but the loop runs to the end of the file whatever it says.
- **Empty slots:** the reader treats a value of 0 as an empty slot:
  - A count of 0 makes the next token the count.
  - A pair is stored after its `to` is read. A `from` whose id is 0 makes the next token the
    `from` again. A `to` whose id is 0 is stored and starts a new pair.
  - No file in the install hits these cases.

## Protected set
`GrokFileProtectedSets` fills a `vector<u32>`: a count, then plain tile ids (not packed). It also
reads the bridge sets (`mp<Net>Bridge`) and `Collapse.txt` (`mpCollapse`).
- **Count:** a count of 0 makes the next token the count, as in the convert set. The count does
  not limit the loop, which runs to the end of the file.
- **Ids:** every token after the count is pushed, including ones that read as 0.

## Rule family
`AddFamiliesFromFile` makes a `cPersistantTilingFamily` (constructor 0x0823EB34) and calls
`Read` (0x0823EEC0). `Read` passes each token to `parseToken` (0x0823F194), which ignores any
token whose first character is not a digit.

### Grammar
The file is a sequence of `tag,value…` groups. Tag 0 sets the state from the next number:

| Group | State | Effect |
|---|---|---|
| `0,n` | 1 | rule count (+0x10). Stored; it equals the parsed count in every file. |
| `1,k` | 2 | key of the current tiling set |
| `2,n` | 3 | condition count (+0x14). Stored, never checked. |
| `3,pos,v` | 4, 5 | add a condition `(id, pos, rot)` to the current rule |
| `4,n` | 6 | solution count (+0x18) |
| `5,pos,v` | 7, 8 | add a solution `(id, pos, rot)` to the current rule |

- **Unknown tags:** a tag other than 0–5 is ignored.
- **`pos`:** it is truncated to a byte. `255` (−1) becomes `0x1F`.
- **Defaults:** an identity starts as id 0, pos `0x1F`, rot 0, and is reset to that after each
  add.

When the number of solutions reaches the solution count:
1. The rule is copied into the current tiling set.
2. The condition and solution lists are cleared.
3. The current set, now holding one rule, is copied into the family.
4. The set is cleared, which sets its key back to 0.

So each rule forms its own set, and each needs its own `1,k` line. Every file in the install
has one.

### Structures

| Class | Layout |
|---|---|
| `cTileIdentity` | 6 bytes: `u32 id`, `u8 pos`, `u8 rot` |
| `cTileRule` | 24 bytes: conditions and solutions, each a `cTileIdentityArray` (`vector<cTileIdentity>`) |
| `cTilingSet` | 16 bytes: `u32 key`, `vector<cTileRule>` |
| `cTilingFamily` | `vector<cTilingSet>` |

### Keys
`cTilingFamily::Lookup` (0x082405E8) tries the sets in order:
- A set takes part if its key is `0x100` or equals the neighbourhood's first field.
- Within a set, it tries `cTileRule::Match` (0x0823E824) on each rule until one matches.
- The first match ends the lookup.

### Positions
`pos` indexes the cells of a 5×5 neighbourhood. Rows are the x offset −2..2 from top to bottom,
columns the y offset −2..2 from left to right (x and y as in a `cSC3CityCoord`):

```
        y−2 y−1  y  y+1 y+2
x−2      10   9   8  23  22
x−1      11   4   0   7  21
x        12   1  31   3  20
x+1      13   5   2   6  19
x+2      14  15  16  17  18
```

- **Groups:**
  - 31 (0x1F) is the cell itself.
  - 0–3 are the edge neighbours: x−1, y−1, x+1, y+1.
  - 4–7 are the corner neighbours: (x−1, y−1), (x+1, y−1), (x+1, y+1), (x−1, y+1).
  - 8–23 are the outer ring.
- **Forward table:** `cTilePos::ConvertRelativeToMeta(a, b)` (0x0823FE48) reads the table at
  data 0x08582B60 at index `a + 5·b + 12`. Its callers pass `a` = the y offset and `b` = the x
  offset (`InitTileIdentity`, `CreateNeighborhood`).
- **Inverse table:** `ConvertMetaToRelative` (0x0823FE80) reads (x, y) pairs at 0x08582B80, and
  `SolveNetwork` adds them to x and y in that order.

### Contents in the install

| File | Rules | Keys | Positions used | Max solutions |
|---|---|---|---|---|
| `ROAD_GRND_SimpleRules.txt` | 430 | per-set | 0–7, 31 | 1 |
| `ROAD_GRND_ComplexRules.txt` | 195 | all `256` | 0–23 except 10, 14, 18, 22; 31 | 4 |
| `ROAD_GRND_final.txt` | 16 | 0–15 | 31 | 1 |
| `HWAY_GRND_COMPLEXRULES.txt` | 673 | all `256` | 0–23, 31 | 14 |
| `PIPE_GRND_COMPLEXRULES.txt`, `RAIL_GRND_COMPLEXRULES.txt` | 0 | | | |

The `final` files map each key 0–15 to one tile. The key is the 4-bit mask of connected edge
neighbours, bit n for meta position n (`../sim/transit.md`, neighbourhoods). For example:
- key 5 (x−1 and x+1) gives straight road tile 29, rotation 0;
- key 10 (y−1 and y+1) gives tile 29, rotation 1;
- key 15 gives the crossing, tile 43.
