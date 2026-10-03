# Binary / subsystem map

All binaries are PE32 i386 built with MSVC 6 (they link `MSVCRT`, `MSVCP60`, `MSVCIRT`).
**The game DLLs do not import each other.** Each exports `GZDllGetGZCOMDirector`, and
components are wired together at runtime through Maxis' GZCOM framework (the same
framework SimCity 4 uses, so SC4 modding headers such as gzcom-dll apply).

Ghidra project: `tools/ghidra/import.sh` → symbol exports in `tools/ghidra/exports/*.tsv`.

| Binary | Size | Guessed role (from name; confirm via GZCOM class IDs) |
|---|---|---|
| `SC3U.exe` | 1.1 MB | Host app, framework startup; imports only `UV.DLL` + runtime |
| `UV.DLL` | 140 KB | Low-level utilities (only DLL the exe imports directly) |
| `GZGraphicD.dll` | 164 KB | DirectDraw graphics layer |
| `GZResourceD.dll` | 156 KB | Resource manager (IXF/TGI lookup) |
| `GZServiceD.dll` | 119 KB | Framework services |
| `GZSOUNDD.DLL` | 78 KB | Sound engine |
| `GZWIND.DLL` | 225 KB | Window/UI toolkit |
| `GZWWWD.DLL` | 180 KB | Web/HTTP (Maxis web content, `HTTPCache`) |
| `GZTOOLSD.DLL` | 61 KB | Tools |
| `AUDIO.DLL` | 184 KB | Audio item mapping (`AUDIO.INI`) |
| `GIMEX.DLL` | 119 KB | EA GIMEX image import/export |
| `SIMCITY.DLL` | 111 KB | City/top-level sim object |
| `SIMINIT.DLL`, `STRTSIM.DLL` | 250 / 233 KB | New city / startup / terrain init |
| `SIMGEOM.DLL` | 221 KB | Map geometry, terrain |
| `SIMNTWRK.DLL` | 233 KB | Transport & utility networks, tiling rules |
| `SIMBABLD.DLL` | 528 KB | Buildings, Building Architect integration |
| `SIMRCI.DLL` | 393 KB | RCI demand, zone development |
| `SIMECO.DLL` | 152 KB | Economy, budget |
| `SIMSERV.DLL` | 168 KB | City services coverage |
| `SIMDIRT.DLL` | 168 KB | Pollution / garbage / landfill |
| `SIMDSTR.DLL` | 266 KB | Disasters |
| `SIMADV.DLL` | 254 KB | Advisors, petitioners |
| `SIMMISC.DLL` | 332 KB | Misc (news ticker, neighbors, rewards?) |
| `SIMSPR.DLL` | 512 KB | Sprite engine / city view rendering |
| `SIMUI.DLL` | 848 KB | Game UI |
| `SIMUTIL.DLL` | 192 KB | Sim utilities |
| `SimTransit.dll` | 147 KB | Transit (Unlimited add-on) |
| `simvariables.dll` | 78 KB | Sim tunables |
| `SCENARIO.DLL` | 143 KB | Scenario engine (`.SNR`) |
| `MaxisAddOn.dll` | 28 KB | Add-on loader |
| `Baapp.exe` | 647 KB | Building Architect tool |

## Naming sources (first Ghidra pass, 2026-10-03)

- **No RTTI.** The only `.?AV` type descriptors are MSVC runtime classes (`exception`,
  `bad_alloc`, `logic_error`…). Class names cannot be recovered from RTTI.
- **No source paths or assert strings** in the SIM* DLLs.
- Only one exported symbol per DLL: `GZDllGetGZCOMDirector`. Functions Ghidra names are
  almost all compiler EH funclets (`Unwind@`, `Catch@`) plus runtime helpers.
- The one class-name string found so far, `cGZDBSegmentIndexedFile::DoOpenRecord()` in
  `SC3U.exe`, matches SimCity 4's `cGZDBSegment*` resource classes. The IXF container is
  the GZ "DB segment indexed file".

### Loki Linux port has C++ symbols
Loki Entertainment ported SC3U to Linux in 2000 (v2.0, patch 2.0a), built with GCC 2.95 and
split the same way as Windows: `sc3u.dynamic` + `lib/libSim*.so`, `libGZ*.so`, `libScenario.so`, ….
- A public gdb backtrace shows `cRZThread::IsValid ()` resolved inside the game binary.
- The 2.0a patch (`ftp.lokigames.twolife.be/patches/sc3u/sc3u-2.0a-x86.run`, XDelta 1.1 deltas
  against 2.0) contains GCC 2.95-mangled names in its literal data: `Init__9cGZDBFont`,
  `Parse__7cRZDate`, `GetFragment__6cRZURL`, `cRZMIFF`, `cGZBarGraph`, `cWinSC3`, `cIGZString`.
- **Confirmed with the free Loki demo** (`sc3u-demo.run`, v2.0, same engine as the full game).
  The ELFs are stripped, but GCC keeps every exported function in `.dynsym`:
  **47,182 functions, 1,862 classes (864 `cSC3*`/`cISC3*`/`cGZ*`/`cRZ*`), with full parameter types**,
  e.g. `cSC3PollutionLayer::Save(cISC3City *, cIGZDBSegment *)`.
- Reproduce: `tools/loki/fetch.sh` (download + unpack, never executes the installer) →
  `tools/loki/symbols.sh` (demangled dumps in `tools/loki/symbols/<binary>.tsv`, class index in
  `classes.tsv`) → `tools/ghidra/import.sh loki` (imports the ELFs into the project's `/loki` folder).

The Linux libraries map 1:1 onto the Windows DLLs (`libSimRCI.so` ↔ `SIMRCI.DLL`, …; see
`tools/match/pairs.txt`).

### Name porting (2026-10-03): 6,833 Windows functions named
Pipeline: `tools/match/export.sh` → `tools/match/match.py` → `ApplyNames.java` (in the Ghidra
project) → `tools/ghidra/import.sh export`. Precision report: `tools/match/check.py`.

1. `FixupVtables.java` creates functions at every vtable target. Auto-analysis missed about
   12,000 virtual methods that are only reachable through vtables.
2. `ExportFeatures.java` exports compiler-independent features: strings (ASCII/UTF-16/UTF-32,
   GCC mid-string references walked back), non-address constants, FP literals, C library calls,
   ordered call lists, vtables (Linux from `__vt_` symbols, Windows detected) and their users.
3. `match.py`: rare-token seeds (mutual best) → vtable pairing (same length and pure-virtual
   pattern, slot-size rank correlation, thunks resolved) with slot alignment → ctor/dtor via
   vtable users → call-graph gaps. Structural matches whose strings disagree are rejected.

| Method | Matches | Note |
|---|---|---|
| vtable slots | 5,046 | bulk of the names |
| rare tokens | 1,132 | seeds |
| callers / callees | 608 | |
| vtable thunks / users | 269 | adjustor thunks, constructors, destructors |

Caveats:
- Some names are platform-specific: `cGZFrameWorkSDL::*` is the SDL counterpart of Windows'
  `cGZFrameworkW95`, and Loki-specific code (`www.lokigames.com`) maps to Maxis equivalents.
- Overloaded virtuals are skipped in slot alignment (MSVC orders them differently).
- MSVC folds identical functions (`/OPT:ICF`), so one Windows address can stand for several
  Linux functions; it gets the first name.
- `SIMBABLD`, `UV` and `MaxisAddOn` have no Linux library and are matched against the Linux
  executable. Mostly statically linked framework classes (`cGZWin`, `cRZFile`, …) get named there.
- Coverage is ~10% of Windows functions. Unnamed ones are mostly non-virtual or static and
  reachable only via weak call-graph evidence. More propagation rounds can raise this.

Architecture visible from class names alone: the simulation is a set of per-cell **layers**
over `cSC3CityCellMap<T>` grids. Each layer has `Init`/`Save`/`Shutdown` and `CellChanged`/`OccupantChanged`
hooks:
| Library | Main classes |
|---|---|
| `libSimRCI` | `cSC3ZoneLayer`, `cSC3ResidentialLayer`, `cSC3LandValueLayer`, `cSC3ValveLayer` (RCI demand), `cSC3AgDeveloperRule`, `cSC3BuildingSchool` |
| `libSimEco` | `cSC3PollutionLayer`, `cSC3BuildingIncinerator`, `cSC3BuildingRecyclingCenter` |
| `libSimServ` | `cSC3PoliceLayer`, `cSC3FireLayer`, `cSC3CrimeLayer`, `cSC3FlammabilityLayer` |
| `libSimSpr` | `cSC3CityViewIso`, `cSC3CitySpriteManager`, `cSC3CitySpriteCellMap`, `cSC3DBSegmentSprite` |
| `sc3u_demo.x86` | GZ framework: `cGZApp`, `cGZAlphaBlt`, `cGZBarGraph`, `cGZDBFont`, `cRZ*` utilities |

Naming strategy, in order of payoff:
0. Loki Linux symbols (above).
1. GZCOM: each DLL's director registers class IDs, and `QueryInterface` implementations
   compare against interface IDs. Map CLSID/IID constants → vtables → classes.
2. Data loaders: follow xrefs from resource group/type constants (see `docs/formats/ixf.md`) and
   from `.ini`/`.txt` key strings (`AUDIO.INI`, `TilingRules`, `BUILDFAM` `TXT!` keys).
3. Win32/DirectDraw/MSVCP60 imports name the platform layer from the bottom up.
4. Runtime tracing under wine to tie functions to observed behaviour.

| Binary | Functions |
|---|---|
| `SC3U.exe` | 9,555 |
| `SIMUI.DLL` | 5,986 |
| `Baapp.exe` | 4,258 |
| `SIMRCI.DLL`, `SIMSPR.DLL`, `SIMBABLD.DLL` | ~3,100–3,200 each |
| all 32 game binaries | ~57,000 total |

Not analysed: `ddraw.dll` (DDrawCompat, third party), `GZGraphicD2.dll` (locally modified copy).
