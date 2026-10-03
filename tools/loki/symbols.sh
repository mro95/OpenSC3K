#!/usr/bin/env bash
# Dump demangled function symbols of the Loki Linux ELFs (see fetch.sh) to
# tools/loki/symbols/<binary>.tsv and a class index to tools/loki/symbols/classes.tsv.
# Uses Ghidra's bundled binutils 2.24 demangler, the last one that reads GCC 2.95 mangling.
#
# Env: SC3K_LOKI (as in fetch.sh), GHIDRA_HOME (default /opt/ghidra)
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
elf="${SC3K_LOKI:-$repo/ghidra-project/loki}/elf"
demangle="${GHIDRA_HOME:-/opt/ghidra}/GPL/DemanglerGnu/os/linux_x86_64/demangler_gnu_v2_24"
out="$here/symbols"
mkdir -p "$out"

for bin in "$elf/sc3u_demo.x86" "$elf"/lib/*.so; do
  name="$(basename "$bin")"
  nm -D --defined-only "$bin" | awk '$2 ~ /^[TtWw]$/ {print $1 "\t" $3}' | sort -u > "$out/.tmp"
  cut -f2 "$out/.tmp" | "$demangle" -s gnu > "$out/.dem"
  {
    printf 'addr\tmangled\tdemangled\n'
    paste "$out/.tmp" "$out/.dem"
  } > "$out/$name.tsv"
done
rm -f "$out/.tmp" "$out/.dem"

# Class index: binary, class, number of methods defined there.
{
  printf 'binary\tclass\tmethods\n'
  for t in "$out"/*.tsv; do
    [ "$(basename "$t")" = classes.tsv ] && continue
    tail -n +2 "$t" | cut -f3 | grep -oE '^[A-Za-z_][A-Za-z0-9_]*(<[^()]*>)?::' | sed 's/::$//' |
      sort | uniq -c | sed -E "s/^ *([0-9]+) (.*)/$(basename "$t" .tsv)\t\2\t\1/"
  done
} > "$out/classes.tsv"
echo "$(tail -n +2 "$out/classes.tsv" | cut -f2 | sort -u | wc -l) classes -> $out/classes.tsv"
