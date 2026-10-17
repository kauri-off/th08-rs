#!/usr/bin/env bash
# redump.sh: ApplySymbols(symbols.csv) then re-export all_decomp.c, callgraph.tsv, globals_xref.tsv
set -e
R="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
D=$R
X=$D/extracted
"$D/tools/gh.sh" ApplySymbolsOpus.java "$D/symbols.csv" | tail -5
"$D/tools/gh.sh" DumpAll2.java "$X"
for f in all_decomp.c callgraph.tsv globals_xref.tsv; do
  [ -s "$X/$f.tmp" ] && mv "$X/$f.tmp" "$X/$f"
done
ls -l --time-style=+%H:%M "$X/all_decomp.c" "$X/callgraph.tsv" "$X/globals_xref.tsv"
