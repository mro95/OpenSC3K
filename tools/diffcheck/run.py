#!/usr/bin/env python3
"""Check the port against the original game code.

Runs functions of the original DLLs from your install in an x86 emulator and the same inputs
through the Rust port (`sc3k-dump diffref`), then compares the results:

  rng       SIMDIRT.DLL cRZRandom: every method, random and edge-case arguments, many seeds
  dirt      SIMDIRT.DLL cSC3DirtGenerator::GenerateRandom: the four vertex maps, the sea
            level and the sequence of cRZRandom calls, over seeds, sizes and options
  qfs       SIMBABLD.DLL cRZFastCompression3: QFS streams from the install, round trips
            through the original compressor, and hand-made edge cases
  ground    SIMDIRT.DLL cSC3DirtBag::Init(city, segment): the saved terrain of every .sct and
            .sc3 under $SC3K_DATA/Cities, and the vertex light computed from it
  ui        SIMUI.DLL main UI layout: place_windows, get_menu_btn_info_main and
            get_layout_info over many screen sizes
  tiling    SIMNTWRK.DLL GrokFileTileSet: every file under $SC3K_DATA/Apps/Res/TilingRules
            and hand-made edge cases
  coverage  every binary of the game: functions, how many are ported, how many of those a
            check ran, and an accuracy that is 0% where nothing is ported yet

`all` runs everything. Exits 0 when every check matches, 1 on any difference; binaries that
are not ported yet do not fail the run. See docs/re-notes/diffcheck.md.

  tools/diffcheck/run.py all --seeds 5 --report docs/accuracy.md --image docs/screenshots/accuracy.png
"""

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import checks  # noqa: E402
import coverage  # noqa: E402
import ground  # noqa: E402
import pool  # noqa: E402
import qfs  # noqa: E402
import rust  # noqa: E402
import tiling  # noqa: E402
import ui  # noqa: E402
from emu import WINDOWS_FPCW, Emu, EmuError  # noqa: E402
from targets import TARGETS  # noqa: E402

RNG_NAMES = {
    "next_u32": "RandomUint32Uniform()",
    "uniform": "RandomUint32Uniform(n)",
    "range": "RandomSint32RangeUniform",
    "gaussian_fast": "RandomSint32RangeGaussianFast",
    "range_min_of_two": "min of two draws (0x1001BCA4)",
    "double": "RandomDoubleUniform",
    "double_range": "RandomDoubleRangeUniform",
}
QFS_SECTION = "QFS decompression"
GROUND_SECTION = "Saved terrain"
GROUND_NAMES = {"altitude": "Altitude per vertex", "water": "Water per vertex",
                "light": "Vertex light (calculateAndSetVertexLight)",
                "vertex_altitude": "GetVertexAltitude per vertex",
                "is_water": "IsWater per cell", "is_real_water": "IsRealWater per cell"}
BUMP_NAMES = {"land": "Land bump map (GenerateBumpMaps)",
              "water": "Water bump map (GenerateBumpMaps)"}
UI_SECTION = "Main UI layout"
UI_NAMES = {"place_windows": "Window placement (place_windows)",
            "get_menu_btn_info_main": "Main buttons (get_menu_btn_info_main)",
            "get_layout_info": "Date bar art (get_layout_info)"}
TILING_SECTION = "Tiling rules"
TILING_NAMES = {"install": "Tile sets (GrokFileTileSet), install files",
                "edge cases": "Tile sets (GrokFileTileSet), edge cases"}
QFS_NAMES = {"install": "Streams from the install", "round trip": "Original compressor output",
             "edge cases": "Hand-made edge cases"}


def rows_for(rng_stats, dirt_cases, qfs_stats=None, failed=None, ground_cases=None,
             ui_stats=None, tiling_stats=None, bump=None):
    """[(section, label, rate, detail)]. `failed`: section -> error, for a check that stopped."""
    rows = []
    if rng_stats:
        for op, s in rng_stats.items():
            detail = f"{s.calls} calls"
            if s.max_ulps:
                detail += f", worst {s.max_ulps} ulp"
            rows.append(("cRZRandom", RNG_NAMES[op], s.rate(), detail))
    if dirt_cases:
        n = len(dirt_cases)
        mean = lambda m: sum(c.equal[m] for c in dirt_cases) / n  # noqa: E731
        detail = f"{n} case{'s' if n != 1 else ''}"
        rows += [
            ("Terrain generator", "Identical terrains", sum(c.ok() for c in dirt_cases) / n, detail),
            ("Terrain generator", "Sea level", sum(c.sea[0] == c.sea[1] for c in dirt_cases) / n,
             detail),
            ("Terrain generator", "RNG call sequence",
             sum(not c.first_call for c in dirt_cases) / n, detail),
        ]
        rows += [("Terrain generator", f"{m.capitalize()} per vertex", mean(m), "all vertices")
                 for m in checks.MAPS]
    for source, s in (qfs_stats or {}).items():
        rows.append((QFS_SECTION, QFS_NAMES[source], s.rate(),
                     f"{s.streams} streams, {s.byte_rate() * 100:.2f}% of bytes"))
    if ground_cases:
        n = len(ground_cases)
        detail = f"{n} file{'s' if n != 1 else ''}"
        rows += [
            (GROUND_SECTION, "Identical terrains", sum(c.ok() for c in ground_cases) / n, detail),
            (GROUND_SECTION, "Sea level", sum(c.sea[0] == c.sea[1] for c in ground_cases) / n,
             detail),
            (GROUND_SECTION, "Whole record read", sum(not c.unread for c in ground_cases) / n,
             detail),
        ]
        rows += [(GROUND_SECTION, label, sum(c.equal[m] for c in ground_cases) / n,
                  f"cells of every {ground.STRIDE}th column" if m in ground.CELL_MAPS
                  else f"vertices of every {ground.STRIDE}th column" if m in ground.QUERIES
                  else "all vertices")
                 for m, label in GROUND_NAMES.items()]
    if bump:
        rows += [(GROUND_SECTION, BUMP_NAMES[m], bump.equal[m], "1024 bytes")
                 for m in BUMP_NAMES]
    for f, s in (ui_stats or {}).items():
        rows.append((UI_SECTION, UI_NAMES[f], s.rate(), f"{s.queries} queries"))
    for src, s in (tiling_stats or {}).items():
        rows.append((TILING_SECTION, TILING_NAMES[src], s.rate(), f"{s.inputs} files"))
    for section, error in (failed or {}).items():
        rows.append((section, "Check stopped", 0.0, error[:70]))
    return rows


def dll_detail(r):
    """One short line for the image."""
    if r.status() == "not implemented yet":
        return "not implemented yet"
    if r.note:
        return f"{r.ported}/{r.functions} ported, {r.note}"
    return f"{r.ported}/{r.functions} ported, {r.checked} checked"


def report(path, rows, rng_stats, dirt_cases, footer, dll_rows=None, qfs_stats=None,
           ground_cases=None, ui_stats=None, tiling_stats=None):
    out = ["# Decomp accuracy", "",
           "Generated by `tools/diffcheck/run.py`: the original code runs in an x86 emulator and is",
           "compared with the port on the same inputs. See `docs/re-notes/diffcheck.md`.", "",
           f"{footer}.", ""]
    if dll_rows:
        total = sum(r.functions for r in dll_rows)
        checked = sum(r.checked for r in dll_rows)
        ported = sum(r.ported for r in dll_rows)
        exempt = sum(r.exempt for r in dll_rows)
        out += ["## Per binary", "",
                "Accuracy = match rate of the binary's checks × checked functions / all its",
                "functions. A binary nothing has been ported from yet is at 0%. Exempt: ported",
                "functions whose doc comment gives a reason no check runs them (`Unchecked:`).", "",
                f"All binaries: {ported} of {total} functions ported, {checked} checked, "
                f"{exempt} exempt.", "",
                "| Binary | Functions | Ported | Checked | Exempt | Checked code matches | Accuracy "
                "| Status |",
                "|---|---:|---:|---:|---:|---:|---:|---|"]
        for r in dll_rows:
            match = f"{r.match * 100:.2f}%" if r.match is not None else "–"
            out.append(f"| {r.binary} | {r.functions} | {r.ported} | {r.checked} | {r.exempt} | "
                       f"{match} | {r.accuracy() * 100:.2f}% | {r.status()} |")
        out.append("")
    if rows:
        out += ["## Checks", "", "| Check | | Match | |", "|---|---|---:|---|"]
        out += [f"| {sec} | {label} | {rate * 100:.2f}% | {detail} |"
                for sec, label, rate, detail in rows]
    firsts = [(RNG_NAMES[op], s.first) for op, s in (rng_stats or {}).items() if s.first]
    if firsts:
        out += ["", "## First cRZRandom difference per method", ""]
        out += [f"- **{name}**: {first}" for name, first in firsts]
    firsts = [(UI_NAMES[f], s.first) for f, s in (ui_stats or {}).items() if s.first]
    if firsts:
        out += ["", "## First main UI difference per function", ""]
        out += [f"- **{name}**: {first}" for name, first in firsts]
    firsts = [(TILING_NAMES[src], s.first) for src, s in (tiling_stats or {}).items() if s.first]
    if firsts:
        out += ["", "## First tiling rule difference per source", ""]
        out += [f"- **{name}**: {first}" for name, first in firsts]
    firsts = [(QFS_NAMES[src], s.first) for src, s in (qfs_stats or {}).items() if s.first]
    if firsts:
        out += ["", "## First QFS difference per source", ""]
        out += [f"- **{name}**: {first}" for name, first in firsts]
    bad = [c for c in dirt_cases or [] if not c.ok()]
    if bad:
        out += ["", f"## Terrain cases that differ ({len(bad)} of {len(dirt_cases)})", "",
                "| Case | Sea level | Altitude | Water | Flora | Salt | First difference | First RNG call that differs |",
                "|---|---|---:|---:|---:|---:|---|---|"]
        for c in bad[:50]:
            eq = " | ".join(f"{c.equal[m] * 100:.2f}%" for m in checks.MAPS)
            out.append(f"| {c.label()} | {c.sea[0]} / {c.sea[1]} | {eq} | {c.first_vertex} | "
                       f"{c.first_call} |")
        if len(bad) > 50:
            out.append(f"\n{len(bad) - 50} more not shown.")
    bad = [c for c in ground_cases or [] if not c.ok()]
    if bad:
        out += ["", f"## Saved terrains that differ ({len(bad)} of {len(ground_cases)})", "",
                "| File | Sea level | Altitude | Water | Light | GetVertexAltitude | IsWater "
                "| IsRealWater | Bytes unread | First difference |",
                "|---|---|---:|---:|---:|---:|---:|---:|---:|---|"]
        for c in bad:
            eq = " | ".join(f"{c.equal[m] * 100:.2f}%" for m in GROUND_NAMES)
            out.append(f"| {c.file} | {c.sea[0]} / {c.sea[1]} | {eq} | {c.unread} | {c.first} |")
    Path(path).write_text("\n".join(out) + "\n")


def run_target(target, selected, path, args, exe, seeds, executed, located):
    """Runs the selected checks of one DLL:
    (rng_stats, dirt_cases, qfs_stats, failed, ground_cases, ui_stats, tiling_stats, bump)."""
    rng_stats = dirt_cases = qfs_stats = ground_cases = ui_stats = tiling_stats = bump = None
    failed = {}
    def make_emu():
        return coverage.watch(Emu(path, fpcw=args.fpcw), located, executed)
    make_emu.seen = executed            # merged back from pool workers
    if "rng" in selected:
        try:
            rng_stats = checks.check_rng(make_emu(), target, exe, seeds, args.rng_calls)
        except (EmuError, RuntimeError) as e:
            failed["cRZRandom"] = str(e)
        for op, s in (rng_stats or {}).items():
            print(f"{RNG_NAMES[op]:34} {s.exact:6}/{s.calls:<6} {s.rate() * 100:7.2f}%"
                  + (f"  worst {s.max_ulps} ulp" if s.max_ulps else ""))
            if s.first:
                print(f"    first difference: {s.first}")

    if "dirt" in selected:
        sizes = [int(s) for s in args.sizes.split(",")]
        cases = checks.dirt_matrix(seeds, sizes)

        def progress(i, n, c):
            eq = " ".join(f"{m} {c.equal[m] * 100:6.2f}%" for m in checks.MAPS)
            mark = "ok  " if c.ok() else "DIFF"
            print(f"[{i + 1:3}/{n}] {mark} {c.label()}  sea {c.sea[0]}/{c.sea[1]}  {eq}")
            if not c.ok():
                if c.first_vertex:
                    print(f"           first difference: {c.first_vertex}")
                if c.first_call:
                    print(f"           first RNG difference: {c.first_call}")

        try:
            dirt_cases = checks.check_dirt(make_emu, target, exe, cases, progress)
        except (EmuError, RuntimeError) as e:
            failed["Terrain generator"] = str(e)

    if "qfs" in selected:
        data = os.environ.get("SC3K_DATA")
        root = data if data and Path(data).is_dir() else None
        try:
            qfs_stats = qfs.check_qfs(make_emu, target, exe, root, args.qfs_streams)
        except (EmuError, RuntimeError) as e:
            failed[QFS_SECTION] = str(e)
        for source, s in (qfs_stats or {}).items():
            print(f"{QFS_NAMES[source]:34} {s.exact:6}/{s.streams:<6} {s.rate() * 100:7.2f}%"
                  f"  bytes {s.byte_rate() * 100:.2f}%")
            if s.first:
                print(f"    first difference: {s.first}")
    if "ground" in selected:
        data = os.environ.get("SC3K_DATA")
        root = data if data and Path(data).is_dir() else None

        def progress(i, n, c):
            eq = " ".join(f"{m} {c.equal[m] * 100:6.2f}%" for m in ground.MAPS)
            if any(c.equal[m] != 1.0 for m in ground.QUERIES):
                eq += "  " + " ".join(f"{m} {c.equal[m] * 100:6.2f}%" for m in ground.QUERIES)
            mark = "ok  " if c.ok() else "DIFF"
            print(f"[{i + 1:3}/{n}] {mark} {c.file:32} sea {c.sea[0]}/{c.sea[1]}  {eq}"
                  + (f"  {c.unread} bytes unread" if c.unread else ""))
            if c.first:
                print(f"           first difference: {c.first}")

        try:
            ground_cases = ground.check_ground(make_emu, target, exe, root, progress)
            bump = ground.check_bump(make_emu(), target, exe)
        except (EmuError, RuntimeError) as e:
            failed[GROUND_SECTION] = str(e)
        if bump:
            print(f"bump maps (GenerateBumpMaps): land {bump.equal['land'] * 100:.2f}%, "
                  f"water {bump.equal['water'] * 100:.2f}%")
            if bump.first:
                print(f"    first difference: {bump.first}")
    if "ui" in selected:
        try:
            ui_stats = ui.check_ui(make_emu, target, exe)
        except (EmuError, RuntimeError) as e:
            failed[UI_SECTION] = str(e)
        for f, s in (ui_stats or {}).items():
            print(f"{UI_NAMES[f]:42} {s.exact:5}/{s.queries:<5} {s.rate() * 100:7.2f}%")
            if s.first:
                print(f"    first difference: {s.first}")
    if "tiling" in selected:
        data = os.environ.get("SC3K_DATA")
        root = data if data and Path(data).is_dir() else None
        try:
            tiling_stats = tiling.check_tiling(make_emu, target, exe, root)
        except (EmuError, RuntimeError) as e:
            failed[TILING_SECTION] = str(e)
        for src, s in (tiling_stats or {}).items():
            print(f"{TILING_NAMES[src]:42} {s.exact:5}/{s.inputs:<5} {s.rate() * 100:7.2f}%")
            if s.first:
                print(f"    first difference: {s.first}")
    for section, error in failed.items():
        print(f"{section}: check stopped: {error}")
    return rng_stats, dirt_cases, qfs_stats, failed, ground_cases, ui_stats, tiling_stats, bump


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("what", nargs="?", default="all",
                    choices=["rng", "dirt", "qfs", "ground", "ui", "tiling", "coverage", "all"])
    ap.add_argument("--seeds", type=int, default=5, help="seeds per check (default 5)")
    ap.add_argument("--sizes", default="64,128,256", help="map sizes for the terrain check")
    ap.add_argument("--rng-calls", type=int, default=400, help="cRZRandom calls per seed")
    ap.add_argument("--qfs-streams", type=int, default=500,
                    help="QFS streams sampled from the install (default 500)")
    ap.add_argument("--apps", help="folder with the game's DLLs (default $SC3K_DATA/Apps)")
    ap.add_argument("--dll", help="SIMDIRT.DLL to use instead of the one in --apps")
    ap.add_argument("--fpcw", type=lambda s: int(s, 0), default=WINDOWS_FPCW,
                    help="x87 control word (default 0x027F, 53-bit precision as on Windows)")
    ap.add_argument("--jobs", type=int, default=pool.JOBS,
                    help=f"worker processes for the terrain and saved-terrain cases "
                         f"(default {pool.JOBS}, all cores)")
    ap.add_argument("--report", help="write a markdown report here")
    ap.add_argument("--image", help="write the README accuracy image here (needs dirt)")
    args = ap.parse_args()
    pool.JOBS = args.jobs

    apps = Path(args.apps) if args.apps else None
    if not apps and os.environ.get("SC3K_DATA"):
        apps = Path(os.environ["SC3K_DATA"]) / "Apps"
    selected = {"rng", "dirt", "qfs", "ground", "ui", "tiling"} if args.what == "all" \
        else {args.what} - {"coverage"}
    if selected and not apps and not args.dll:
        print("SC3K_DATA not set and no --apps: the checks are skipped", file=sys.stderr)

    matches, notes = {}, {}
    bins, ported = coverage.ported_functions()
    if args.what == "coverage":
        notes.update({t.dll: "not run: coverage only" for t in TARGETS})
    executed = {name: set() for name in bins}
    rng_stats = dirt_cases = qfs_stats = ground_cases = ui_stats = tiling_stats = bump = None
    failed = {}
    exe = None
    seeds = [1 + 0x9E3779B9 * i & 0x7FFFFFFF for i in range(args.seeds)]

    for target in TARGETS:
        mine = selected & set(target.checks)
        if not mine:
            continue
        path = Path(args.dll) if args.dll and target.dll == "SIMDIRT.DLL" else \
            (apps / target.dll if apps else None)
        if not path or not path.exists():
            notes[target.dll] = "not run: DLL not found"
            print(f"{target.dll}: not found, skipping {', '.join(sorted(mine))}")
            continue
        if exe is None:
            print("building sc3k-dump ...", file=sys.stderr)
            exe = rust.build()
        print(f"== {target.dll}")
        r, d, q, f, g, u, t, b = run_target(target, mine, path, args, exe, seeds,
                                         executed[target.dll], ported[target.dll].located)
        rng_stats, dirt_cases, qfs_stats = r or rng_stats, d or dirt_cases, q or qfs_stats
        ground_cases, ui_stats = g or ground_cases, u or ui_stats
        tiling_stats, bump = t or tiling_stats, b or bump
        failed.update(f)
        own = rows_for(r, d, q, f, g, u, t, b)
        if own:
            matches[target.dll] = sum(row[2] for row in own) / len(own)

    rows = rows_for(rng_stats, dirt_cases, qfs_stats, failed, ground_cases, ui_stats,
                    tiling_stats, bump)
    dll_rows = coverage.table(bins, ported, executed, matches, notes)
    if args.what in ("coverage", "all"):
        print(f"\n{'binary':18} {'functions':>9} {'ported':>6} {'checked':>7} {'exempt':>6} "
              f"{'accuracy':>9}  status")
        for r in dll_rows:
            print(f"{r.binary:18} {r.functions:9} {r.ported:6} {r.checked:7} {r.exempt:6} "
                  f"{r.accuracy() * 100:8.2f}%  {r.status()}")

    parts = [f"{args.seeds} seeds"] if rng_stats or dirt_cases else []
    if rng_stats:
        parts.append(f"{sum(s.calls for s in rng_stats.values())} cRZRandom calls")
    if dirt_cases:
        parts.append(f"{len(dirt_cases)} terrains")
    if qfs_stats:
        parts.append(f"{sum(s.streams for s in qfs_stats.values())} QFS streams")
    if ground_cases:
        parts.append(f"{len(ground_cases)} saved terrains")
    if ui_stats:
        parts.append(f"{sum(s.queries for s in ui_stats.values())} UI layout queries")
    if tiling_stats:
        parts.append(f"{sum(s.inputs for s in tiling_stats.values())} tiling rule files")
    footer = ", ".join(parts + [f"x87 control word {args.fpcw:#06x}"])
    show_dlls = dll_rows if args.what in ("coverage", "all") else None
    if args.report:
        report(args.report, rows, rng_stats, dirt_cases, footer, show_dlls, qfs_stats,
               ground_cases, ui_stats, tiling_stats)
        print(f"wrote {args.report}")
    if args.image:
        if not dirt_cases:
            sys.exit("--image needs the dirt check")
        import picture
        picture.render(args.image, dirt_cases[0], rows, footer, show_dlls)
        print(f"wrote {args.image}")

    bad = [r for r in rows if r[2] != 1.0]
    print("no checks ran" if not rows else "all checks match" if not bad
          else f"{len(bad)} checks differ")
    unchecked = ratchet(ported, executed, notes) if args.what == "all" else []
    return 1 if bad or unchecked else 0


def ratchet(ported, executed, notes):
    """After `all`: every ported function must be checked or say why not. Binaries whose
    checks did not run are skipped, since nothing of theirs could have been executed."""
    unchecked, stale = coverage.unchecked(ported, executed, skip=notes)
    for where in stale:
        print(f"{where}: every function under this Unchecked: marker is checked now; it can go")
    if unchecked:
        print(f"\n{len(unchecked)} ported functions no check runs. Add a check, or say why not "
              f"with `Unchecked: <reason>` in the doc comment:")
        for binary, function, where in unchecked:
            print(f"  {where}: {binary} {function}")
    return unchecked


if __name__ == "__main__":
    sys.exit(main())
