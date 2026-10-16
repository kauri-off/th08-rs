#!/usr/bin/env bash
# usage: gh.sh <agent-dir> <Script.java> [args...]   (run serially: one Ghidra process per project)
DIR="$(cd "$1" && pwd)"; shift
SCRIPT="$1"; shift
TOOLS="$(cd "$(dirname "$0")" && pwd)"
export GH_OUT="$(mktemp)"
/opt/ghidra/support/analyzeHeadless "$DIR/ghidra" th08 -process th08.exe -noanalysis \
  -scriptPath "$TOOLS" -postScript "$SCRIPT" "$@" > "$GH_OUT.log" 2>&1
rc=$?
grep -E '\.java:[0-9]+: error|^ERROR|Unable to lock|LockException' "$GH_OUT.log" >&2
cat "$GH_OUT"; rm -f "$GH_OUT" "$GH_OUT.log"
exit $rc
