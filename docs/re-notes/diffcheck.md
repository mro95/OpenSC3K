# Checking the port against the original (`tools/diffcheck`)

Code: `tools/diffcheck/`. Rust side: `sc3k-dump diffref` and the `trace` feature of `sc3k-sim`.

The tool runs functions of the original Windows DLLs from your install and the same inputs
through the Rust port. It then reports how often they agree and where they first differ, and
for every binary of the game how much of it is ported and checked. Nothing runs under wine and
the game never starts.

```bash
pip install -r tools/diffcheck/requirements.txt
tools/diffcheck/run.py all                    # needs $SC3K_DATA; exit status 1 on any difference
                                              # or on a ported function no check runs
tools/diffcheck/run.py rng --seeds 50         # only cRZRandom
tools/diffcheck/run.py dirt --sizes 128       # only the terrain generator
tools/diffcheck/run.py qfs                    # only QFS decompression (SIMBABLD.DLL)
tools/diffcheck/run.py ground                 # only saved terrains, their vertex light,
                                              # the dirt bag's queries and the bump maps
tools/diffcheck/run.py ui                     # only the main UI layout (SIMUI.DLL)
tools/diffcheck/run.py tiling                 # only the tile set reader (SIMNTWRK.DLL)
tools/diffcheck/run.py coverage               # the per-binary table; needs no install
tools/diffcheck/run.py all --apps /path/to/Apps   # DLLs from somewhere else
tools/diffcheck/run.py all --jobs 1          # one process: the cases run in order
tools/diffcheck/run.py all --report docs/accuracy.md --image docs/screenshots/accuracy.png
tools/diffcheck/selftest.py                   # tests the checker itself, no game needed
```

## How it works
- **Emulator** (`emu.py`): Unicorn, 32-bit x86. Each DLL is mapped at its preferred base
  (0x10000000, `SIMBABLD.DLL` 0x12000000), so the addresses in `docs/` apply unchanged.
  - A DLL missing from `--apps` (default `$SC3K_DATA/Apps`) skips its checks with a note.
  - A check that stops (a missing stub, a bad memory access) becomes a failed row with the
    error; the other DLLs still run.
  - DllMain and the C runtime start-up never run. A check builds the objects it needs and
    calls the function directly.
  - Imports point at small stubs (`STUBS` in `emu.py`): `operator new`, `memset`, `_ftol`,
    `sqrt`, `_CIcos`, `Interlocked*` and similar. An import without a stub stops the run and
    names itself.
    - Most stubs are Python, called from a code hook on the stub's own hook point.
    - The hot ones are x86 code that runs in the emulator, so no Python runs per call:
      `_ftol` (a `fistp` with the rounding mode set to chop, as MSVC's), `abs` and `isdigit`.
      The saved-terrain check calls `_ftol` once per vertex, and the terrain generator calls
      `abs` about 175,000 times for a 128-cell terrain.
    - The string stubs (`strtok`, `strcpy`, `atoi`) read memory 64 bytes at a time.
  - Interfaces from other DLLs (a city, a DB segment, a record) are fake objects
    (`Emu.fake_object`): a vtable whose slots call Python. A slot the fake does not implement
    stops the run, naming the object and the slot, so a missing method is never silent.
  - `fs:` points at a zeroed TEB, for MSVC's exception frames.
  - The x87 control word is reset before every call to 0x027F: 53-bit precision, as in a
    Windows process. `--fpcw 0x007F` tries the 24-bit precision that Direct3D would set.
- **Port** (`rust.py`): `sc3k-dump diffref` replays the same calls. The `trace` feature of
  `sc3k-sim` logs every outermost `cRZRandom` call. It is off unless `Random::start_trace` is
  called, so the game never logs.
- **Addresses** (`targets.py`): copied from `docs/sim/random.md`, `docs/sim/terrain-gen.md`
  and `tools/match/names`. `TARGETS` lists each DLL with its checks.

## Speed
`run.py all` takes about a minute and a half on 32 cores.
- **Cases in parallel** (`pool.py`): the terrain cases, the saved terrains and the tiling
  files are independent, so they run in forked worker processes, one emulator per case (one
  per worker for tiling). `--jobs` sets the number of workers; the default is all cores.
  - Workers send back the addresses their emulators ran, so the coverage table counts the
    same functions as a run with `--jobs 1`.
  - Results come back in case order, so the output and the report do not depend on `--jobs`.
- **Measured costs before this:** the saved terrains took 171 s, almost all of it in the
  Python `_ftol` and `acos` stubs. The tiling check took 48 s in `isdigit` and byte-by-byte
  memory reads. Profile with `python -m cProfile -s tottime tools/diffcheck/run.py <check>`.

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

### `qfs`: cRZFastCompression3 (`SIMBABLD.DLL`)
- The class is the QFS/RefPack codec. Its methods are named by `tools/match` from the Loki demo
  (`sc3u_demo.x86`); the vtable is found from its first three slots (QueryInterface, AddRef,
  Release) and put on a zeroed object.
- `GetLengthOfDecompressedData`, then `DecompressData(src, len, dst, &dst_len)` with `dst_len`
  set to that length. The port's side is `sc3k-dump diffref qfs`.
- Three sources, one row each. A stream matches when both decoders give the same bytes, or
  both reject it.
  - **Install**: up to `--qfs-streams` (500) streams from the containers under `$SC3K_DATA`,
    spread over all of them (`sc3k-dump diffref qfs-samples`): whole records, image pixels
    and sprite pixels.
  - **Round trip**: zeros, text, noise with repeats and a 200 KB buffer with far repeats,
    compressed by the original `CompressData`, so the game's own encoder picks the opcodes.
  - **Edge cases**: streams assembled in `qfs.py` with every opcode form, overlapping copies,
    the farthest offsets and longest copies, and the compressed-size header field. These
    test the decoder beyond what the game's files use. They leave out 4-byte sizes (flag
    0x80): the original decoder (0x1205A012) ignores the flag, always reads a 3-byte size and
    runs off the buffer.
  - `cRZFastCompression3` keeps each stream behind a 4-byte little-endian size of the whole
    block, prefix included. `DecompressData` and `GetLengthOfDecompressedData` skip it and
    `CompressData` writes it; `qfs.py` adds and strips it, so the port sees bare streams.

### `ground`: cSC3DirtBag::Init(city, segment)
- Every `.sct` in `$SC3K_DATA/Cities/Terrains` and every `.sc3` in `$SC3K_DATA/Cities`.
  `sc3k-dump diffref ground` writes the dirt bag's record as stored and the port's reading.
- **The object:** the check builds the dirt bag the way `Init(cISC3City*)` (0x10003E50)
  leaves it:
  - the real vtable;
  - the altitude, light and water cell maps;
  - the blocked-cell words;
  - the ready flag;
  - a fake `cSC3CityChangeSender`.
  The static `cRZCriticalSection` gets a fake vtable, since static construction never ran.
  Addresses and offsets are in `targets.py` (`dirt_bag`).
- **The call:** `Init` at 0x10004A00 gets a fake city (cell counts, version) and a fake
  segment. The segment's `OpenRecord` checks the key and returns a fake serial record, which
  serves the fields from the record bytes in the order the original asks for them; marker
  strings are skipped. `Init` then computes the vertex light (0x10007010).
- **Compared:**
  - the altitude, water and light maps, vertex by vertex;
  - the sea level;
  - whether the original read the whole record.
- **Queries:** on the loaded dirt bag, `GetVertexAltitude` (0x10005A70) for every vertex and
  `IsWater` (0x100065F0) and `IsRealWater` (0x10006710) for every cell, in every fourth
  column (`ground.STRIDE`), against `sc3k_sim::flora`. Every column would take the check
  from 8 to 23 seconds. `diffref ground` writes the port's answers for all of them.
- **Bump maps:** `GenerateBumpMaps` (0x100128E1) runs once and fills the two 1024-byte tables
  at 0x100252D0 (land) and 0x10025740 (water), against `sc3k-dump diffref bump 0`. Its
  `cRZRandom` is seeded with the clock, and the `timeGetTime` stub returns 0.
- Windows slot numbers of the interfaces are in `docs/formats/save.md`. The dirt bag's
  vtable (0x1002046C) has the Loki slots in the Loki order, except that overloads come in
  another order (`Init`, `SetVertexAltitude`, `SetWaterTable`, `InCellBounds`, ...).

### `ui`: the main UI layout (`SIMUI.DLL`)
- Three functions from `docs/ui/main-ui.md`, on one emulator. Fakes are made once and their
  answers set per query, since the fake-method region holds about 128 objects.
  - **`cSC3MainUIMgr::place_windows`** (0x100148D1):
    - **Fakes:** a main window that reports its area and finds its four children by ID, and
      four child windows that report their size and record the `Move` and `SetArea` calls.
      The window manager it asks first is a fake put in its cached global (0x100BFABC).
    - **Inputs:** 11 screen sizes, from 640x480 to 2560x1440, with the port's window sizes
      for each. Each size runs with all four windows, then with each one missing.
  - **`cSC3WinMenuBtnMain::get_menu_btn_info_main`** (0x1004C3E9): button IDs 0x1000..0x100A
    at 14 screen heights, including 479, 599 and 601. The fake button reports its ID and a
    parent with the height.
    - **Compared:** the area, the hover image and its group, the hover-while-open image, the
      submenu image, its group and its offset.
    - **Ignored:** the offset of a button without a submenu image, since it is never used.
  - **`cSC3WinDateCashTitle::get_layout_info`** (0x100270E5): the art group for 16 screen
    widths. The text field positions are not compared yet, since the port does not use them.
- `sc3k-dump diffref ui` answers the same queries from `sc3k_ui::main_ui`. A query matches
  when both answers are equal, including both refusing.
- Window slots are in `targets.py` (`ui`). Windows `cIGZWin` slots are the Loki ones minus 8
  from 0x80 on.

### `tiling`: the tile set reader (`SIMNTWRK.DLL`)
- **`cSTTransitLayer::GrokFileTileSet`** (0x1001746E): static stdcall
  `(cRZFile*, char* buffer, vector<cGZResourceKey>*)`, returning a bool in AL.
  - **Fake file:** a `cRZFile` that serves the input bytes. The read helper at 0x10017596
    calls slot 0x54 (no arguments), `0x0C(1, 2, 1)`, `0x3C(buffer, &length)` with the length
    going in as 0x1FFFF, and `0x14`. These are the Loki slots minus 8.
  - **Read back:** the vector's begin and end at +0 and +4. Each 12-byte key must be
    `E223741F-A317745F-<id>`.
  - **Inputs:** every file under `TilingRules`, including the rule, convert and protected
    files, and hand-made edge cases: braces, signs, NUL, `\v`/`\f`/`\r`, bytes from 0x80,
    numbers past 2^31, and texts longer than the 0x1FFFF-byte buffer.
- **Imports:** `strtok`, `isdigit`, `atoi` and `strcpy` from `MSVCRT.DLL` are stubs.
  - `isdigit` gets a sign-extended `char`. MSVCRT would read outside its table for bytes from
    0x80, and the stub treats them as not digits.
  - `atoi` follows MSVCRT's `atox.c`: there is no overflow check, so the total wraps at
    32 bits. The Loki build reads with `strtol`, which clamps instead. The two builds give
    different ids for numbers past 2^31; `ROAD_Exits.txt` has one.
- `sc3k-dump diffref tiling` reads the same files with `parse_tile_set`. A file matches when
  both give the same ids in the same order.
- The convert, protected and rule readers are not checked yet: their Windows addresses are
  not known.
- **Sensitivity:** changing one constant in each of the three port functions makes its row
  fail.

### `coverage`: every binary
- **Functions**: the `func` rows of `tools/ghidra/exports/<binary>.tsv` plus the vtable-only
  functions in `tools/match/names`, without the `Unwind@` / `Catch@` funclets.
- **Ported**: functions the Rust code cites in its doc comments (`coverage.py` reads them).
  - A Windows address (`SIMDIRT.DLL 0x1001BB50`, `SC3U.exe+0x3a8b9`) counts when it is a
    function start, so cited tables and constants do not.
  - A Loki address (`libSimDirt 0x4142C`, or bare in a file that names its library) is looked
    up in `tools/loki/symbols`; when a function name is quoted just before it, the symbol must
    have that name. Its Windows address comes from `tools/match/names` by C++ name. Without
    one it still counts as ported, but cannot be checked.
  - Loki addresses are Ghidra's: a library's is its `tools/loki/symbols` address + 0x10000,
    the executable's (`sc3u_demo`) is the same as there.
  - A citation's parentheses may run over several doc comment lines.
  - Cite new ported code the same way, or it is not counted.
- **Checked**: ported functions at a known address that ran during the checks (one-shot hooks,
  `Emu.once`).
- **Exempt**: ported functions no check runs, whose doc comment says why: `Unchecked:` and a
  reason, anywhere in the doc comment block (consecutive `///` or `//!` lines) that cites them.
  The marker covers every citation in its block. `Unchecked:` with no reason does not count.
- **The ratchet**: after `run.py all`, a ported function that is neither checked nor exempt
  fails the run, with its `file:line`. Give it a check, or an `Unchecked:` reason.
  - Binaries whose DLL was not found are skipped, since none of their checks ran. Partial runs
    (`run.py dirt`, ...) and `run.py coverage` do not apply it.
  - A marker all of whose functions a check now runs is reported as one that can go; it does
    not fail the run.
  - `grep -rn 'Unchecked:' crates` lists what is still open.
- **Accuracy** = mean match rate of the binary's checks × checked / functions. A binary nothing
  has been ported from yet is at 0% and marked "not implemented yet"; it is listed in the
  report and the image but does not fail the run.

## Confirmed on the real `SIMDIRT.DLL`
- The salt bit map layout above (bit `y % 32` of word `y / 32`, least significant bit first).
- `GenerateRandom` needs `_EH_prolog` (native code, it rewrites the caller's frame), `abs`,
  `acos` and `strlen` beyond the first stubs.
- All checks match with the 53-bit control word 0x027F.
- `sin`, `cos`, `acos` and `pow` stubs use the host libm, like the port. A stub that differed
  from MSVCRT in the last bit would show up in `CreateFlora` positions, not in the RNG trace.

## To confirm on the first real `SIMBABLD.DLL` run
- `DecompressData` takes the output capacity in `dst_len` and returns a bool in AL. If every
  stream is rejected by the original, check this first.
- `CompressData` and `DecompressData` reach no import beyond the stubs; a missing one stops the
  QFS check with its name.

## Confirmed by the `ground` check
- The dirt bag record layout in `docs/formats/save.md`, on all 35 saved terrains.
- The vertex light, once the port used the float π and the original's float rounding
  (`docs/render/terrain.md`).
- `GetVertexAltitude`, `IsWater` and `IsRealWater` (`sc3k_sim::flora`), and the bump maps
  (`docs/render/terrain.md`).
- Windows inlines `getVertAltForLightCalc` into `calculateAndSetVertexLight`, so the vertex
  light covers it.

## Confirmed by the `ui` check
- The window placement, the main button table and the bar's art group in
  `docs/ui/main-ui.md`, at every screen size tried.

## Not checked yet
- `cSC3WinMenuPanelMain::SetArea` and `Init`, the panel's pieces and their placement: their
  Windows addresses are not located. Also the `MenuItem.INI` parsers.
- The compressed segment reader (`sc3k_formats::segment`). Its Windows code is in
  `GZResourceD.dll` but not located function by function; the `ground` check starts from the
  record bytes the port extracts.
- The flora layer record (`cSC3FloraLayer::Init`, SIMGEOM.DLL, not located on Windows), and
  `cSC3DirtBag::SimulationBegin` (0x10005230), which fills it.
- The dirt clods: `GetDirtClod` (0x10005B10) and `cSC3DirtClodLand::getColor` (0x1001309E)
  need a `cSC3DirtClodFactory` with its palettes; `Init_FromBuffer` (0x10012342) needs a
  fake `cIGZBuffer`.
- Everything ported from Loki addresses with no Windows match (`SIMINIT`, `SIMCITY`, most of
  the render code): `run.py coverage` lists them as ported, not checked.

## Adding a check
1. Add the addresses to `targets.py`, with the docs as the source.
2. Give the port a way to produce the same output: a `diffref` mode in
   `tools/sc3k-dump/src/main.rs`.
3. In `checks.py`, build the object, `emu.call` the function and compare. Use `emu.hook` to
   watch calls in between.
4. Add its rows in `run.py` (`rows_for`), so the report and the README image include it.
5. For a new DLL, add a `Target` to `TARGETS` with its `checks` and run them from
   `run_target`. `qfs` in `SIMBABLD.DLL` is the smallest example.

## Self-test
`selftest.py` uses clang, lld-link and llvm-dlltool to build `fixture/fixture.c` as a 32-bit
Windows DLL: a `cRZRandom` written from `docs/sim/random.md`, and a `cRZFastCompression3`
look-alike with a QFS decoder written from `docs/formats/qfs.md`. The correct build must match
the port on every call and stream. A build with deliberate bugs in `GaussianFast` and in the
`C0–DF` offset must be caught. The self-test also covers the import stubs, x87 results, `fs:`,
the trace hooks, the coverage table, the report and the image.
The self-test also runs the ratchet on a small crate it writes: a function without a reason
fails, one with a reason passes, and a marker over checked functions is reported.
`Emu.fake_object`, the `ground` check and the `ui` check are not covered by the self-test yet.
