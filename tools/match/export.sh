#!/usr/bin/env bash
# Export matching features (tools/ghidra/ExportFeatures.java) for every binary in pairs.txt.
# Windows programs live in the Ghidra project root, Loki ELFs in /loki (tools/ghidra/import.sh loki).
#
# Env: SC3K_GHIDRA_PROJECT, SC3K_LOKI, GHIDRA_HOME (see tools/ghidra/import.sh, tools/loki/fetch.sh)
# Usage: tools/match/export.sh [win|linux|all]   (default: all)
# Output: $SC3K_GHIDRA_PROJECT/features/{win,linux}/<binary>.tsv
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
ghidra="${GHIDRA_HOME:-/opt/ghidra}/support/analyzeHeadless"
proj_dir="${SC3K_GHIDRA_PROJECT:-$repo/ghidra-project}"
elf="${SC3K_LOKI:-$proj_dir/loki}/elf"
out="$proj_dir/features"
mkdir -p "$out/win" "$out/linux" "$out/vtables"

pairs=$(grep -v '^#' "$here/pairs.txt" | awk 'NF')

# Linux vtables from __vt_ symbols: offset from the lowest PT_LOAD vaddr, size, mangled name.
for p in $(awk '{print $2}' <<<"$pairs" | sort -u); do
  bin="$elf/$p"; [ -f "$bin" ] || bin="$elf/lib/$p"
  base=$(readelf -l -W "$bin" | awk '$1=="LOAD" {print $3; exit}')
  nm -D -S --defined-only "$bin" | awk -v base="$base" '$4 ~ /^__vt_/ {
      printf "%x\t%d\t%s\n", strtonum("0x" $1) - strtonum(base), strtonum("0x" $2), $4 }' \
    > "$out/vtables/$p.txt"
done

# One headless run per project folder. -process without a name = every program in the folder;
# the root run is non-recursive, so /loki stays out of it. ExportFeatures writes <dir>/<program>.tsv.
what="${1:-all}"
if [ "$what" = all ] || [ "$what" = win ]; then
# Create functions at vtable targets auto-analysis missed (modifies the Windows programs).
"$ghidra" "$proj_dir" SC3U -process -noanalysis -scriptPath "$repo/tools/ghidra" \
  -postScript FixupVtables.java 2>&1 | grep -E "FixupVtables.java>|ERROR|Exception" || true
"$ghidra" "$proj_dir" SC3U -process -readOnly -noanalysis -scriptPath "$repo/tools/ghidra" \
  -postScript ExportFeatures.java "$out/win" 2>&1 | grep -E "ExportFeatures.java>|ERROR|Exception" || true
fi
if [ "$what" = all ] || [ "$what" = linux ]; then
# import.sh loki keeps the ELF directory layout: executable in /loki, libraries in /loki/lib.
"$ghidra" "$proj_dir" SC3U/loki -process -recursive -readOnly -noanalysis -scriptPath "$repo/tools/ghidra" \
  -postScript ExportFeatures.java "$out/linux" "$out/vtables" 2>&1 |
  grep -E "ExportFeatures.java>|ERROR|Exception" || true
fi
