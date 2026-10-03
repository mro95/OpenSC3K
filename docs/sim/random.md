# Random numbers and cell maps

Code: `crates/sc3k-sim/src/rng.rs` and `crates/sc3k-sim/src/cellmap.rs`.

## cRZRandom
Each SIM DLL links its own copy. Addresses below are for SIMDIRT.DLL; the Loki names come from
`sc3u_demo.x86` (0x0839CEC4 to 0x0839DCD0).

**The Windows and Loki builds differ.** Loki returns the state rotated by 16 bits and keeps it
rotated; Windows keeps the plain LCG state. The Windows build is the reference.

State (8 bytes or more):

| Offset | Use |
|---|---|
| 0 | integer LCG state |
| 4 | floating-point LCG state |

### Seed (0x1001BB50)
- Sets both states to the seed.
- A seed of `0xFFFFFFFF` means "use the clock": `timeGetTime` on Windows, `clock()` on Loki.

### RandomUint32Uniform() (0x1001BB6B)
```
p      = state * 0x41C64E6D + 0x3039      (64-bit)
state  = low 32 bits of p
return bits 16..47 of p
```

### RandomUint32Uniform(n) (0x1001BBA8)
- `n` = 0: returns 0 and draws nothing.
- `n` < `0xFFFFFF`: returns `next % n`.
- Otherwise: draws until the value is at most `n`, so `n` itself is possible. Each draw masks
  the value with its own bit length (`bsr`/`shld`), which changes nothing.
- Loki differs here: its large path multiplies by 2^-31 and truncates, so it always returns 0.

### Range helpers
| Function | Address | Result |
|---|---|---|
| `RandomSint32RangeUniform(lo, hi)` | 0x1001BC23 | `lo + uniform(hi − lo)` |
| `RandomSint32RangeGaussianFast(lo, hi)` | 0x1001BC58 | see below |
| unnamed, min of two draws (`range_min_of_two`) | 0x1001BCA4 | `lo + min(uniform(hi − lo), uniform(hi − lo))` |

`RandomSint32RangeGaussianFast(lo, hi)`:
1. `half = (hi − lo) / 2`, a signed division rounding toward zero.
2. `m = min(uniform(half), uniform(half))`, compared as signed.
3. If `uniform(2)` is not 0, `m = −m`.
4. Return `lo + half + m`.

The two calls at 0x1001BC38 and 0x1001BC48 are thunks to 0x1001BC58 and 0x1001BCA4.

### Floating point
- `RandomDoubleUniform()` (0x1001BCD9):
  1. If the second state is 0, it becomes `0x12345678`.
  2. The state is multiplied by `0x278DDE6D`.
  3. The result is `state × 2^-32`. The constant at 0x10020F08 is `0x3DF0000000000007`, 7 ulp
     above 2^-32.
- `RandomDoubleRangeUniform(lo, hi)` (0x1001BD1B): `double() × (hi − lo) + lo`.
  - The original computes this in x87 extended precision. The remake uses f64, so the last
    bit can differ.

### Use in the terrain generator
The generator uses the global instance at SIMDIRT 0x10025BC0 and seeds it once at the start of
`GenerateRandom`, so a seed fixes the whole terrain. See `docs/sim/terrain-gen.md`.

`cSC3DirtGenerator` (Loki `libSimDirt.so`) calls:
- `Seed` once;
- `RandomSint32RangeUniform` (16 sites);
- `RandomSint32RangeGaussianFast` (10 sites);
- `RandomUint32Uniform(n)` (5 sites);
- `RandomDoubleRangeUniform` (1 site).

## cRZCellMap\<T\>
Loki `libSimDirt.so`: the constructor is at 0x5E974; `GetValue` and `SetValue` are at 0x5EAA4
and 0x5EA28.

| Offset | Field |
|---|---|
| 0 | `CellCountX` |
| 4 | `CellCountY` |
| 8 | `T**`: one array per x, each `CellCountY` long |
| 12 | vtable |

- The cell at (x, y) is `columns[x][y]`.
- `GetValue` and `SetValue` do no bounds checks. The remake panics on an out-of-range cell.
- `SetValue(x0, y0, x1, y1, v)` fills the inclusive rectangle.
- `SetAllCells(v)` fills every cell.
