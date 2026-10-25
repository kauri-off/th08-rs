#!/bin/zsh
# play_session.sh <name> [--display D] [--no-drawlog] [--out DIR] [--sound keep|off] [--fast] [--game DIR]
# Replays sessions/<name> in the original (probe play mode: Input_Poll returns the recorded words, boot seed forced),
# writes the same files to sessions/<name>/play_<time>/ (or --out) and compares them with the recording.
# Default display: Xvfb :77 (headless). Audio goes to a temporary null sink (--sound off: no audio device at all).
# --fast: probe fast mode (TH08_FAST=1, virtual frame timer). --game DIR: own game copy dir (parallel runs; set
# TH08_PREFIX to an own prefix copy too, it is killed with wineserver -k at the end instead of pkill th08.exe).
emulate -L zsh
setopt no_nomatch

REPO=${0:A:h:h:h}
GAME=$REPO/re/work/game
PGAME=$REPO/re/work/sessplay/game
HOOK=$REPO/re/work/hook
WINE=${TH08_WINE:-wine}
export WINEPREFIX=${TH08_PREFIX:-$REPO/re/work/wine}

disp=:77 drawlog="" out="" sound=keep name="" fast=0 owngame=0
while (( $# )); do
  case $1 in
    --display) disp=$2; shift 2 ;;
    --no-drawlog) drawlog=0; shift ;;
    --drawlog) drawlog=1; shift ;;
    --out) out=$2; shift 2 ;;
    --sound) sound=$2; shift 2 ;;
    --fast) fast=1; shift ;;
    --game) PGAME=${2:A}; owngame=1; shift 2 ;;
    -*) print -u2 "unknown option $1"; exit 2 ;;
    *) name=$1; shift ;;
  esac
done
die() { print -u2 "play_session: $*"; exit 1 }
[[ -n $name ]] || die "usage: play_session.sh <name>"
SESS=${name:A}; [[ -d $SESS ]] || SESS=$REPO/sessions/$name
[[ -f $SESS/polls.txt ]] || die "no recorded session at $SESS"
[[ -f $HOOK/d3d8.dll ]] || die "probe not built (run record_session.sh once)"
(( owngame )) || ! pgrep -A -f 'th08\.exe' >/dev/null || die "th08.exe is already running"
[[ -z $drawlog ]] && drawlog=$(sed -n 's/^drawlog: //p' $SESS/meta.txt)
[[ -z $drawlog ]] && drawlog=0
noclip=$(sed -n 's/^noclip: //p' $SESS/meta.txt); noclip=${noclip:-0}
[[ -z $out ]] && out=$SESS/play_$(date +%Y%m%d-%H%M%S)
mkdir -p $out; out=${out:A}

rm -rf $PGAME; mkdir -p $PGAME/replay
for f in th08.exe th08.dat thbgm.dat; do ln -s $GAME/$f $PGAME/$f; done
cp $HOOK/d3d8.dll $PGAME/
cp $SESS/start/th08.cfg $PGAME/
[[ -f $SESS/start/score.dat ]] && cp $SESS/start/score.dat $PGAME/
for r in $SESS/start/replay/*.rpy(N); do cp -p $r $PGAME/replay/; done
if [[ $sound == off ]]; then
  python3 -I -c "import sys;p=sys.argv[1];b=bytearray(open(p,'rb').read());b[0x1F]=0;b[0x20]=0;open(p,'wb').write(b)" $PGAME/th08.cfg
fi

if [[ $disp != :0 ]] && ! pgrep -A -f "Xvfb $disp( |\$)" >/dev/null; then
  Xvfb $disp -screen 0 1024x768x24 >/dev/null 2>&1 &
  sleep 1
fi
sink_mod="" sink=th08_play_$$
if [[ $sound != off ]] && command -v pactl >/dev/null; then
  sink_mod=$(pactl load-module module-null-sink sink_name=$sink 2>/dev/null)
fi
if [[ -z $sink_mod ]]; then
  [[ $sound != off ]] && print "no null audio sink available: playing with music/SE off"
  python3 -I -c "import sys;p=sys.argv[1];b=bytearray(open(p,'rb').read());b[0x1F]=0;b[0x20]=0;open(p,'wb').write(b)" $PGAME/th08.cfg
fi
presents=$(sed -n 's/^last_present: //p' $SESS/meta.txt)
limit=$(( ${presents:-36000} / 60 * 3 / 2 + 120 ))
print "playing $SESS -> $out (display $disp, draw log $drawlog, noclip $noclip, fast $fast, time limit ${limit}s)"
t0=$(date +%s.%N)
(
  cd $PGAME
  export DISPLAY=$disp WINEDEBUG=-all
  [[ -n $sink_mod ]] && export PULSE_SINK=$sink
  export WINEDLLOVERRIDES='d3d8=n,b;c:\windows\syswow64\d3d8.dll=b'
  [[ $sound == off ]] && WINEDLLOVERRIDES+=';winepulse.drv=d;winealsa.drv=d;wineoss.drv=d'
  export TH08_SESS=play TH08_SESS_IN="Z:${SESS//\//\\}" TH08_SESS_OUT="Z:${out//\//\\}" TH08_SESS_DL=$drawlog TH08_NOCLIP=$noclip TH08_FAST=$fast
  timeout $limit $WINE th08.exe > $out/wine.log 2>&1
)
sleep 1
if (( owngame )); then wineserver -k 2>/dev/null; else pkill -A -f 'th08\.exe' 2>/dev/null; fi
[[ -n $sink_mod ]] && pactl unload-module $sink_mod 2>/dev/null
print "wall_seconds: $(( $(date +%s.%N) - t0 ))"
grep -q "PLAY MISMATCH" $out/probe_log.txt && print "WARNING: poll alignment mismatches, see $out/probe_log.txt"
python3 -I $REPO/re/tools/session.py finalize $out
python3 -I $REPO/re/tools/session.py compare $SESS $out
