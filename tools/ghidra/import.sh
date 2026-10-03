#!/usr/bin/env bash
# Import and auto-analyze the SC3U binaries into a headless Ghidra project,
# then export symbol tables to tools/ghidra/exports/ (text, diffable).
#
# Env:
#   SC3K_DATA            original install root (required)
#   GHIDRA_HOME          Ghidra install (default /opt/ghidra)
#   SC3K_GHIDRA_PROJECT  project dir (default <repo>/ghidra-project, gitignored)
#
# Usage: tools/ghidra/import.sh [import|export|loki|all]   (default: all; loki runs separately)
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
: "${SC3K_DATA:?set SC3K_DATA to the SimCity 3000 Unlimited install root}"
ghidra="${GHIDRA_HOME:-/opt/ghidra}/support/analyzeHeadless"
proj_dir="${SC3K_GHIDRA_PROJECT:-$repo/ghidra-project}"
proj_name=SC3U
apps="$SC3K_DATA/Apps"

# The MSVC runtime is imported first so the game binaries can link against it.
# The game DLLs do not import each other; they talk through GZCOM at runtime.
runtime=(MSVCRT.DLL MSVCP60.DLL MSVCIRT.DLL)
# Excluded: ddraw.dll (third-party DDrawCompat wrapper), GZGraphicD2.dll (local modified copy).
game=(
  SC3U.exe UV.DLL AUDIO.DLL GIMEX.DLL MaxisAddOn.dll SimTransit.dll simvariables.dll
  GZGraphicD.dll GZResourceD.dll GZServiceD.dll GZSOUNDD.DLL GZTOOLSD.DLL GZWIND.DLL GZWWWD.DLL
  SCENARIO.DLL SIMADV.DLL SIMBABLD.DLL SIMCITY.DLL SIMDIRT.DLL SIMDSTR.DLL SIMECO.DLL
  SIMGEOM.DLL SIMINIT.DLL SIMMISC.DLL SIMNTWRK.DLL SIMRCI.DLL SIMSERV.DLL SIMSPR.DLL
  SIMUI.DLL SIMUTIL.DLL STRTSIM.DLL Baapp.exe
)

import_all() {
  mkdir -p "$proj_dir"
  local args=()
  for f in "${runtime[@]}"; do args+=(-import "$apps/$f"); done
  "$ghidra" "$proj_dir" "$proj_name" "${args[@]}" -noanalysis -overwrite

  args=()
  for f in "${game[@]}"; do args+=(-import "$apps/$f"); done
  "$ghidra" "$proj_dir" "$proj_name" "${args[@]}" -overwrite \
    -loader PeLoader \
    -loader-linkExistingProjectLibraries true \
    -loader-projectLibrarySearchFolder / \
    -analysisTimeoutPerFile 3600
}

export_all() {
  mkdir -p "$here/exports"
  for f in "${game[@]}"; do
    "$ghidra" "$proj_dir" "$proj_name" -process "$f" -readOnly -noanalysis \
      -scriptPath "$here" -postScript ExportSymbols.java "$here/exports/$f.tsv"
  done
}

# Loki Linux v2.0 ELFs (tools/loki/fetch.sh) into /loki, the name source for Version Tracking.
import_loki() {
  local elf="${SC3K_LOKI:-$repo/ghidra-project/loki}/elf"
  [ -d "$elf" ] || { echo "run tools/loki/fetch.sh first" >&2; exit 1; }
  "$ghidra" "$proj_dir" "$proj_name/loki" -import "$elf/sc3u_demo.x86" -import "$elf/lib" \
    -recursive -overwrite -analysisTimeoutPerFile 3600
}

case "${1:-all}" in
  import) import_all ;;
  export) export_all ;;
  loki) import_loki ;;
  all) import_all; export_all ;;
  *) echo "usage: $0 [import|export|loki|all]" >&2; exit 2 ;;
esac
