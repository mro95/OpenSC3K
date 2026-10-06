"""The tiling rule check: SIMNTWRK.DLL's tile set reader against `sc3k_formats::tiling`.

- `cSTTransitLayer::GrokFileTileSet` reads a file through a fake `cRZFile` that serves the
  input bytes, into a fresh `vector<cGZResourceKey>`. Each key's instance is the tile id; its
  type and group must be the occupant key's.
- Inputs: every file under `$SC3K_DATA/Apps/Res/TilingRules` (the rule, convert and protected
  files too, which exercise the digit skipping), and hand-made edge cases.

`sc3k-dump diffref tiling` reads the same inputs with `parse_tile_set`. An input matches when
both give the same ids in the same order.
"""

import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

import pool

SOURCES = ["install", "edge cases"]

EDGE_CASES = [
    b"",
    b"29",
    b"{29, 43,\r\n18070}\r\n",
    b"} \r -5 a1b2 {}\r\n",
    b"+7 -0 0 00012 7a 7-3",
    b"1\x002 3",                                # a NUL ends the text
    b"12\x0b34 \x0c5 \r6",                      # \v, \f and \r are not delimiters
    b",,,  \t\n\n 8 ,\t, 9",
    b"\x80" b"5 \xff12 \xb39",                  # bytes from 0x80 are not digits
    b"2147483647 2147483648 4294967295 4294967296 99999999999",
    b" 7" * 0x8000,                             # longer than the 0x1FFFF-byte buffer
    b"1" * 0x1FFFE + b" 2",
]


@dataclass
class TilingStats:
    inputs: int = 0
    exact: int = 0
    first: str = ""

    def rate(self):
        return self.exact / self.inputs if self.inputs else 1.0


def inputs(root):
    """[(source, label, bytes)]."""
    out = []
    rules = Path(root) / "Apps" / "Res" / "TilingRules" if root else None
    if rules and rules.is_dir():
        for f in sorted(rules.iterdir(), key=lambda p: p.name.upper()):
            if f.is_file():
                out.append(("install", f.name, f.read_bytes()))
    for i, data in enumerate(EDGE_CASES):
        out.append(("edge cases", f"edge case {i}", data))
    return out


def port(exe, cases):
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        names = []
        for i, (_, _, data) in enumerate(cases):
            f = d / f"{i}.txt"
            f.write_bytes(data)
            names.append(str(f))
        src, out = d / "list.txt", d / "out.txt"
        src.write_text("".join(n + "\n" for n in names))
        subprocess.run([str(exe), "diffref", "tiling", str(src), str(out)], check=True)
        lines = out.read_text().splitlines()
    if len(lines) != len(cases):
        raise RuntimeError(f"diffref tiling answered {len(lines)} of {len(cases)} inputs")
    return [line.strip() for line in lines]


class Original:
    """`GrokFileTileSet` on one emulator, with one fake file whose bytes are set per input."""

    def __init__(self, emu, target):
        self.emu, self.t = emu, target.tiling
        t = self.t
        self.data = b""

        def read(e, this):
            buf, length = e.arg(0), e.arg(1)
            n = min(len(self.data), e.u32(length))
            e.write(buf, self.data[:n])
            e.w32(length, n)
            return 1

        def open_(e, this):
            got = (e.arg(0), e.arg(1), e.arg(2))
            if got != (1, 2, 1):
                raise RuntimeError(f"cRZFile opened with {got}, expected (1, 2, 1)")
            return 1

        ok = lambda e, this: 1  # noqa: E731
        self.file = emu.fake_object("cRZFile", {
            t["file_check"]: (0, ok), t["file_open"]: (3, open_),
            t["file_read"]: (2, read), t["file_close"]: (0, ok)})
        self.buf = emu.alloc(t["buffer_size"])

    def tile_set(self, data):
        emu, t = self.emu, self.t
        self.data = data
        emu.write(self.buf, bytes(t["buffer_size"]))
        vec = emu.alloc(12)
        if not emu.call(t["GrokFileTileSet"], [self.file, self.buf, vec]) & 0xFF:
            return "failed"
        begin, end = emu.u32(vec + t["vec_begin"]), emu.u32(vec + t["vec_end"])
        ids = []
        for at in range(begin, end, t["key_size"]):
            kind, group, inst = (emu.u32(at + 4 * i) for i in range(3))
            if (kind, group) != (t["key_type"], t["key_group"]):
                return f"key {kind:08X}-{group:08X}-{inst:08X}"
            ids.append(str(inst))
        return " ".join(ids) or "-"


def check_tiling(make_emu, target, exe, root):
    """{source: TilingStats}."""
    cases = inputs(root)
    answers = port(exe, cases)
    stats = {s: TilingStats() for s in SOURCES if any(c[0] == s for c in cases)}
    originals = {}

    def one(i):
        # One emulator per worker process: the reader leaves no state behind but strtok's.
        if os.getpid() not in originals:
            originals[os.getpid()] = Original(make_emu(), target)
        return originals[os.getpid()].tile_set(cases[i][2])

    theirs_all = pool.ordered(one, len(cases), getattr(make_emu, "seen", None))
    for (source, label, data), mine, theirs in zip(cases, answers, theirs_all):
        s = stats[source]
        s.inputs += 1
        if theirs == mine:
            s.exact += 1
        elif not s.first:
            s.first = f"{label}: {first_difference(theirs, mine)}"
    return stats


def first_difference(theirs, mine):
    a, b = theirs.split(), mine.split()
    i = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
    if i == len(a) == len(b):
        return f"original `{theirs[:80]}`, port `{mine[:80]}`"
    at = lambda ids: ids[i] if i < len(ids) else "end"  # noqa: E731
    return (f"id {i} of {len(a)}/{len(b)}: original `{at(a)}`, port `{at(b)}` "
            f"(after `{' '.join(a[max(0, i - 3):i])}`)")
