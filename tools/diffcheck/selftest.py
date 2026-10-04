#!/usr/bin/env python3
"""Tests the checker itself, without the game: builds tools/diffcheck/fixture/fixture.c (a
cRZRandom written from docs/sim/random.md and a QFS decoder written from docs/formats/qfs.md)
into a 32-bit Windows DLL with clang and lld-link, then runs the cRZRandom and QFS checks
against it. The correct build must match the port exactly; a build with deliberate bugs must
be caught. Also exercises the import stubs, x87 results, the fs segment, the call-trace hooks,
the per-binary coverage table, the report and the image.

Needs clang, lld-link and llvm-dlltool on PATH. Exits 0 on success.
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import checks  # noqa: E402
import coverage  # noqa: E402
import pefile  # noqa: E402
import qfs  # noqa: E402
import run  # noqa: E402
import rust  # noqa: E402
from emu import Emu, EmuError  # noqa: E402
from targets import Target  # noqa: E402

FIXTURE = HERE / "fixture"


def build(out_dir, broken=False):
    out_dir = Path(out_dir)
    lib = out_dir / "msvcrt.lib"
    obj = out_dir / ("broken.obj" if broken else "fixture.obj")
    dll = out_dir / ("broken.dll" if broken else "fixture.dll")
    subprocess.run(["llvm-dlltool", "-m", "i386", "-d", FIXTURE / "msvcrt.def", "-l", lib],
                   check=True)
    subprocess.run(["clang", "--target=i686-pc-windows-msvc", "-O2", "-mno-sse", "-fno-builtin",
                    *(["-DBROKEN"] if broken else []), "-c", FIXTURE / "fixture.c", "-o", obj],
                   check=True)
    subprocess.run(["lld-link", "/dll", "/noentry", "/nodefaultlib", "/base:0x10000000",
                    f"/out:{dll}", obj, lib], check=True, stdout=subprocess.DEVNULL)
    return dll


def exports(dll, base):
    pe = pefile.PE(str(dll))
    return {s.name.decode(): base + s.address for s in pe.DIRECTORY_ENTRY_EXPORT.symbols}


def target_for(dll):
    emu = Emu(dll)
    ex = exports(dll, emu.base)
    rng = {op: ex[f"rng_{op}"] for op in
           ["seed", "uniform", "range", "gaussian_fast", "double", "double_range"]}
    rng["next_u32"] = ex["rng_next"]
    rng["range_min_of_two"] = ex["rng_min_of_two"]
    starts = sorted(rng.values())
    end = min(a for a in ex.values() if a > starts[-1])
    q = {"QueryInterface": ex["qfs_query_interface"], "AddRef": ex["qfs_add_ref"],
         "Release": ex["qfs_release"], "CompressData": ex["qfs_compress"],
         "DecompressData": ex["qfs_decompress"],
         "GetMaxLengthRequiredForCompressedData": ex["qfs_max_length"],
         "GetLengthOfDecompressedData": ex["qfs_length"], "size": 16}
    return emu, ex, Target(dll=Path(dll).name, rng=rng, rng_code=(starts[0], end), qfs=q)


failures = []


def expect(cond, what):
    print(("ok    " if cond else "FAIL  ") + what)
    if not cond:
        failures.append(what)


def main():
    missing = [t for t in ("clang", "lld-link", "llvm-dlltool") if not shutil.which(t)]
    if missing:
        sys.exit(f"selftest needs {', '.join(missing)} on PATH")
    exe = rust.build()
    with tempfile.TemporaryDirectory() as tmp:
        good, bad = build(tmp), build(tmp, broken=True)
        emu, ex, target = target_for(good)

        # Harness mechanics.
        expect(emu.call(ex["probe_sqrt"], [2.0], returns="double") == 2.0 ** 0.5,
               "double argument, imported sqrt stub, result in ST0")
        p = emu.call(ex["probe_new"], [64])
        expect(emu.u32(p) == 0xC0FFEE, "imported operator new returns writable heap memory")
        expect(emu.call(ex["probe_fs"]) != 0, "fs:[0x18] reads the TEB")
        try:
            emu.call(ex["probe_missing"])
            expect(False, "a missing import stub stops the run")
        except EmuError as e:
            expect("not_stubbed" in str(e), "a missing import stub stops the run, by name")

        # Trace hooks: only outermost calls, with the state on entry.
        calls = checks.trace_rng(emu, target)
        this = emu.alloc(16)
        emu.call(ex["probe_trace"], [this])
        ops = [c.op for c in calls]
        expect(ops == ["seed", "gaussian_fast", "double_range"], f"trace is outermost only: {ops}")
        expect(calls[1].state == 7 and calls[1].a == 0xFFFFFFFD and calls[1].b == 9,
               "trace records arguments and state on entry")
        expect(calls[2].a == 0x3FF0000000000000 and calls[2].b == 0x4000000000000000,
               "trace records double arguments as bits")

        # The real check: the correct build matches the port exactly, the broken one doesn't.
        seeds = [1, 2, 0xDEADBEEF]
        stats = checks.check_rng(Emu(good), target, exe, seeds, 400)
        expect(all(s.rate() == 1.0 for s in stats.values()),
               "correct cRZRandom matches the port on every call: "
               + ", ".join(f"{op} {s.exact}/{s.calls}" for op, s in stats.items()))
        _, _, bad_target = target_for(bad)
        bad_stats = checks.check_rng(Emu(bad), bad_target, exe, seeds, 400)
        expect(bad_stats["gaussian_fast"].rate() < 1.0 and bad_stats["gaussian_fast"].first,
               f"broken GaussianFast is caught: {bad_stats['gaussian_fast'].first[:80]}...")

        # QFS: the correct decoder matches the port on every stream, the broken one doesn't.
        q_stats = qfs.check_qfs(lambda: Emu(good), target, exe)
        expect(set(q_stats) == {"round trip", "edge cases"}
               and all(s.rate() == 1.0 and s.byte_rate() == 1.0 for s in q_stats.values()),
               "correct QFS decoder matches the port on every stream: "
               + ", ".join(f"{k} {s.exact}/{s.streams}" for k, s in q_stats.items()))
        bad_q = qfs.check_qfs(lambda: Emu(bad), bad_target, exe)
        expect(bad_q["edge cases"].rate() < 1.0 and bad_q["edge cases"].first,
               f"broken QFS offset is caught: {bad_q['edge cases'].first[:80]}")

        # Coverage: one-shot hooks record what ran; every binary gets a row.
        executed = set()
        watched = coverage.watch(Emu(good), [ex["rng_seed"], ex["probe_sqrt"]], executed)
        watched.call(ex["probe_trace"], [watched.alloc(16)])
        watched.call(ex["probe_trace"], [watched.alloc(16)])
        expect(executed == {ex["rng_seed"]}, "coverage records functions that ran, once")
        bins, ported = coverage.ported_functions()
        dirt = ported["SIMDIRT.DLL"]
        expect(0x1001BB50 in dirt.located and 0x10017C2D in dirt.located
               and 0x12059EBE in ported["SIMBABLD.DLL"].located,
               "coverage finds the functions the Rust code cites")
        dll_rows = coverage.table(bins, ported, {"SIMDIRT.DLL": set(dirt.located)},
                                  {"SIMDIRT.DLL": 1.0}, {"SIMBABLD.DLL": "not run: DLL not found"})
        by = {r.binary: r for r in dll_rows}
        expect(len(dll_rows) == len(coverage.binaries()) >= 31, f"{len(dll_rows)} binaries in the table")
        expect(by["SIMECO.DLL"].accuracy() == 0.0 and by["SIMECO.DLL"].status() == "not implemented yet",
               "an unported binary is at 0%, not implemented yet")
        r = by["SIMDIRT.DLL"]
        expect(r.accuracy() == len(dirt.located) / r.functions and r.status() == "checked",
               f"SIMDIRT accuracy is checked functions / all: {r.accuracy() * 100:.2f}%")

        # Report and image, from two port terrains where one is altered.
        want = rust.dirt(exe, 1, 128, 1, 0x40, 0x40, 0x40, 0x24)
        got = rust.dirt(exe, 1, 128, 1, 0x40, 0x40, 0x40, 0x24)
        alt = bytearray(got.altitude)
        for i in range(2000, 2400):
            alt[i] = min(255, alt[i] + 3)
        got.altitude = bytes(alt)
        got.trace[100].state ^= 1
        case = checks.compare_dirt(checks.DirtCase(1, 128, 1, 0x40, 0x40, 0x40, 0x24), want, got,
                                   checks.Names(target, emu.base))
        expect(not case.ok() and case.max_delta == 3, f"terrain difference found: {case.first_vertex}")
        expect(case.first_call.startswith("call 100:"), f"trace difference found: {case.first_call[:60]}")
        rows = run.rows_for(stats, [case], q_stats)
        report, image = Path(tmp) / "accuracy.md", Path(tmp) / "accuracy.png"
        run.report(report, rows, stats, [case], "selftest", dll_rows, q_stats)
        import picture
        picture.render(image, case, rows, "selftest", dll_rows)
        text = report.read_text()
        expect("| Terrain generator | Altitude per vertex |" in text
               and "| QFS decompression | Hand-made edge cases | 100.00% |" in text, "report written")
        expect("| SIMECO.DLL | 1007 | 0 | 0 | – | 0.00% | not implemented yet |" in text,
               "report lists unported binaries at 0%")
        expect(image.stat().st_size > 10_000, "image written")
        if len(sys.argv) > 1:
            shutil.copy(image, sys.argv[1])
            print(f"copied the sample image to {sys.argv[1]}")

    print("selftest passed" if not failures else f"selftest: {len(failures)} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
