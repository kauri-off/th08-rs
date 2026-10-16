#!/usr/bin/env bash
# gh.sh <Script.java> [args...]  -- runs on opus project with both script paths
DIR=/home/anon/Projects/th_08/specs/opus
SCRIPT="$1"; shift
export GH_OUT="$(mktemp)"
/opt/ghidra/support/analyzeHeadless "$DIR/ghidra" th08 -process th08.exe -noanalysis \
  -scriptPath "/home/anon/Projects/th_08/specs/ghidra_tools;$DIR/ghidra_scripts" -postScript "$SCRIPT" "$@" > "$GH_OUT.log" 2>&1
rc=$?
grep -E '\.java:[0-9]+: error|^ERROR|Unable to lock|LockException' "$GH_OUT.log" >&2
cat "$GH_OUT"; rm -f "$GH_OUT" "$GH_OUT.log"
exit $rc
