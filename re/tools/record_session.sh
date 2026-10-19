#!/bin/zsh
# record_session.sh [--score FILE] [--no-drawlog] [--sound on|off|keep] [--display D] [--chunk N] [--noclip] [name]
# Plays the original th08 (re/work/game copy, probe d3d8.dll in record mode) and saves the session to sessions/<name>/.
# User guide: sessions/README.md. File formats: specs/data/drawlog/README.md ("Sessions").
emulate -L zsh
setopt no_nomatch

REPO=${0:A:h:h:h}
GAME=$REPO/re/work/game
HOOK=$REPO/re/work/hook
PROBE=$REPO/re/probe
WINE=${TH08_WINE:-$HOME/PortProton/data/dist/WINE_LG_11-1/bin/wine}
export WINEPREFIX=${TH08_PREFIX:-$HOME/PortProton/data/prefixes/DEFAULT}

noclip=0 score="" drawlog=1 sound=on disp=${DISPLAY:-:0} chunk=1800 name=""
while (( $# )); do
  case $1 in
    --score) score=$2; shift 2 ;;
    --no-drawlog) drawlog=0; shift ;;
    --drawlog) drawlog=1; shift ;;
    --sound) sound=$2; shift 2 ;;
    --display) disp=$2; shift 2 ;;
    --chunk) chunk=$2; shift 2 ;;
    --noclip) noclip=1; shift ;;
    -h|--help) sed -n 2,4p $0; exit 0 ;;
    -*) print -u2 "unknown option $1"; exit 2 ;;
    *) name=$1; shift ;;
  esac
done
[[ -z $name ]] && name=$(date +%Y%m%d-%H%M%S)
SESS=$REPO/sessions/$name

die() { print -u2 "record_session: $*"; exit 1 }
[[ -x $WINE ]] || die "wine not found: $WINE"
[[ -d $WINEPREFIX ]] || die "wine prefix not found: $WINEPREFIX"
[[ -f $GAME/th08.exe && -f $GAME/th08.dat ]] || die "game copy missing in $GAME"
[[ -e $SESS ]] && die "session $SESS already exists"
[[ $sound == (on|off|keep) ]] || die "--sound must be on, off or keep"
pgrep -A -f 'th08\.exe' >/dev/null && die "th08.exe is already running"
if [[ -n $score ]]; then [[ -f $score ]] || die "no such score file: $score"; fi

# probe: rebuild from re/probe when a source is newer than the dll
mkdir -p $HOOK
for f in hook.c drawlog.inc session.inc d3d8.def; do
  [[ ! -f $HOOK/$f || $PROBE/$f -nt $HOOK/$f ]] && cp $PROBE/$f $HOOK/$f
done
if [[ ! -f $HOOK/d3d8.dll || $HOOK/hook.c -nt $HOOK/d3d8.dll || $HOOK/drawlog.inc -nt $HOOK/d3d8.dll || $HOOK/session.inc -nt $HOOK/d3d8.dll ]]; then
  print "building probe d3d8.dll"
  (cd $HOOK && i686-w64-mingw32-gcc -O2 -shared -o d3d8.dll hook.c d3d8.def -static-libgcc -Wl,--enable-stdcall-fixup) || die "probe build failed"
fi
cp $HOOK/d3d8.dll $GAME/d3d8.dll

# starting state
[[ -n $score ]] && cp $score $GAME/score.dat
python3 -I - $GAME/th08.cfg $sound <<'EOF' || die "th08.cfg patch failed"
import sys
p, sound = sys.argv[1], sys.argv[2]
b = bytearray(open(p, 'rb').read())
b[0x22] = 1                      # windowed
if sound == 'on': b[0x1F], b[0x20] = 1, 1    # music WAV (thbgm.dat), sound effects on
if sound == 'off': b[0x1F], b[0x20] = 0, 0
open(p, 'wb').write(b)
EOF
mkdir -p $SESS/start/replay $SESS/replays $SESS/end || die "cannot create $SESS"
cp $GAME/th08.cfg $SESS/start/
[[ -f $GAME/score.dat ]] && cp $GAME/score.dat $SESS/start/
mkdir -p $GAME/replay
for r in $GAME/replay/*.rpy(N); do cp -p $r $SESS/start/replay/; done
rpys=($GAME/replay/*.rpy(N)); : > $SESS/start/replay.md5
(( ${#rpys} )) && (cd $GAME/replay && md5sum *.rpy > $SESS/start/replay.md5)
start_ts=$(date +%s)
{
  print "name: $name"
  print "recorded: $(date -Iseconds)"
  print "wine: $WINE"
  print "prefix: $WINEPREFIX"
  print "display: $disp"
  print "renderer: wined3d (wine builtin d3d8 behind the probe)"
  print "drawlog: $drawlog"
  print "noclip: $noclip"
  print "sound: $sound"
  print "score_source: ${score:-re/work/game/score.dat}"
  print "cfg_patch: +0x22 windowed=1$([[ $sound == on ]] && print ', +0x1F music=1 (WAV), +0x20 SE=1')$([[ $sound == off ]] && print ', +0x1F music=0, +0x20 SE=0')"
} > $SESS/meta.txt

# background compressor for finished draw-log chunks
compress_done() {
  local files=($SESS/draw_*.txt(N)) f
  (( ${#files} > ${1:-1} )) || return
  for f in ${files[1,-$(( ${1:-1} + 1 ))]}; do
    nice -n 19 xz -T4 -3 -c $f > ${f%.txt}.drawlog.xz.part && mv ${f%.txt}.drawlog.xz.part ${f%.txt}.drawlog.xz && rm -f $f
  done
}
if (( drawlog )); then
  ( while :; do compress_done 1; sleep 5; done ) &
  comp_pid=$!
fi

print "recording session '$name' -> $SESS$( (( noclip )) && print ' (noclip: the player cannot die)')"
print "play normally; quit the game (Quit on the title screen or close the window) to finish."
win_out="Z:${SESS//\//\\}"
(
  cd $GAME
  export DISPLAY=$disp WINEDEBUG=-all
  export WINEDLLOVERRIDES='d3d8=n,b;c:\windows\syswow64\d3d8.dll=b'
  export TH08_SESS=rec TH08_SESS_OUT=$win_out TH08_SESS_DL=$drawlog TH08_SESS_CHUNK=$chunk TH08_NOCLIP=$noclip
  exec $WINE th08.exe > $SESS/wine.log 2>&1
) &
wine_pid=$!
trap 'print "interrupted: stopping the game"; pkill -A -f "th08\.exe"; ' INT TERM
wait $wine_pid
trap - INT TERM
sleep 1
pkill -A -f 'th08\.exe' 2>/dev/null
[[ -n $comp_pid ]] && kill $comp_pid 2>/dev/null && wait $comp_pid 2>/dev/null
rm -f $SESS/*.part(N)

# results: new/changed replays, end state
(cd $GAME/replay && for r in *.rpy(N); do
  grep -q "$(md5sum $r)" $SESS/start/replay.md5 2>/dev/null || cp -p $r $SESS/replays/
done)
cp $GAME/th08.cfg $SESS/end/
[[ -f $GAME/score.dat ]] && cp $GAME/score.dat $SESS/end/
[[ -f $GAME/score.txt && $(stat -c %Y $GAME/score.txt) -ge $start_ts ]] && cp $GAME/score.txt $SESS/end/
print "elapsed_seconds: $(( $(date +%s) - start_ts ))" >> $SESS/meta.txt
print "replays_saved: $(cd $SESS/replays && print -l *.rpy(N) | tr '\n' ' ')" >> $SESS/meta.txt
python3 -I $REPO/re/tools/session.py finalize $SESS
print "done: $SESS"
