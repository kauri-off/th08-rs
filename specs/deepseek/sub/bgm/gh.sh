#!/usr/bin/env bash
# Local copy of specs/ghidra_tools/gh.sh that also searches MY ghidra_scripts dir.
# usage: gh.sh <agent-dir> <Script.java> [args...]   (run serially: one Ghidra process per project)
DIR="$(cd "$1" && pwd)"; shift
SCRIPT="$1"; shift
TOOLS="/home/anon/Projects/th_08/specs/ghidra_tools"
MINE="$DIR/ghidra_scripts"
export GH_OUT="$(mktemp)"
export HOME="${BGM_GHIDRA_HOME:-/tmp/opencode/ghome}"
/opt/ghidra/support/analyzeHeadless "$DIR/ghidra" th08 -process th08.exe -noanalysis \
  -scriptPath "$MINE" -postScript "$SCRIPT" "$@" > "$GH_OUT.log" 2>&1
rc=$?
grep -E '\.java:[0-9]+: error|^ERROR|Unable to lock|LockException' "$GH_OUT.log" >&2
cat "$GH_OUT"; rm -f "$GH_OUT" "$GH_OUT.log"
exit $rc
