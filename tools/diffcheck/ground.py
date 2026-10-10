"""The saved-terrain check: `cSC3DirtBag::Init(cISC3City*, cIGZDBSegment*)` in SIMDIRT.DLL
against `sc3k_sim::load::read_dirt_bag`, and the vertex light it computes at the end
(`calculateAndSetVertexLight`) against `sc3k_render::light::vertex_light`. On the loaded dirt
bag it then asks the queries `diffref ground` lists in `queries.txt` (`sc3k_sim::dirt_bag`)
and compares the answers. The port picks the arguments: the per-vertex and per-cell queries
cover some columns only, since all of them would triple the check's time.

Every `.sct` terrain and `.sc3` city under $SC3K_DATA/Cities. `sc3k-dump diffref ground`
copies the dirt bag's record out of the file and gives the port's reading of it. The original
reads the same bytes: the check builds a dirt bag the way `Init(cISC3City*)` leaves it and
hands `Init` a fake city, DB segment and serial record (`Emu.fake_object`). The serial record
serves the fields from the record's bytes in the order the original asks for them.

`GenerateBumpMaps` runs once, against `sc3k_render::terrain::bump_maps`. It seeds its
`cRZRandom` with the clock, and the emulator's `timeGetTime` returns 0.
"""

import struct
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

import pool
from checks import read_cellmap

SERIAL_IID = 0x00199627     # cIGZDBSerialRecord
MAPS = ["altitude", "water", "light"]              # what Init leaves, one byte per vertex


@dataclass
class Ground:
    vx: int
    vy: int
    sea: int
    altitude: bytes
    water: bytes
    light: bytes


@dataclass
class GroundCase:
    file: str
    sea: tuple = (0, 0)
    equal: dict = field(default_factory=dict)       # map -> fraction of equal vertices
    queries: dict = field(default_factory=dict)     # query -> [equal answers, answers]
    first: str = ""
    unread: int = 0                                 # record bytes the original left unread

    def ok(self):
        return self.sea[0] == self.sea[1] and all(v == 1.0 for v in self.equal.values()) \
            and all(e == n for e, n in self.queries.values()) and not self.unread


def port(exe, path, outdir):
    """(record bytes, port's Ground, port's queries as [(name, args, answer)])."""
    subprocess.run([str(exe), "diffref", "ground", str(path), str(outdir)], check=True)
    data = (outdir / "port.bin").read_bytes()
    if data[:8] != b"SC3KGREF":
        raise ValueError("not a diffref ground file")
    version, vx, vy, sea = struct.unpack_from("<4I", data, 8)
    if version != 3:
        raise ValueError(f"diffref ground version {version}, expected 3")
    n, at = vx * vy, 24
    maps = [data[at + i * n:at + (i + 1) * n] for i in range(3)]
    queries = []
    for line in (outdir / "queries.txt").read_text().splitlines():
        call, answer = line.split(" = ")
        name, *args = call.split()
        queries.append((name, [int(a) for a in args], int(answer)))
    return (outdir / "record.bin").read_bytes(), Ground(vx, vy, sea, *maps), queries


class Record:
    """The serial record's read position over the record bytes."""

    def __init__(self, data):
        self.data, self.pos = data, 0

    def take(self, n):
        if self.pos + n > len(self.data):
            raise RuntimeError(f"read of {n} bytes at {self.pos} runs past the record "
                               f"({len(self.data)} bytes)")
        out = self.data[self.pos:self.pos + n]
        self.pos += n
        return out


def cellmap(emu, width, height, fill):
    """A `cRZCellMap<u8>` as SIMDIRT lays it out: vtable, X, Y, X column pointers."""
    cols = emu.alloc(4 * width)
    for x in range(width):
        col = emu.alloc(height)
        emu.write(col, bytes([fill]) * height)
        emu.w32(cols + 4 * x, col)
    m = emu.fake_object("cRZCellMap<u8>", {})
    for i, v in enumerate((width, height, cols)):
        emu.w32(m + 4 + 4 * i, v)
    return m


def ask(emu, target, obj, name, args):
    """One dirt bag query, its answer as the port writes it."""
    address, kind = target.dirt_bag["queries"][name]
    if kind == "out8":
        out = emu.alloc(4)
        emu.call(address, args + [out], this=obj)
        return emu.u8(out)
    if kind == "f32":
        (bits,) = struct.unpack("<I", struct.pack("<f", emu.call(address, args, this=obj,
                                                                 returns="double")))
        return bits
    r = emu.call(address, args, this=obj)
    return r if kind == "u32" else r & 0xFF if kind == "u8" else int(r & 0xFF != 0)


def original(emu, target, record, size, queries):
    """`Init(city, segment)` on a dirt bag of `size` cells per side, then `queries`."""
    g = target.dirt_bag
    rec = Record(record)
    ok = lambda e, this: 1  # noqa: E731

    def field_into(n):
        def read(e, this):
            e.write(e.arg(0), rec.take(n))
            return 1
        return read

    def get_void(e, this):
        e.write(e.arg(0), rec.take(e.arg(1)))
        return 1

    def get_string(e, this):
        # The dirt bag never looks at its marker strings; skip the text.
        (n,) = struct.unpack("<I", rec.take(4))
        rec.take(n)
        return 1

    def query(e, this):
        if e.arg(0) != SERIAL_IID:
            return 0
        e.w32(e.arg(1), this)
        return 1

    serial = emu.fake_object("cIGZDBSerialRecord", {
        0x00: (2, query), 0x04: (0, ok), 0x08: (0, ok),
        0x14: (2, get_void), 0x18: (1, field_into(1)), 0x28: (1, field_into(2)),
        0x38: (1, field_into(4)), 0x40: (1, field_into(4)), 0x54: (1, get_string)})

    def open_record(e, this):
        key = struct.unpack("<3I", e.read(e.arg(0), 12))
        if key != g["key"]:
            raise RuntimeError(f"opened record {key[0]:08X}-{key[1]:08X}-{key[2]:08X}")
        e.w32(e.arg(1), serial)
        return 1

    segment = emu.fake_object("cIGZDBSegment", {
        0x04: (0, ok), 0x08: (0, ok), 0x20: (2, open_record), 0x24: (1, ok)})
    city = emu.fake_object("cISC3City", {
        0x04: (0, ok), 0x08: (0, ok),
        g["city_cells_x"]: (0, lambda e, this: size), g["city_cells_z"]: (0, lambda e, this: size),
        g["city_version"]: (1, lambda e, this: 0)})

    # The global critical section (static construction never ran) and the change sender.
    lock = emu.fake_object("cRZCriticalSection", {0x04: (0, ok), 0x08: (0, ok)})
    emu.w32(g["lock"], emu.u32(lock))
    sender = emu.fake_object("cSC3CityChangeSender", {g["lock_updates"]: (1, ok)})

    # The dirt bag as Init(cISC3City*) leaves it (SIMDIRT.DLL 0x10003E50).
    v = size + 1
    obj = emu.alloc(g["size"])
    emu.w32(obj, g["vtable"])
    emu.w32(obj + 4, emu.u32(sender))
    emu.write(obj + g["ready"], b"\x01")
    emu.w32(obj + g["altitude"], cellmap(emu, v, v, 0x7F))
    emu.w32(obj + g["light"], cellmap(emu, v, v, 0))
    emu.w32(obj + g["water"], cellmap(emu, v, v, 0xF6))
    emu.write(obj + g["sea"], b"\xF6")
    for off in g["cell_bits"]:
        emu.w32(obj + off, emu.alloc(4 * (size * size >> 5)))
    emu.w32(obj + g["city"], city)

    if not emu.call(g["Init"], [city, segment], this=obj) & 0xFF:
        raise RuntimeError(f"Init refused the record after {rec.pos} of {len(record)} bytes")
    maps = {k: read_cellmap(emu, emu.u32(obj + g[k])) for k in MAPS}
    vx, vy, _ = maps["altitude"]
    answers = [ask(emu, target, obj, name, args) for name, args, _ in queries]
    return Ground(vx, vy, emu.u8(obj + g["sea"]), *(maps[k][2] for k in MAPS)), \
        len(record) - rec.pos, answers


def compare(case, want, got, queries, answers):
    case.sea = (want.sea, got.sea)
    if (want.vx, want.vy) != (got.vx, got.vy):
        raise RuntimeError(f"{case.file}: original is {want.vx}x{want.vy} vertices, port "
                           f"{got.vx}x{got.vy}")
    n = want.vx * want.vy
    for m in MAPS:
        a, b = getattr(want, m), getattr(got, m)
        case.equal[m] = sum(x == y for x, y in zip(a, b)) / n
        diff = next((i for i in range(n) if a[i] != b[i]), None)
        if diff is not None and not case.first:
            x, y = divmod(diff, want.vy)
            case.first = f"{m} at ({x}, {y}): original {a[diff]}, port {b[diff]}"
    for (name, args, mine), theirs in zip(queries, answers):
        tally = case.queries.setdefault(name, [0, 0])
        tally[0] += mine == theirs
        tally[1] += 1
        if mine != theirs and not case.first:
            case.first = f"{name}({', '.join(map(str, args))}): original {theirs}, port {mine}"
    return case


def files(root):
    cities = Path(root) / "Cities"
    return sorted(cities.glob("Terrains/*.sct")) + sorted(cities.glob("*.sc3"))


def check_ground(make_emu, target, exe, root, progress=None):
    paths = files(root) if root else []
    out = []
    with tempfile.TemporaryDirectory() as d:
        def one(i):
            record, got, queries = port(exe, paths[i], Path(d) / str(i))
            want, unread, answers = original(make_emu(), target, record, got.vx - 1, queries)
            case = compare(GroundCase(paths[i].name), want, got, queries, answers)
            case.unread = unread
            return case

        for i, case in enumerate(pool.ordered(one, len(paths), getattr(make_emu, "seen", None))):
            if progress:
                progress(i, len(paths), case)
            out.append(case)
    return out


@dataclass
class BumpCase:
    equal: dict = field(default_factory=dict)       # "land"/"water" -> fraction of equal bytes
    first: str = ""

    def ok(self):
        return all(v == 1.0 for v in self.equal.values())


def check_bump(emu, target, exe):
    """`GenerateBumpMaps` against the port's, both from a clock of 0."""
    g = target.dirt_bag
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "bump.bin"
        subprocess.run([str(exe), "diffref", "bump", "0", str(out)], check=True)
        data = out.read_bytes()
    if data[:8] != b"SC3KBUMP" or struct.unpack_from("<I", data, 8)[0] != 1:
        raise ValueError("not a diffref bump file, version 1")
    n = g["bump_len"]
    emu.call(g["GenerateBumpMaps"], [])
    case = BumpCase()
    for i, m in enumerate(["land", "water"]):
        a, b = emu.read(g[f"{m}_bump"], n), data[12 + i * n:12 + (i + 1) * n]
        case.equal[m] = sum(x == y for x, y in zip(a, b)) / n
        diff = next((k for k in range(n) if a[k] != b[k]), None)
        if diff is not None and not case.first:
            case.first = f"{m} byte {diff}: original {a[diff]:#04x}, port {b[diff]:#04x}"
    return case
