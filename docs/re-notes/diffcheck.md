# Checking the port against the original (`tools/diffcheck`)

Code: `tools/diffcheck/`. Rust side: `sc3k-dump diffref` and the `trace` feature of `sc3k-sim`.

The tool runs functions of the original Windows DLLs from your install and the same inputs
through the Rust port. It then reports how often they agree and where they first differ.
Nothing runs under wine and the game never starts.

```bash
pip install -r tools/diffcheck/requirements.txt
tools/diffcheck/run.py all                    # needs $SC3K_DATA; exit status 1 on any difference
tools/diffcheck/run.py rng --seeds 50         # only cRZRandom
tools/diffcheck/run.py dirt --sizes 128       # only the terrain generator
tools/diffcheck/run.py all --report docs/accuracy.md --image docs/screenshots/accuracy.png
tools/diffcheck/selftest.py                   # tests the checker itself, no game needed
```

## How it works
- **Emulator** (`emu.py`): Unicorn, 32-bit x86. `SIMDIRT.DLL` is mapped at its preferred
  base 0x10000000, so the addresses in `docs/` apply unchanged.
  - DllMain and the C runtime start-up never run. A check builds the objects it needs and
    calls the function directly.
  - Imports point at small stubs (`STUBS` in `emu.py`): `operator new`, `memset`, `_ftol`,
    `sqrt`, `_CIcos` and similar. An import without a stub stops the run and names itself.
  - `fs:` points at a zeroed TEB, for MSVC's exception frames.
  - The x87 control word is reset before every call to 0x027F: 53-bit precision, as in a
    Windows process. `--fpcw 0x007F` tries the 24-bit precision that Direct3D would set.
- **Port** (`rust.py`): `sc3k-dump diffref` replays the same calls. The `trace` feature of
  `sc3k-sim` logs every outermost `cRZRandom` call. It is off unless `Random::start_trace` is
  called, so the game never logs.
- **Addresses** (`targets.py`): copied from `docs/sim/random.md` and `docs/sim/terrain-gen.md`.

## Checks
### `rng`: cRZRandom
- For each seed, `Seed` and then 400 random calls of the seven methods.
- Arguments mix ordinary values and edge cases: `n` = 0, 0xFFFFFF, 0x1000000, 0xFFFFFFFF, and
  `hi < lo`.
- Integer results must be equal. For doubles the report also gives the largest ULP distance.

### `dirt`: cSC3DirtGenerator
- The generator object is zeroed, and its vtable is found by searching for the five slots
  0x0C..0x1C. The check then calls `Init(size, size)`, `SetDifficulty` and `GenerateRandom`.
- It reads back the sea level (`+0x10`), the altitude, water and flora `cRZCellMap<u8>`s
  (`+0x14`, `+0x18`, `+0x1C`) and the salt bit map (`+0x20`).
- Hooks on every `cRZRandom` entry point, and on the thunks at 0x1001BC38 and 0x1001BC48, log
  the outermost calls. A call made from inside the RNG code is nested and is skipped. Each
  call is compared with the port's trace: method, arguments and both states on entry.
  - The first call that differs, and the function it came from (for example
    `CreateRivers+0x1a2`), usually names the step that went wrong.
- Matrix, per seed:
  - 14 option sets at 128 × 128, easy. They cover every flag bit and extreme hills, water
    and trees.
  - The New City defaults (`0x40, 0x40, 0x40, 0x24`) at every size and difficulty.

## Assumptions to confirm on the first real run
- The salt bit map stores a pointer at `+0x10` to one array of words per column, and bit
  `y % 32` of word `y / 32` is vertex y. If only the salt map differs, check this first.
- `GenerateRandom` reaches no import beyond the stubs. A missing one fails with its name and
  caller; add a stub to `STUBS`.
- `sin`, `cos` and `pow` stubs use the host libm, like the port. A stub that differs from MSVCRT
  in the last bit would show up in `CreateFlora` positions, not in the RNG trace.

## Adding a check
1. Add the addresses to `targets.py`, with the docs as the source.
2. Give the port a way to produce the same output: a `diffref` mode in
   `tools/sc3k-dump/src/main.rs`.
3. In `checks.py`, build the object, `emu.call` the function and compare. Use `emu.hook` to
   watch calls in between.
4. Add its rows in `run.py` (`rows_for`), so the report and the README image include it.

## Self-test
`selftest.py` uses clang, lld-link and llvm-dlltool to build `fixture/fixture.c`, a
`cRZRandom` written from `docs/sim/random.md`, as a 32-bit Windows DLL. The correct build must
match the port on every call. A build with a deliberate bug in `GaussianFast` must be caught.
The self-test also covers the import stubs, x87 results, `fs:`, the trace hooks, the report
and the image.
