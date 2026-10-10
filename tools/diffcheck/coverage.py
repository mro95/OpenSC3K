"""How much of each original binary the port covers, and how much of that a check runs.

- functions: the binary's functions in tools/ghidra/exports/<dll>.tsv (and the vtable-only
  ones tools/match/names adds), without the compiler's `Unwind@` / `Catch@` funclets;
- ported: functions the Rust code cites in its doc comments. A Windows address
  (`SIMDIRT.DLL 0x1001BB50`) names its function directly. A Loki address (`libSimDirt
  0x4142C`, or a bare one in a file that names its library) is looked up in
  tools/loki/symbols and carried over to Windows through tools/match/names by its C++ name;
  without a match it still counts as ported, at an unknown Windows address;
- checked: ported functions at a known address that a check executed in the emulator;
- exempt: ported functions whose doc comment says why no check runs them (`Unchecked:` and a
  reason, anywhere in the same doc comment block as the citation).

After a full run, a ported function that is neither checked nor exempt is an error
(`unchecked`), so new code gets a check or a stated reason.

`accuracy` = match rate of the binary's checks × checked / functions, so a binary nothing
has been ported from is at 0%.
"""

import bisect
import csv
import re
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXPORTS = REPO / "tools/ghidra/exports"
NAMES = REPO / "tools/match/names"
LOKI = REPO / "tools/loki/symbols"
PAIRS = REPO / "tools/match/pairs.txt"
LOKI_GHIDRA_BASE = 0x10000      # Ghidra loads the Loki libraries 0x10000 above their ELF address
NEAR = 0x4000                   # an address further than this past a function start is data


def binaries():
    return sorted((p.name[:-4] for p in EXPORTS.glob("*.tsv")), key=str.upper)


def pairs():
    """Windows binary -> Loki binaries, and back."""
    to_loki, to_win = {}, {}
    for line in PAIRS.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        win, *loki = line.split()
        to_loki[win] = loki
        for lib in loki:
            to_win.setdefault(lib, win)
    return to_loki, to_win


@dataclass
class Binary:
    name: str
    base: int = 0
    starts: list = field(default_factory=list)      # absolute function starts, sorted
    names: dict = field(default_factory=dict)       # C++ name -> absolute address

    def function_at(self, address):
        i = bisect.bisect_right(self.starts, address) - 1
        if i < 0 or address - self.starts[i] >= NEAR:
            return None
        return self.starts[i]


def load_binary(name):
    b = Binary(name)
    funcs = set()
    export = EXPORTS / f"{name}.tsv"
    with open(export) as f:
        first = f.readline()
        m = re.search(r"imagebase=0x([0-9a-fA-F]+)", first)
        b.base = int(m.group(1), 16) if m else 0x10000000
        for row in csv.reader(f, delimiter="\t"):
            if row and row[0] == "func" and not re.match(r"(Unwind|Catch)@", row[2]):
                funcs.add(b.base + int(row[1], 16))
    names = NAMES / f"{name}.tsv"
    if names.exists():
        for row in csv.reader(open(names), delimiter="\t"):
            if row and row[0] != "rva":
                a = b.base + int(row[0], 16)
                funcs.add(a)
                b.names.setdefault(row[1], a)
    b.starts = sorted(funcs)
    return b


def ghidra_offset(lib):
    """Ghidra address - ELF address: 0x10000 for a Loki library, 0 for the executable."""
    return 0 if lib.endswith(".x86") else LOKI_GHIDRA_BASE


def load_loki(lib):
    """ELF address -> demangled name, as sorted lists."""
    path = LOKI / f"{lib}.tsv"
    rows = []
    if path.exists():
        for row in csv.reader(open(path), delimiter="\t"):
            if row and row[0] != "addr":
                rows.append((int(row[0], 16), row[2]))
    rows.sort()
    return [a for a, _ in rows], [n for _, n in rows]


# Citations in the Rust code -----------------------------------------------------------------

GROUP = re.compile(r"(?<![\w`])\(([^()]*\b0x[0-9A-Fa-f]+[^()]*)\)")
RVA = re.compile(r"\b([A-Za-z][\w]*\.(?:DLL|dll|exe))\+0x([0-9A-Fa-f]+)")
HEX = re.compile(r"\b0x([0-9A-Fa-f_]+)\b")
WORD = re.compile(r"\b([A-Za-z][A-Za-z0-9_]*)(?:\.(?:DLL|dll|exe|so))?\b")
TICKED = re.compile(r"`([^`]+)`")
DATA = re.compile(r"\b(rodata|\.?data|table|tables|bytes|row|rows)\b")
MARKER = re.compile(r"\bUnchecked:\s*(\S.*)")


@dataclass
class Cite:
    file: str
    line: int
    address: int      # absolute Windows address, or Loki Ghidra address
    binary: str       # Windows name ("SIMDIRT.DLL") or Loki library ("libSimDirt.so")
    hint: str = ""    # the function name quoted just before, if any
    exempt: str = ""  # the reason after `Unchecked:` in the same doc comment block, if any
    marker: str = ""  # "file:line" of that marker


def _binary_name(word, windows, loki):
    """`SIMDIRT`, `SIMDIRT.DLL`, `SimUI.dll` or `libSimDirt` -> a known binary, or None."""
    up = word.upper()
    for w in windows:
        if up in (w.upper(), w.upper().rsplit(".", 1)[0]):
            return w
    for lib in loki:
        if word in (lib, lib.rsplit(".", 1)[0]):
            return lib
    return None


def _hint(before):
    """The method name of the last `quoted` identifier: `cSC3DirtBag::GetDirtClod` ->
    GetDirtClod; `Draw__C17cSC3DirtClodShore` -> Draw."""
    ticked = TICKED.findall(before)
    if not ticked:
        return ""
    name = ticked[-1].split("(")[0].split("::")[-1].split("__")[0]
    return name if re.fullmatch(r"~?\w+", name) and not name.startswith("0x") else ""


def citations(crates=REPO / "crates"):
    """Every function address in the doc comments of `crates`. The binary is the one named in
    the same parentheses, else the last one the file named. Parentheses may run over several
    doc comment lines. A citation is exempt when its doc comment block (consecutive `///` or
    `//!` lines) has an `Unchecked:` marker."""
    windows = binaries()
    _, to_win = pairs()
    loki = sorted(to_win)
    bases = {}
    out = []
    for path in sorted(crates.rglob("*.rs")):
        rel = str(path.relative_to(crates.parent))
        default = None
        block, reason, marker = len(out), "", ""
        lines = [line.strip() for line in path.read_text().splitlines()] + [""]
        is_doc = [line.startswith(("///", "//!")) for line in lines]
        joined = None       # (first line number, text) of a doc line whose "(" is still open
        for n, text in enumerate(lines, 1):
            if not is_doc[n - 1]:
                for c in out[block:]:
                    c.exempt, c.marker = reason, marker
                block, reason, marker = len(out), "", ""
                continue
            if m := MARKER.search(text):
                reason, marker = m.group(1).strip(), f"{rel}:{n}"
            more = is_doc[n]        # the next line continues this doc comment block
            if joined:
                n, text = joined[0], joined[1] + " " + text[3:].strip()
                joined = None
            if text.count("(") > text.count(")") and more:
                joined = n, text
                continue
            for m in RVA.finditer(text):
                b = _binary_name(m.group(1), windows, loki)
                if b in windows:
                    if b not in bases:
                        bases[b] = load_binary(b).base
                    out.append(Cite(rel, n, bases[b] + int(m.group(2), 16), b,
                                    _hint(text[:m.start()])))
                    default = b
            text_wo = RVA.sub("", text)
            for word in WORD.findall(text_wo):
                b = _binary_name(word, windows, loki)
                if b:
                    default = b
                    break
            for m in GROUP.finditer(text_wo):
                current, data = default, False
                hint = _hint(text_wo[:m.start()])
                for part in re.split(r",|\band\b", m.group(1)):
                    for word in WORD.findall(part):
                        b = _binary_name(word, windows, loki)
                        if b:
                            current = b
                    if DATA.search(part):
                        data = True
                    if data or not current:
                        continue
                    for h in HEX.findall(part):
                        out.append(Cite(rel, n, int(h.replace("_", ""), 16), current, hint))
    return out


# Ported functions ---------------------------------------------------------------------------

@dataclass
class Ported:
    located: set = field(default_factory=set)       # absolute Windows addresses
    unlocated: set = field(default_factory=set)     # C++ names with no Windows address
    sources: dict = field(default_factory=dict)     # address or name -> "file:line"
    exempt: dict = field(default_factory=dict)      # address or name -> reason it is unchecked
    markers: dict = field(default_factory=dict)     # marker "file:line" -> addresses and names

    def count(self):
        return len(self.located) + len(self.unlocated)


def ported_functions(cites=None):
    """(binaries, Windows binary -> Ported). Only addresses that are function starts count:
    a citation of a table, a constant or a call site is skipped."""
    to_loki, to_win = pairs()
    bins = {name: load_binary(name) for name in binaries()}
    lokis = {}
    out = {name: Ported() for name in bins}

    def loki_name(lib, address, hint):
        if lib not in lokis:
            lokis[lib] = load_loki(lib)
        addrs, names = lokis[lib]
        elf = address - ghidra_offset(lib)
        i = bisect.bisect_left(addrs, elf)
        if i == len(addrs) or addrs[i] != elf:
            return None
        name = names[i]
        method = name.split("(")[0].split("::")[-1]
        return name if not hint or method == hint else None

    starts = {name: set(b.starts) for name, b in bins.items()}
    cites = cites if cites is not None else citations()
    windows_lines = set()       # a Loki address next to a Windows one names the same function
    loki_cites = []
    for c in cites:
        win = c.binary if c.binary in bins else to_win.get(c.binary)
        if win and bins[win].base <= c.address < bins[win].base + 0x0100_0000:
            if c.address in starts[win]:
                out[win].located.add(c.address)
                out[win].sources.setdefault(c.address, f"{c.file}:{c.line}")
                if c.exempt:
                    out[win].exempt.setdefault(c.address, c.exempt)
                    out[win].markers.setdefault(c.marker, set()).add(c.address)
                windows_lines.add((c.file, c.line))
        elif c.address >= LOKI_GHIDRA_BASE:
            loki_cites.append(c)
    for c in loki_cites:
        if (c.file, c.line) in windows_lines:
            continue
        where = f"{c.file}:{c.line}"
        # A Loki address: the library named, then every other one (a file may cite more than
        # one library and name only the first).
        first = c.binary if c.binary in to_win else (to_loki.get(c.binary) or [None])[0]
        found = None
        for lib in [first] + [lib for lib in sorted(to_win) if lib != first]:
            if lib and (name := loki_name(lib, c.address, c.hint)):
                found = lib, name
                break
            if not c.hint:
                break       # without a name to confirm it, only the named library counts
        if not found:
            continue
        lib, name = found
        candidates = [w for w, libs in to_loki.items() if lib in libs]
        for w in candidates:
            if name in bins[w].names:
                key = bins[w].names[name]
                out[w].located.add(key)
                break
        else:
            w, key = candidates[0], name
            out[w].unlocated.add(name)
        out[w].sources.setdefault(key, where)
        if c.exempt:
            out[w].exempt.setdefault(key, c.exempt)
            out[w].markers.setdefault(c.marker, set()).add(key)
    return bins, out


def watch(emu, addresses, executed):
    """Adds each of `addresses` to the set `executed` the first time the emulator runs it."""
    for a in addresses:
        if emu.base <= a < emu.base + emu.size:
            emu.once(a, executed.add)
    return emu


# The ratchet --------------------------------------------------------------------------------

def unchecked(ported, executed, skip=()):
    """(unchecked, stale) after a full run. unchecked: (binary, function, "file:line") of the
    ported functions no check ran and with no `Unchecked:` reason. stale: "file:line" of the
    markers all of whose functions a check ran, so the marker can go. Binaries in `skip` (their
    checks did not run) are left out."""
    bad, covers = [], {}
    for name, p in ported.items():
        ran = executed.get(name, set())
        for marker, keys in p.markers.items():
            covers.setdefault(marker, []).append(name not in skip and keys <= ran)
        if name in skip:
            continue
        for key in sorted(p.located) + sorted(p.unlocated):
            if key not in ran and key not in p.exempt:
                what = f"{key:#010x}" if isinstance(key, int) else key
                bad.append((name, what, p.sources[key]))
    return bad, sorted(m for m, done in covers.items() if all(done))


# The per-binary table -----------------------------------------------------------------------

@dataclass
class Row:
    binary: str
    functions: int
    ported: int
    unlocated: int
    checked: int
    exempt: int = 0
    match: float = None     # mean match rate of the binary's checks, None when none ran
    note: str = ""

    def accuracy(self):
        if not self.functions or self.match is None:
            return 0.0
        return self.match * self.checked / self.functions

    def status(self):
        if self.note:
            return self.note
        if not self.ported:
            return "not implemented yet"
        if self.match is None or not self.checked:
            return "ported, not checked"
        return "checked"


def table(bins, ported, executed, matches, notes):
    """`executed`: binary -> set of addresses; `matches`: binary -> mean match rate of its
    checks; `notes`: binary -> why its checks did not run."""
    rows = []
    for name, b in bins.items():
        p = ported[name]
        ran = p.located & executed.get(name, set())
        rows.append(Row(name, len(b.starts), p.count(), len(p.unlocated), len(ran),
                        len(p.exempt.keys() - ran), matches.get(name), notes.get(name, "")))
    rows.sort(key=lambda r: (-r.accuracy(), -r.ported, r.binary.upper()))
    return rows
