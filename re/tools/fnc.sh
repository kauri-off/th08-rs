#!/usr/bin/env bash
# fnc.sh <addr>... : print function(s) with referenced _DAT_ constants annotated (f32 / i32)
R="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
D=$R
for a in "$@"; do
  "$D/tools/fn.sh" "$a" > /tmp/fnc_$$.c
  cat /tmp/fnc_$$.c
  grep -oE '_?DAT_00[4][0-9a-f]{5}' /tmp/fnc_$$.c | sed 's/_\?DAT_//' | sort -u | while read x; do
    python3 -I - "$D/tools" "$x" <<'PY'
import sys,struct
sys.path.insert(0,sys.argv[1]); from pe import PE
p=PE(); a=int(sys.argv[2],16)
try:
  b=p.read(a,4); print("  %08x f=%g i=%d"%(a,struct.unpack('<f',b)[0],struct.unpack('<i',b)[0]))
except Exception as e: print("  %08x ?"%a)
PY
  done
done
rm -f /tmp/fnc_$$.c
