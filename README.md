# OpenSC3K

An educational reverse-engineering and clean-room remake of SimCity 3000 Unlimited, written in Rust.
The original game's data is loaded at runtime from your own install. No original assets or
binaries are stored in this repository.

## Setup

```bash
export SC3K_DATA="/path/to/SimCity 3000 Unlimited"   # directory containing Apps/, Cities/, ...
cargo test                                            # install-wide tests skip if SC3K_DATA is unset
```

## Run

```bash
cargo run --release -p opensc3k                       # splash, then the main menu
cargo run --release -p opensc3k -- --res 1024x768     # other screen sizes; the window scales
cargo run --release -p opensc3k -- --screenshot menu.png --hover 251,297 --time 400
cargo run --release -p opensc3k -- --scene newcity --res 640x480 --click 200,150 --type "Jr." --screenshot nc.png
```

## Tools

```bash
cargo run -p sc3k-dump -- list "$SC3K_DATA/Apps/Res/BUILDFAM.IXF"   # TGI index of a container
cargo run -p sc3k-dump -- extract "$SC3K_DATA/Cities/Madison, WI.sc3" out/madison
cargo run -p sc3k-dump -- census                                    # TypeID histogram of the install
cargo run -p sc3k-dump -- images "$SC3K_DATA/Apps/Res/UI/Shared/MAIN.IXF" out/main   # UI images to PNG
tools/ghidra/import.sh                                              # headless Ghidra import + symbol export
tools/loki/fetch.sh && tools/loki/symbols.sh                         # Linux port symbols (C++ names)
tools/ghidra/import.sh loki                                         # import Linux ELFs for name matching
tools/match/export.sh && tools/match/match.py                       # port Linux names to Windows functions
analyzeHeadless ghidra-project SC3U -process SC3U.exe -readOnly -noanalysis \
  -scriptPath tools/ghidra -postScript Decompile.java out.c cSC3MainMenu:: @0x4faa28   # decompile by name/address/xref
```

## Layout

| Path | Purpose |
|---|---|
| `crates/sc3k-formats` | Parsers for original file formats |
| `crates/sc3k-assets` | Mounts the install; resources by (type, id), strings, fonts |
| `crates/sc3k-ui` | Software-drawn UI like the original GZ windows: title screens, main menu, New City dialog |
| `crates/opensc3k` | The game binary (winit + softbuffer window) |
| `tools/sc3k-dump` | CLI for inspecting and extracting data files |
| `tools/ghidra` | Ghidra headless import/export scripts, text symbol exports |
| `tools/loki` | Loki Linux demo fetch + demangled symbol tables (class/method names) |
| `tools/match` | Linux→Windows function matching; `names/` holds the ported names |
| `docs/formats` | File format specs |
| `docs/ui` | Recovered UI screens: layout, behaviour, source addresses |
| `docs/re-notes` | Binary/subsystem notes |
| `docs/sim` | Recovered simulation algorithms (with source addresses) |

## Roadmap

0. RE infrastructure: Ghidra project, Linux-symbol name porting, GZCOM types, runtime tracing under wine
1. File formats: IXF ✔, QFS ✔, UI images ✔, strings ✔, fonts ✔, sprites ✔, SYS.PAK ✔, TGI type registry, attribute tables, terrain, saves, audio, tiling rules
2. Asset DB and wgpu isometric renderer, plus a static `.sc3` city viewer
3. Simulation: networks, zoning/growth, utilities, RCI/economy, traffic, services, advisors, disasters
4. UI (copyright splash ✔, main menu ✔, New City dialog ✔), audio (menu music ✔), game loop, save compatibility
5. Scenarios, Building Architect, localisation, mods
