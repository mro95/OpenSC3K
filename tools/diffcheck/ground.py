"""The saved-terrain check: `cSC3DirtBag::Init(cISC3City*, cIGZDBSegment*)` in SIMDIRT.DLL
against `sc3k_sim::load::read_dirt_bag`, and the vertex light it computes at the end
(`calculateAndSetVertexLight`) against `sc3k_render::light::vertex_light`. On the loaded dirt
bag it then asks the queries `diffref ground` lists in `queries.txt` (`sc3k_sim::dirt_bag`)
and compares the answers. The port picks the arguments: the per-vertex and per-cell queries
cover some columns only, since all of them would triple the check's time. Then it runs the
terraforming operations `ops.txt` lists (`sc3k_sim::terraform`) and compares their answers
and the maps after them.

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
    ops: dict = field(default_factory=dict)         # terraforming op -> [equal answers, ops]
    after: dict = field(default_factory=dict)       # map -> fraction equal after the ops
    first: str = ""
    unread: int = 0                                 # record bytes the original left unread

    def ok(self):
        return self.sea[0] == self.sea[1] and not self.unread \
            and all(v == 1.0 for v in [*self.equal.values(), *self.after.values()]) \
            and all(e == n for e, n in [*self.queries.values(), *self.ops.values()])


def read_maps(path):
    data = path.read_bytes()
    if data[:8] != b"SC3KGREF":
        raise ValueError("not a diffref ground file")
    version, vx, vy, sea = struct.unpack_from("<4I", data, 8)
    if version != 3:
        raise ValueError(f"diffref ground version {version}, expected 3")
    n, at = vx * vy, 24
    return Ground(vx, vy, sea, *(data[at + i * n:at + (i + 1) * n] for i in range(3)))


def read_calls(lines):
    """`Name arg... = answer...` lines as [(name, [args], [answers])]."""
    out = []
    for line in lines:
        call, answer = line.split(" =")
        name, *args = call.split()
        out.append((name, [int(a) for a in args], [int(a) for a in answer.split()]))
    return out


def port(exe, path, outdir):
    """(record bytes, port's Ground, queries as [(name, args, answer)], the terraforming
    costs, ops as [(name, args, answers)], Ground after the ops)."""
    subprocess.run([str(exe), "diffref", "ground", str(path), str(outdir)], check=True)
    queries = [(n, a, r[0]) for n, a, r in
               read_calls((outdir / "queries.txt").read_text().splitlines())]
    costs, *ops = (outdir / "ops.txt").read_text().splitlines()
    costs = dict(zip((8, 9, 10), map(int, costs.split()[1:])))
    return (outdir / "record.bin").read_bytes(), read_maps(outdir / "port.bin"), queries, \
        costs, read_calls(ops), read_maps(outdir / "after.bin")


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


def fnv(data):
    h = 0x811C9DC5
    for b in data:
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return h


def terraform(emu, target, obj, size, name, args):
    """One terraforming op, its answers as the port writes them."""
    g = target.dirt_bag
    x1, z1, x2, z2, *level = args
    bounds = emu.alloc(24)
    for i, v in enumerate((x1 << 8, z1 << 8, 0, x2 << 8, z2 << 8, 0)):
        emu.w32(bounds + 4 * i, v)
    out, cost, at = emu.alloc(24), emu.alloc(4), emu.alloc(4)
    emu.write(out, b"\xFF" * 24)
    emu.w32(cost, 0)
    address = g["terraform"][name]
    if name == "GetOptimalLevel":
        call = [bounds, at, out, cost]
    else:
        call = [bounds, *level, out, cost] + ([] if name.startswith("Can") else [1])
    ok = emu.call(address, call, this=obj) & 0xFF != 0
    raw = [emu.u32(out + 4 * i) for i in (0, 1, 3, 4)]
    box = [0xFFFFFFFF] * 4 if raw == [0xFFFFFFFF] * 4 else [r >> 8 for r in raw]
    bits = emu.u32(obj + g["changes"])
    words, rows = emu.u32(bits + 0xC), emu.u32(bits + 0x10)
    changes = b"".join(emu.read(emu.u32(rows + 4 * x), 4 * words) for x in range(size))
    first = [emu.u8(at)] if name == "GetOptimalLevel" else []
    return first + [int(ok), emu.u32(cost), *box, fnv(changes)]


def native(emu, obj, slot, code):
    """Points a fake object's vtable slot at machine code: for methods the code under test
    calls so often that a Python callback each time would slow the check down."""
    at = emu.alloc(len(code))
    emu.write(at, code)
    emu.w32(emu.u32(obj) + slot, at)


def original(emu, target, record, size, queries, costs, ops):
    """`Init(city, segment)` on a dirt bag of `size` cells per side, then `queries`, then the
    terraforming `ops`."""
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
    sim = emu.fake_object("cISC3Simulator", {g["sim_value"]: (1, lambda e, this: costs[e.arg(0)])})
    city = emu.fake_object("cISC3City", {
        0x04: (0, ok), 0x08: (0, ok),
        g["city_version"]: (1, lambda e, this: 0), g["city_sim"]: (0, lambda e, this: sim)})
    for slot in (g["city_cells_x"], g["city_cells_z"]):
        native(emu, city, slot, b"\xB8" + struct.pack("<I", size) + b"\xC3")     # mov eax; ret

    # The statics (static construction never ran): the critical section and the app that
    # hands out the city. Then the change sender.
    lock = emu.fake_object("cRZCriticalSection", {0x04: (0, ok), 0x08: (0, ok)})
    emu.w32(g["lock"], emu.u32(lock))
    app = emu.fake_object("cISC3App", {g["app_city"]: (0, lambda e, this: city)})
    made, at = g["app"]
    emu.write(made, bytes([emu.u8(made) | 1]))
    emu.w32(at, app)
    emu.write(g["notifyCellUpdate"], b"\xC2\x14\x00")          # ret 0x14
    updates = emu.alloc(4)
    emu.w32(updates, 1)

    def set_updates(e, this):
        e.w32(updates, int(e.arg(0) & 0xFF != 0))
        return 1

    sender = emu.fake_object("cSC3CityChangeSender", {g["lock_updates"]: (1, set_updates)})
    native(emu, sender, g["updates_on"], b"\xA1" + struct.pack("<I", updates) + b"\xC3")

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
    head = emu.alloc(16)
    emu.w32(head, head)
    emu.w32(head + 4, head)
    emu.w32(obj + g["filters"], head)
    words, rows = (size + 31) // 32, emu.alloc(4 * size)
    for x in range(size):
        emu.w32(rows + 4 * x, emu.alloc(4 * words))
    bits = emu.alloc(20)
    for i, val in enumerate((0, size, size, words, rows)):
        emu.w32(bits + 4 * i, val)
    emu.w32(obj + g["changes"], bits)

    if not emu.call(g["Init"], [city, segment], this=obj) & 0xFF:
        raise RuntimeError(f"Init refused the record after {rec.pos} of {len(record)} bytes")
    maps = {k: read_cellmap(emu, emu.u32(obj + g[k])) for k in MAPS}
    vx, vy, _ = maps["altitude"]
    answers = [ask(emu, target, obj, name, args) for name, args, _ in queries]
    loaded = Ground(vx, vy, emu.u8(obj + g["sea"]), *(maps[k][2] for k in MAPS))
    results = [terraform(emu, target, obj, size, name, args) for name, args, _ in ops]
    maps = {k: read_cellmap(emu, emu.u32(obj + g[k])) for k in MAPS}
    after = Ground(vx, vy, loaded.sea, *(maps[k][2] for k in MAPS))
    return loaded, len(record) - rec.pos, answers, results, after


def compare_maps(case, equal, want, got, when=""):
    if (want.vx, want.vy) != (got.vx, got.vy):
        raise RuntimeError(f"{case.file}: original is {want.vx}x{want.vy} vertices, port "
                           f"{got.vx}x{got.vy}")
    n = want.vx * want.vy
    for m in MAPS:
        a, b = getattr(want, m), getattr(got, m)
        equal[m] = sum(x == y for x, y in zip(a, b)) / n
        diff = next((i for i in range(n) if a[i] != b[i]), None)
        if diff is not None and not case.first:
            x, y = divmod(diff, want.vy)
            case.first = f"{m}{when} at ({x}, {y}): original {a[diff]}, port {b[diff]}"


def compare_calls(case, tally, calls, answers):
    for (name, args, mine), theirs in zip(calls, answers):
        t = tally.setdefault(name, [0, 0])
        t[0] += mine == theirs
        t[1] += 1
        if mine != theirs and not case.first:
            case.first = f"{name}({', '.join(map(str, args))}): original {theirs}, port {mine}"


def compare(case, want, got, queries, answers, ops=(), results=(), want_after=None,
            got_after=None):
    case.sea = (want.sea, got.sea)
    compare_maps(case, case.equal, want, got)
    compare_calls(case, case.queries, queries, answers)
    compare_calls(case, case.ops, ops, results)
    if want_after:
        compare_maps(case, case.after, want_after, got_after, " after terraforming")
    return case


def files(root):
    cities = Path(root) / "Cities"
    return sorted(cities.glob("Terrains/*.sct")) + sorted(cities.glob("*.sc3"))


def check_ground(make_emu, target, exe, root, progress=None):
    paths = files(root) if root else []
    out = []
    with tempfile.TemporaryDirectory() as d:
        def one(i):
            record, got, queries, costs, ops, got_after = port(exe, paths[i], Path(d) / str(i))
            want, unread, answers, results, want_after = original(
                make_emu(), target, record, got.vx - 1, queries, costs, ops)
            case = compare(GroundCase(paths[i].name), want, got, queries, answers, ops,
                           results, want_after, got_after)
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
