#!/usr/bin/env python3
"""Precision report for tools/match/match.py, per matching method.

"comparable" = both functions reference strings; "no-overlap" = they share none (a likely
mismatch, though platform variants such as lokigames.com/simcity.com also land here).
"size rho" = rank correlation of instruction counts over all matches of that method.
Run from the repo root after tools/match/export.sh.
"""
import collections
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from match import Matcher, Program, elf_base, load_symbols, spearman

os.chdir(Path(__file__).resolve().parents[2])
stats = collections.defaultdict(lambda: [0, 0, 0])  # method -> [n, comparable, conflicting]
sizes = collections.defaultdict(lambda: ([], []))
examples = collections.defaultdict(list)
for line in open("tools/match/pairs.txt"):
    if not line.strip() or line.startswith("#"): continue
    w, l = line.split()
    W = Program(f"ghidra-project/features/win/{w}.tsv"); L = Program(f"ghidra-project/features/linux/{l}.tsv")
    elf = Path("ghidra-project/loki/elf") / l
    if not elf.exists(): elf = Path("ghidra-project/loki/elf/lib") / l
    delta = elf_base(elf) - L.imagebase; syms = load_symbols(f"tools/loki/symbols/{l}.tsv")
    m = Matcher(L, W, lambda s: None, lambda a: (syms.get(a + delta) or (None, None))[1]); m.run()
    for la, wa in m.l2w.items():
        kind = m.how[la].split(":")[0]; st = stats[kind]; st[0] += 1
        lt = {t for t in L.funcs[la].tokens if t[0] == "s"}; wt = {t for t in W.funcs[wa].tokens if t[0] == "s"}
        if len(lt) >= 1 and len(wt) >= 1:
            st[1] += 1
            if not lt & wt:
                st[2] += 1
                if len(examples[kind]) < 3:
                    examples[kind].append((w, hex(wa), (syms.get(la + delta) or ("", "?"))[1][:70], sorted(lt)[:3], sorted(wt)[:3]))
        sizes[kind][0].append(L.funcs[la].ninstr); sizes[kind][1].append(W.funcs[wa].ninstr)
print(f"{'method':14} {'matches':>8} {'comparable':>10} {'no-overlap':>10} {'size rho':>8}")
for k, (n, c, x) in sorted(stats.items(), key=lambda kv: -kv[1][0]):
    print(f"{k:14} {n:8} {c:10} {x:10} {spearman(*sizes[k]):8.2f}")
for k, ex in examples.items():
    for e in ex: print(k, e)
