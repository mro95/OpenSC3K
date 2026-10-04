"""The port's side: `sc3k-dump diffref` (tools/sc3k-dump/src/main.rs) and its output format."""

import struct
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from targets import RNG_ARGS, RNG_OPS

REPO = Path(__file__).resolve().parents[2]


def build():
    """Builds sc3k-dump (release) once and returns the binary."""
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "sc3k-dump"], cwd=REPO, check=True)
    exe = REPO / "target" / "release" / "sc3k-dump"
    return exe if exe.exists() else exe.with_suffix(".exe")


def encode(op, args):
    """A script line: integers as u32 hex, doubles as their bits in hex."""
    words = [op]
    for kind, a in zip(RNG_ARGS[op], args):
        bits = struct.unpack("<Q", struct.pack("<d", a))[0] if kind == "d" else a & 0xFFFFFFFF
        words.append(f"{bits:x}")
    return " ".join(words)


def rng(exe, script):
    """Runs a list of (op, args) and returns each result: u32 for integer ops, the u64 bits
    of the double for floating-point ones, 0 for seed."""
    with tempfile.TemporaryDirectory() as d:
        src, out = Path(d) / "script.txt", Path(d) / "out.txt"
        src.write_text("".join(encode(op, args) + "\n" for op, args in script))
        subprocess.run([str(exe), "diffref", "rng", str(src), str(out)], check=True)
        return [int(line, 16) for line in out.read_text().split()]


@dataclass
class Call:
    op: str
    a: int
    b: int
    state: int
    double_state: int
    caller: int = 0   # only known on the original's side

    def key(self):
        return (self.op, self.a, self.b, self.state, self.double_state)


@dataclass
class Terrain:
    vx: int
    vy: int
    sea: int
    altitude: bytes   # x * vy + y, like cRZCellMap
    water: bytes
    flora: bytes
    salt: bytes       # 0 or 1
    trace: list

    def at(self, name, x, y):
        return getattr(self, name)[x * self.vy + y]


def dirt(exe, seed, size, difficulty, hills, water, trees, flags):
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "out.bin"
        args = [seed, size, difficulty, hills, water, trees, flags]
        subprocess.run([str(exe), "diffref", "dirt", *map(str, args), str(out)], check=True)
        data = out.read_bytes()
    if data[:8] != b"SC3KDREF":
        raise ValueError("not a diffref dirt file")
    version, vx, vy, sea = struct.unpack_from("<4I", data, 8)
    if version != 1:
        raise ValueError(f"diffref dirt version {version}, expected 1")
    at, n = 24, vx * vy
    maps = []
    for _ in range(4):
        maps.append(data[at:at + n])
        at += n
    (count,) = struct.unpack_from("<I", data, at)
    at += 4
    trace = []
    for op, a, b, state, dstate in struct.iter_unpack("<IQQII", data[at:at + 28 * count]):
        trace.append(Call(RNG_OPS[op], a, b, state, dstate))
    return Terrain(vx, vy, sea, *maps, trace)


def qfs(exe, streams):
    """`sc3k_formats::qfs::decompress` of each stream: (True, bytes) or (False, error)."""
    with tempfile.TemporaryDirectory() as d:
        for i, s in enumerate(streams):
            (Path(d) / f"{i:05}.qfs").write_bytes(s)
        subprocess.run([str(exe), "diffref", "qfs", d], check=True)
        out = []
        for i in range(len(streams)):
            ok = Path(d) / f"{i:05}.out"
            out.append((True, ok.read_bytes()) if ok.exists()
                       else (False, (Path(d) / f"{i:05}.err").read_text()))
        return out
