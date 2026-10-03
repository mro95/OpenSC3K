#!/usr/bin/env bash
# Fetch Loki's free SC3U Linux demo (v2.0, 2000) and unpack its ELF binaries, which keep
# GCC 2.95 C++ symbol names in .dynsym. See docs/re-notes/subsystems.md.
# The .run installer is a Makeself archive; it is unpacked with tar, never executed.
#
# Env: SC3K_LOKI  output dir (default <repo>/ghidra-project/loki, gitignored)
# Output: $SC3K_LOKI/elf/sc3u_demo.x86, $SC3K_LOKI/elf/lib/*.so
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${SC3K_LOKI:-$repo/ghidra-project/loki}"
url=https://ftp.lokigames.twolife.be/updates/loki_demos/sc3u-demo.run
md5=092e88459af9afc011dc4dc5a6e045cc
skip=175  # from the Makeself header: payload starts at this line

mkdir -p "$out"
cd "$out"
[ -f sc3u-demo.run ] || curl -fL -o sc3u-demo.run "$url"
echo "$md5  sc3u-demo.run" | md5sum -c -

rm -rf demo elf
mkdir -p demo elf/lib
tail -n +"$skip" sc3u-demo.run | gzip -cd | tar xf - -C demo
src=demo/data/demos/sc3u_demo
# The binaries are individually gzipped inside the archive.
gzip -cd < "$src/sc3u_demo.x86" > elf/sc3u_demo.x86
for f in "$src"/lib/*.so; do
  gzip -cd < "$f" > "elf/lib/$(basename "$f")"
done
echo "unpacked $(ls elf/lib | wc -l) libraries + sc3u_demo.x86 to $out/elf"
