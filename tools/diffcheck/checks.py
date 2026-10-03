"""The checks: run the same inputs through the original (emulated) and the port, compare."""

import bisect
import csv
import random
import struct
from dataclasses import dataclass, field
from pathlib import Path

import rust
from emu import UC_X86_REG_ECX, UC_X86_REG_ESP
from targets import RNG_ARGS, RNG_DOUBLE_RESULT, RNG_OPS

REPO = Path(__file__).resolve().parents[2]


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def ulps(a, b):
    """Distance in units of the last place between two doubles given as bits."""
    def ordered(u):
        return u if u < 1 << 63 else (1 << 63) - u
    return abs(ordered(a) - ordered(b))


# cRZRandom ------------------------------------------------------------------------------------

@dataclass
class OpStats:
    calls: int = 0
    exact: int = 0
    max_ulps: int = 0
    first: str = ""

    def rate(self):
        return self.exact / self.calls if self.calls else 1.0


INTERESTING_N = [0, 1, 2, 3, 7, 16, 100, 0x8000, 0xFFFFFE, 0xFFFFFF, 0x1000000, 0x7FFFFFFF,
                 0xFFFFFFFF]


def rng_script(seed, length, r):
    """A seed followed by `length` random calls with a mix of ordinary and edge arguments."""
    script = [("seed", [seed])]
    for _ in range(length):
        op = r.choice(RNG_OPS[1:])
        if op == "uniform":
            n = r.choice(INTERESTING_N) if r.random() < 0.3 else r.randrange(1, 1 << 16)
            args = [n]
        elif op in ("range", "gaussian_fast", "range_min_of_two"):
            lo = r.randrange(-300, 300)
            hi = lo + r.randrange(0, 600) if r.random() < 0.9 else r.randrange(-300, 300)
            args = [lo, hi]
        elif op == "double_range":
            lo = r.uniform(-1000, 1000)
            args = [lo, lo + r.uniform(0, 1000)]
        else:
            args = []
        script.append((op, args))
    return script


def rng_original(emu, target, script):
    this = emu.alloc(16)
    out = []
    for op, args in script:
        call_args = [float(a) if k == "d" else a for k, a in zip(RNG_ARGS[op], args)]
        returns = "double" if op in RNG_DOUBLE_RESULT else "int"
        v = emu.call(target.rng[op], call_args, this=this, returns=returns)
        out.append(0 if op == "seed" else bits(v) if returns == "double" else v)
    return out


def check_rng(emu, target, exe, seeds, length=400):
    """Per op: how many results match exactly, and the worst ULP distance for doubles."""
    stats = {op: OpStats() for op in RNG_OPS[1:]}
    r = random.Random(0x5C3)
    for seed in seeds:
        script = rng_script(seed, length, r)
        want = rng_original(emu, target, script)
        got = rust.rng(exe, script)
        for i, ((op, args), w, g) in enumerate(zip(script, want, got)):
            if op == "seed":
                continue
            s = stats[op]
            s.calls += 1
            if w == g:
                s.exact += 1
                continue
            if op in RNG_DOUBLE_RESULT:
                s.max_ulps = max(s.max_ulps, ulps(w, g))
            if not s.first:
                fmt = (lambda v: repr(struct.unpack("<d", struct.pack("<Q", v))[0])) \
                    if op in RNG_DOUBLE_RESULT else hex
                s.first = (f"seed {seed:#x}, call {i}: {op}({', '.join(map(str, args))}) "
                           f"original {fmt(w)}, port {fmt(g)}")
    return stats


# Function names for reports -------------------------------------------------------------------

class Names:
    """Names the function containing an address, from the Ghidra export and the ported Loki
    names (tools/ghidra/exports, tools/match/names)."""

    def __init__(self, target, base):
        self.starts, self.names = [], []
        found = {}
        export = REPO / "tools/ghidra/exports" / f"{target.dll}.tsv"
        ported = REPO / "tools/match/names" / f"{target.dll}.tsv"
        if export.exists():
            for row in csv.reader(open(export), delimiter="\t"):
                if row and row[0] == "func":
                    found[base + int(row[1], 16)] = row[2]
        if ported.exists():
            for row in csv.reader(open(ported), delimiter="\t"):
                if row and row[0] != "rva":
                    found[base + int(row[0], 16)] = row[1].split("(")[0]
        found.update(target.stages)
        for a in sorted(found):
            self.starts.append(a)
            self.names.append(found[a])

    def __call__(self, address):
        i = bisect.bisect_right(self.starts, address) - 1
        if i < 0:
            return f"{address:#010x}"
        return f"{self.names[i]}+{address - self.starts[i]:#x} ({address:#010x})"


# Terrain generator ----------------------------------------------------------------------------

def trace_rng(emu, target):
    """Hooks every cRZRandom entry point; returns the list that fills with outermost calls."""
    calls = []
    lo, hi = target.rng_code
    entries = {**{a: op for op, a in target.rng.items()}, **target.rng_thunks}
    last = [None]

    def make(op):
        def on_entry(e):
            esp = e.reg(UC_X86_REG_ESP)
            caller = e.u32(esp)
            if lo <= caller < hi:
                return                      # nested inside another RNG method
            if last[0] == (esp, caller):
                return                      # a thunk that jumped here, already logged
            last[0] = (esp, caller)
            kinds = RNG_ARGS[op]
            words = [e.arg(i) for i in range(2 * len(kinds))]
            if kinds == "dd":
                a, b = words[0] | words[1] << 32, words[2] | words[3] << 32
            else:
                a = words[0] if kinds else 0
                b = words[1] if len(kinds) > 1 else 0
            this = e.reg(UC_X86_REG_ECX)
            calls.append(rust.Call(op, a, b, e.u32(this), e.u32(this + 4), caller))
        return on_entry

    for address, op in entries.items():
        emu.hook(address, make(op))
    return calls


def read_cellmap(emu, at):
    """cRZCellMap<u8>: vtable, X, Y, then a pointer to X column pointers of Y bytes each."""
    vx, vy, cols = emu.u32(at + 4), emu.u32(at + 8), emu.u32(at + 12)
    return vx, vy, b"".join(emu.read(emu.u32(cols + 4 * x), vy) for x in range(vx))


def read_bitmap(emu, at):
    """The salt-water bit map: vtable, X, Y, words per column, then a pointer to X column
    pointers. Bit y % 32 of word y / 32 (assumed least significant bit first)."""
    vx, vy, words, cols = (emu.u32(at + 4 * i) for i in range(1, 5))
    out = bytearray()
    for x in range(vx):
        col = emu.read(emu.u32(cols + 4 * x), 4 * words)
        for y in range(vy):
            out.append(col[y >> 3] >> (y & 7) & 1)
    return bytes(out)


def find_vtable(emu, target):
    g = target.generator
    slots = [g["Init"], g["Shutdown"], g["SetDifficulty"], g["IsReady"], g["GenerateRandom"]]
    hits = emu.find_dwords(slots)
    if len(hits) != 1:
        raise RuntimeError(f"expected one cSC3DirtGenerator vtable, found {len(hits)}")
    return hits[0] - 0x0C


def dirt_original(emu, target, vtable, calls, seed, size, difficulty, hills, water, trees,
                  flags):
    """Init, SetDifficulty and GenerateRandom on a hand-built generator. `calls` is the list
    from `trace_rng` on this emulator."""
    g = target.generator
    obj = emu.alloc(g["size"])
    emu.w32(obj, vtable)
    emu.call(g["Init"], [size, size], this=obj)
    emu.call(g["SetDifficulty"], [difficulty], this=obj)
    calls.clear()
    emu.call(g["GenerateRandom"], [seed, hills, water, trees, flags], this=obj)
    vx, vy, alt = read_cellmap(emu, emu.u32(obj + g["altitude"]))
    maps = [alt] + [read_cellmap(emu, emu.u32(obj + g[k]))[2] for k in ("water", "flora")]
    salt = read_bitmap(emu, emu.u32(obj + g["salt"]))
    return rust.Terrain(vx, vy, emu.u8(obj + g["sea"]), *maps, salt, list(calls))


@dataclass
class DirtCase:
    seed: int
    size: int
    difficulty: int
    hills: int
    water: int
    trees: int
    flags: int
    sea: tuple = (0, 0)
    equal: dict = field(default_factory=dict)       # map -> fraction of equal vertices
    max_delta: int = 0                              # altitude
    first_vertex: str = ""
    calls: tuple = (0, 0)
    first_call: str = ""
    original: object = None
    port: object = None

    def label(self):
        return (f"seed {self.seed:#x} size {self.size} difficulty {self.difficulty} "
                f"hills {self.hills:#04x} water {self.water:#04x} trees {self.trees:#04x} "
                f"flags {self.flags:#04x}")

    def ok(self):
        return (self.sea[0] == self.sea[1] and all(v == 1.0 for v in self.equal.values())
                and not self.first_call)


MAPS = ["altitude", "water", "flora", "salt"]


def compare_dirt(case, want, got, names):
    case.original, case.port = want, got
    case.sea = (want.sea, got.sea)
    if (want.vx, want.vy) != (got.vx, got.vy):
        raise RuntimeError(f"{case.label()}: original is {want.vx}x{want.vy} vertices, "
                           f"port {got.vx}x{got.vy}")
    n = want.vx * want.vy
    for m in MAPS:
        a, b = getattr(want, m), getattr(got, m)
        case.equal[m] = sum(x == y for x, y in zip(a, b)) / n
    alt_w, alt_g = want.altitude, got.altitude
    deltas = [abs(x - y) for x, y in zip(alt_w, alt_g)]
    case.max_delta = max(deltas)
    for m in MAPS:
        a, b = getattr(want, m), getattr(got, m)
        diff = next((i for i in range(n) if a[i] != b[i]), None)
        if diff is not None:
            x, y = divmod(diff, want.vy)
            case.first_vertex = f"{m} at ({x}, {y}): original {a[diff]}, port {b[diff]}"
            break
    case.calls = (len(want.trace), len(got.trace))
    for i in range(max(case.calls)):
        w = want.trace[i] if i < len(want.trace) else None
        g = got.trace[i] if i < len(got.trace) else None
        if w and g and w.key() == g.key():
            continue
        show = lambda c: (f"{c.op}({c.a:#x}, {c.b:#x}) state {c.state:#010x}"  # noqa: E731
                          if c else "nothing (trace ended)")
        where = f" from {names(w.caller)}" if w else ""
        case.first_call = f"call {i}: original {show(w)}{where}; port {show(g)}"
        break
    return case


DEFAULT_PARAMS = (0x40, 0x40, 0x40, 0x24)
PARAM_SETS = [DEFAULT_PARAMS] + [(0x40, 0x40, 0x40, f) for f in
                                 (0x00, 0x01, 0x02, 0x08, 0x10, 0x40, 0x80, 0x0F, 0x5A, 0xA5)] + [
    (0x00, 0x00, 0x00, 0x24), (0xFF, 0xFF, 0xFF, 0x24), (0x80, 0x90, 0x20, 0x31)]


def dirt_matrix(seeds, sizes):
    """Every parameter set at 128 × 128, easy; and the New City defaults at every size and
    difficulty. Per seed."""
    cases = []
    for seed in seeds:
        for p in PARAM_SETS:
            cases.append((seed, 128 if 128 in sizes else sizes[0], 1, *p))
        for size in sizes:
            for d in (1, 2, 3):
                if (size, d) != (128, 1):
                    cases.append((seed, size, d, *DEFAULT_PARAMS))
    return cases


def check_dirt(make_emu, target, exe, cases, progress=None):
    emu = make_emu()
    vtable = find_vtable(emu, target)
    names = Names(target, emu.base)
    out = []
    for i, args in enumerate(cases):
        # A fresh emulator per case keeps the bump heap small and the globals clean.
        emu = emu if i == 0 else make_emu()
        calls = trace_rng(emu, target)
        want = dirt_original(emu, target, vtable, calls, *args)
        got = rust.dirt(exe, *args)
        case = compare_dirt(DirtCase(*args), want, got, names)
        if progress:
            progress(i, len(cases), case)
        out.append(case)
    return out
