#!/usr/bin/env python3
"""Port Loki Linux (GCC 2.95) function names onto the Windows (MSVC 6) SC3U binaries.

Inputs (from tools/match/export.sh): features/{win,linux}/<binary>.tsv, plus the demangled
symbol tables in tools/loki/symbols/<binary>.tsv. Output: tools/match/names/<win binary>.tsv
with one row per matched Windows function.

Matching never looks at instruction bytes (the compilers differ). It uses:
  1. seeds: functions sharing rare strings / constants / FP literals / C library calls,
     accepted only when the pair is each other's clear best candidate;
  2. vtables: once a vtable pair is anchored by matched slots (or a matched function that
     references both), the remaining slots pair up by position;
  3. call graph: between matched anchors in a matched function's ordered call list, a gap
     holding exactly one unmatched callee on each side is a match (same for callers).
Steps 2-3 repeat until nothing new is found.
"""

import argparse
import collections
import difflib
import math
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# Calls whose names differ only by platform convention.
EXT_ALIASES = {
    "__builtin_new": "new", "??2@YAPAXI@Z": "new",
    "__builtin_vec_new": "new[]", "??_U@YAPAXI@Z": "new[]",
    "__builtin_delete": "delete", "??3@YAXPAX@Z": "delete",
    "__builtin_vec_delete": "delete[]", "??_V@YAXPAX@Z": "delete[]",
    "strcasecmp": "stricmp", "_stricmp": "stricmp",
    "strncasecmp": "strnicmp", "_strnicmp": "strnicmp",
    "snprintf": "snprintf", "_snprintf": "snprintf",
    "vsnprintf": "vsnprintf", "_vsnprintf": "vsnprintf",
}


PURE = "P"


def is_code(x):
    return isinstance(x, int)


def ranks(values):
    order = sorted(range(len(values)), key=values.__getitem__)
    r = [0.0] * len(values)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        for k in range(i, j + 1):
            r[order[k]] = (i + j) / 2
        i = j + 1
    return r


def spearman(xs, ys):
    """Rank correlation: GCC's PIC prologues inflate small functions, so sizes are only
    comparable by order, not by ratio."""
    if len(xs) < 3:
        return 0.0
    rx, ry = ranks(xs), ranks(ys)
    mx, my = sum(rx) / len(rx), sum(ry) / len(ry)
    cov = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    vx = sum((a - mx) ** 2 for a in rx) ** 0.5
    vy = sum((b - my) ** 2 for b in ry) ** 0.5
    return cov / (vx * vy) if vx and vy else 0.0


class Func:
    __slots__ = ("addr", "name", "size", "ninstr", "tokens", "calls", "callers")

    def __init__(self, addr, name, size, ninstr):
        self.addr, self.name, self.size, self.ninstr = addr, name, size, ninstr
        self.tokens = set()
        self.calls = []      # ordered, internal callee addresses
        self.callers = []


class Program:
    def __init__(self, path):
        self.funcs = {}
        self.vtables = {}    # addr -> (name, [slot addr or None])
        self.vt_users = collections.defaultdict(set)  # vtable addr -> function addrs
        self.imagebase = 0
        cur = None
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                p = line.rstrip("\n").split("\t")
                tag = p[0]
                if tag.startswith("# program"):
                    self.imagebase = int(line.split()[4], 16)
                elif tag == "F":
                    cur = Func(int(p[1], 16), p[2], int(p[3]), int(p[4]))
                    self.funcs[cur.addr] = cur
                elif tag == "C":
                    to = int(p[2], 16)
                    if to not in cur.calls:
                        cur.calls.append(to)
                elif tag == "X":
                    name = p[2]
                    # Ghidra qualifies externals with their library ("MSVCRT.DLL::strcpy",
                    # "<EXTERNAL>::strcpy"); only the symbol itself is comparable.
                    head, sep, tail = name.partition("::")
                    if sep and ("." in head or head == "<EXTERNAL>"):
                        name = tail
                    name = EXT_ALIASES.get(name, name)
                    # Only C-level names are comparable across compilers: drop MSVC (?, @)
                    # and GCC 2.95 (__7cClass, _._7cClass, Method__7cClass) mangled names.
                    if not any(c in name for c in "?@.") and "__" not in name.lstrip("_") \
                            and not name.startswith("__"):
                        cur.tokens.add("x:" + name.lstrip("_"))
                elif tag == "S":
                    # Paths differ by platform: "\\Sys\\SC3Tune.INI" vs "/sys/sc3tune.ini".
                    cur.tokens.add("s:" + p[2].replace("\\\\", "/").lower())
                elif tag == "K":
                    cur.tokens.add("k:" + p[2])
                elif tag == "R":
                    cur.tokens.add("r:" + p[2])
                elif tag == "V":
                    slots = []  # None = null, PURE = pure virtual stub, int = code address
                    for s in p[3].split(",") if len(p) > 3 and p[3] else []:
                        slots.append(None if s == "0" else PURE if s == "P" else int(s.lstrip("?"), 16))
                    self.vtables[int(p[1], 16)] = (p[2], slots)
                elif tag == "VR":
                    self.vt_users[int(p[1], 16)].add(int(p[2], 16))
        for fn in self.funcs.values():
            for c in fn.calls:
                if c in self.funcs:
                    self.funcs[c].callers.append(fn.addr)

    def resolve(self, x):
        """Follow a this-adjusting thunk (GCC __thunk_N_*, MSVC adjustor) to its target."""
        f = self.funcs.get(x)
        if f is not None and len(f.calls) == 1 and f.ninstr <= 10 and not f.tokens:
            return f.calls[0]
        return x


def elf_base(path):
    """Lowest PT_LOAD vaddr of a 32-bit ELF (nm addresses are relative to this, Ghidra's are not)."""
    data = path.read_bytes()
    phoff, = struct.unpack_from("<I", data, 28)
    phentsize, phnum = struct.unpack_from("<HH", data, 42)
    vaddrs = [struct.unpack_from("<II", data, phoff + i * phentsize + 4)[1]
              for i in range(phnum) if struct.unpack_from("<I", data, phoff + i * phentsize)[0] == 1]
    return min(vaddrs)


def load_symbols(path):
    """nm address -> (mangled, demangled) from tools/loki/symbols/<binary>.tsv."""
    out = {}
    with open(path, encoding="utf-8") as f:
        next(f)
        for line in f:
            addr, mangled, demangled = line.rstrip("\n").split("\t")
            out.setdefault(int(addr, 16), (mangled, demangled))
    return out


def method_name(demangled):
    """'cA::Init(cB *, int) const' -> 'Init' (None if unknown)."""
    if not demangled:
        return None
    depth = 0
    for i, ch in enumerate(demangled):
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif ch == "(" and depth == 0 and not demangled[:i].endswith("operator"):
            return demangled[:i].rsplit("::", 1)[-1]
    return demangled.rsplit("::", 1)[-1]


class Matcher:
    def __init__(self, lin, win, log, lname=lambda a: None):
        self.lin, self.win, self.log, self.lname = lin, win, log, lname
        self.l2w, self.w2l, self.how = {}, {}, {}
        self.vpairs, self.vpairs_w = {}, {}  # paired vtables, both directions

    def add(self, l, w, how):
        if l in self.l2w or w in self.w2l or l not in self.lin.funcs or w not in self.win.funcs:
            return False
        if not how.startswith("tokens") and not self._strings_compatible(l, w):
            return False
        self.l2w[l], self.w2l[w], self.how[l] = w, l, how
        return True

    def _strings_compatible(self, l, w):
        """Reject structural matches whose strings disagree. Platform variants of the same
        code ("www.lokigames.com" / "www.simcity.com", cGZFrameworkSDL / cGZFrameworkW95)
        still pass on similarity."""
        ls = sorted(t for t in self.lin.funcs[l].tokens if t.startswith("s:"))[:12]
        ws = sorted(t for t in self.win.funcs[w].tokens if t.startswith("s:"))[:12]
        if not ls or not ws or set(ls) & set(ws):
            return True
        return max(difflib.SequenceMatcher(None, a, b).ratio() for a in ls for b in ws) >= 0.6

    # 1. Seeds from rare shared tokens.
    def seed(self):
        dfl = collections.Counter(t for f in self.lin.funcs.values() for t in f.tokens)
        dfw = collections.Counter(t for f in self.win.funcs.values() for t in f.tokens)
        shared = {t for t in dfl if t in dfw}
        n = len(self.lin.funcs) + len(self.win.funcs)
        idf = {t: math.log(n / (dfl[t] + dfw[t])) for t in shared if dfl[t] <= 8 and dfw[t] <= 8}

        index = collections.defaultdict(list)
        for w in self.win.funcs.values():
            for t in w.tokens:
                if t in idf:
                    index[t].append(w.addr)

        def scores(f, idx, other_tokens):
            acc = collections.Counter()
            for t in f.tokens:
                if t in idf:
                    for o in idx[t]:
                        acc[o] += idf[t]
            return acc

        best_w = {}
        for l in self.lin.funcs.values():
            acc = scores(l, index, None)
            if acc:
                (w, s1), *rest = acc.most_common(2)
                s2 = rest[0][1] if rest else 0.0
                best_w[l.addr] = (w, s1, s2)

        rindex = collections.defaultdict(list)
        for l in self.lin.funcs.values():
            for t in l.tokens:
                if t in idf:
                    rindex[t].append(l.addr)
        n_new = 0
        for l, (w, s1, s2) in best_w.items():
            if s1 < 6.0 or s1 < 1.5 * s2:
                continue
            racc = scores(self.win.funcs[w], rindex, None)
            (l_back, r1), *rest = racc.most_common(2)
            r2 = rest[0][1] if rest else 0.0
            if l_back == l and r1 >= 1.5 * r2 and self.add(l, w, f"tokens:{s1:.1f}"):
                n_new += 1
        return n_new

    # 2. Vtables: pair Linux and Windows vtables, then pair their slots by position.
    #    A pair needs equal length and the same pure-virtual slots, and must be each other's
    #    clear best by score: 10 per slot agreeing with an existing match (-10 per contradiction;
    #    MSVC's identical-code folding makes a few unavoidable) plus the rank correlation of
    #    slot function sizes (thunks resolved) times the number of comparable slots.
    def vtables(self):
        # GCC 2.95 vtables (vtable thunks, no RTTI): [this offset, 0 (type info), slots..., 0].
        lslots = {a: s[2:-1] if s and s[-1] is None else s[2:] for a, (_, s) in self.lin.vtables.items()}
        wslots = {a: s for a, (_, s) in self.win.vtables.items()}
        lslots = {a: s for a, s in lslots.items() if any(map(is_code, s)) and a not in self.vpairs}
        wslots = {a: s for a, s in wslots.items() if any(map(is_code, s)) and a not in self.vpairs_w}

        def shape(slots):
            return tuple(x if not is_code(x) else 1 for x in slots)

        def score(ls, ws):
            agree = conflict = 0
            lsz, wsz = [], []
            for x, y in zip(ls, ws):
                if not is_code(x) or not is_code(y):
                    continue
                for a, b in ((x, y), (self.lin.resolve(x), self.win.resolve(y))):
                    if a in self.l2w:
                        if self.l2w[a] == b:
                            agree += 1
                        elif b in self.w2l:
                            conflict += 1
                lf = self.lin.funcs.get(self.lin.resolve(x))
                wf = self.win.funcs.get(self.win.resolve(y))
                if lf is not None and wf is not None:
                    lsz.append(lf.ninstr)
                    wsz.append(wf.ninstr)
            if conflict > agree:
                return None
            rho = spearman(lsz, wsz)
            return 10 * (agree - conflict) + max(rho, 0.0) * len(lsz), agree, rho, len(lsz)

        w_by_shape = collections.defaultdict(list)
        for a, ws in wslots.items():
            w_by_shape[shape(ws)].append(a)

        scored = collections.defaultdict(dict)  # la -> {wa: (score, agree, rho, n)}
        for la, ls in lslots.items():
            for wa in w_by_shape.get(shape(ls), ()):
                sc = score(ls, wslots[wa])
                if sc is not None:
                    scored[la][wa] = sc
        # A matched function referencing both vtables (constructor/destructor) adds evidence.
        for la in scored:
            for lu in self.lin.vt_users.get(la, ()):
                wu = self.l2w.get(lu)
                for wa, sc in scored[la].items():
                    if wu is not None and wu in self.win.vt_users.get(wa, ()):
                        scored[la][wa] = (sc[0] + 10, sc[1] + 1) + sc[2:]
        best_l = {}
        for la, cands in scored.items():
            for wa, sc in cands.items():
                if sc[0] > best_l.get(wa, (None, -1.0))[1]:
                    best_l[wa] = (la, sc[0])

        n_new = 0
        for la, cands in scored.items():
            ranked = sorted(cands.items(), key=lambda kv: -kv[1][0])
            wa, (s1, agree, rho, n) = ranked[0]
            s2 = ranked[1][1][0] if len(ranked) > 1 else 0.0
            if best_l[wa][0] != la or s1 - s2 < max(1.0, 0.15 * s1):
                continue  # not mutual, or not clearly best
            if not agree and (n < 4 or rho < 0.6):
                continue  # structure alone is too thin
            self.vpairs[la], self.vpairs_w[wa] = wa, la
            n_new += self._pair_slots(la, wa, lslots[la], wslots[wa])
        # Constructors/destructors: the only unmatched user of each vtable in a pair.
        for la, wa in self.vpairs.items():
            lu = [u for u in self.lin.vt_users.get(la, ()) if u not in self.l2w]
            wu = [u for u in self.win.vt_users.get(wa, ()) if u not in self.w2l]
            if len(lu) == 1 and len(wu) == 1 and self._plausible(lu[0], wu[0]):
                n_new += self.add(lu[0], wu[0], f"vtable-user:{self.lin.vtables[la][0]}")
        return n_new

    def _pair_slots(self, la, wa, ls, ws):
        name = self.lin.vtables[la][0]
        # MSVC groups overloaded virtuals (in a different order than GCC), so their slots
        # are not positionally comparable; leave them to the other strategies.
        methods = [method_name(self.lname(self.lin.resolve(x))) if is_code(x) else None for x in ls]
        overloaded = {m for m, c in collections.Counter(methods).items() if m and c > 1}
        n_new = 0
        for x, y, meth in zip(ls, ws, methods):
            if not is_code(x) or not is_code(y) or meth in overloaded:
                continue
            n_new += self.add(x, y, f"vtable:{name}")
            rx, ry = self.lin.resolve(x), self.win.resolve(y)
            if (rx, ry) != (x, y):
                n_new += self.add(rx, ry, f"vtable-thunk:{name}")
        return n_new

    # 3. Call graph: unique unmatched callee/caller in a gap between matched anchors.
    def callgraph(self):
        n_new = 0
        for l, w in list(self.l2w.items()):
            lf, wf = self.lin.funcs[l], self.win.funcs[w]
            for lseq, wseq, kind in ((lf.calls, wf.calls, "callee"), (lf.callers, wf.callers, "caller")):
                n_new += self._gaps(lseq, wseq, kind)
        return n_new

    def _gaps(self, lseq, wseq, kind):
        lseq = [a for a in lseq if a in self.lin.funcs]
        wseq = [a for a in wseq if a in self.win.funcs]
        if kind == "caller":  # callers have no order; only the whole set counts as one gap
            lu = [a for a in lseq if a not in self.l2w]
            wu = [a for a in wseq if a not in self.w2l]
            return int(len(lu) == 1 and len(wu) == 1 and self._plausible(lu[0], wu[0])
                       and self.add(lu[0], wu[0], "caller"))
        # Anchors: matched callees appearing in both sequences in the same relative order.
        anchors = [(i, wseq.index(self.l2w[a])) for i, a in enumerate(lseq)
                   if a in self.l2w and self.l2w[a] in wseq]
        anchors = [(-1, -1)] + [p for k, p in enumerate(anchors)
                                if all(p[1] > q[1] for q in anchors[:k])] + [(len(lseq), len(wseq))]
        n_new = 0
        for (li, wi), (lj, wj) in zip(anchors, anchors[1:]):
            lu = [a for a in lseq[li + 1:lj] if a not in self.l2w]
            wu = [a for a in wseq[wi + 1:wj] if a not in self.w2l]
            if len(lu) == 1 and len(wu) == 1 and self._plausible(lu[0], wu[0]):
                n_new += self.add(lu[0], wu[0], "callee")
        return n_new

    def _plausible(self, l, w):
        a, b = self.lin.funcs[l].ninstr, self.win.funcs[w].ninstr
        return min(a, b) * 4 >= max(a, b) or max(a, b) <= 12

    def run(self):
        total = self.seed()
        self.log(f"  seeds: {total}")
        while True:
            v, c = self.vtables(), self.callgraph()
            self.log(f"  +vtable {v}, +callgraph {c}")
            total += v + c
            if not v and not c:
                return total


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--features", type=Path, default=REPO / "ghidra-project/features")
    ap.add_argument("--elf", type=Path, default=REPO / "ghidra-project/loki/elf")
    ap.add_argument("--symbols", type=Path, default=REPO / "tools/loki/symbols")
    ap.add_argument("--out", type=Path, default=Path(__file__).parent / "names")
    ap.add_argument("only", nargs="*", help="Windows binaries to process (default: all in pairs.txt)")
    args = ap.parse_args()

    pairs = collections.defaultdict(list)
    for line in (Path(__file__).parent / "pairs.txt").read_text().splitlines():
        if line.strip() and not line.startswith("#"):
            w, l = line.split()
            pairs[w].append(l)

    args.out.mkdir(exist_ok=True)
    grand = 0
    for wname, lnames in pairs.items():
        if args.only and wname not in args.only:
            continue
        win = Program(args.features / "win" / f"{wname}.tsv")
        rows = {}
        for lname in lnames:
            print(f"{wname} <- {lname}", file=sys.stderr)
            lin = Program(args.features / "linux" / f"{lname}.tsv")
            elf = args.elf / lname if (args.elf / lname).exists() else args.elf / "lib" / lname
            delta = elf_base(elf) - lin.imagebase
            syms = load_symbols(args.symbols / f"{lname}.tsv")
            m = Matcher(lin, win, lambda s: print(s, file=sys.stderr),
                        lambda a: (syms.get(a + delta) or (None, None))[1])
            m.run()
            for l, w in m.l2w.items():
                sym = syms.get(l + delta)
                if sym is None or w in rows:
                    continue  # static (unnamed) Linux function, or already named by an earlier source
                rows[w] = (sym[1], sym[0], lname, m.how[l])
        with open(args.out / f"{wname}.tsv", "w", encoding="utf-8") as f:
            f.write("rva\tdemangled\tmangled\tsource\tmethod\n")
            for w in sorted(rows):
                f.write(f"{w - win.imagebase:06x}\t" + "\t".join(rows[w]) + "\n")
        print(f"{wname}: {len(rows)} / {len(win.funcs)} functions named", file=sys.stderr)
        grand += len(rows)
    print(f"total: {grand} functions named", file=sys.stderr)


if __name__ == "__main__":
    main()
