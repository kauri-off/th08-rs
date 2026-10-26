#!/bin/zsh
# run_rpy.sh <file.rpy> <out_dir> [--drawlog] [--timeout S] [--cfg FILE] [--score FILE] [--source LABEL] [--normal]
# Plays one replay file in the original (off-screen, fast mode, sound off) and records it as a normal session in
# <out_dir> (meta.txt, polls.txt, input.txt, events.txt, trace.txt.xz, start/ with the rpy, [draw_*.drawlog.xz]).
# Each run takes a free slot re/work/runs/rpy/slot<N> (own game dir, own wine prefix copy, own Xvfb :1<N>), so several
# runs can go in parallel. Probe: TH08_SESS=rec, TH08_FAST=1, TH08_AUTOKEY=replay (Replay -> the file -> first stage).
# Result line in meta.txt: rpy_result: accepted | rejected | error | timeout.
emulate -L zsh
setopt no_nomatch

REPO=${0:A:h:h:h}
GAME=$REPO/re/work/game
HOOK=$REPO/re/work/hook
PROBE=$REPO/re/probe
SLOTS=$REPO/re/work/runs/rpy
WINE=${TH08_WINE:-wine}
CFG=$REPO/sessions/golden/start/th08.cfg
SCORE=/home/anon/Projects/th08_ext_replays/score_full_unlock/score.dat

drawlog=0 tmo="" rpy="" out="" srclabel="" fast=1
while (( $# )); do
  case $1 in
    --drawlog) drawlog=1; shift ;;
    --no-drawlog) drawlog=0; shift ;;
    --timeout) tmo=$2; shift 2 ;;
    --cfg) CFG=${2:A}; shift 2 ;;
    --score) SCORE=${2:A}; shift 2 ;;
    --source) srclabel=$2; shift 2 ;;
    --normal) fast=0; shift ;;
    -h|--help) sed -n 2,7p $0; exit 0 ;;
    -*) print -u2 "unknown option $1"; exit 2 ;;
    *) if [[ -z $rpy ]]; then rpy=${1:A}; else out=${1:A}; fi; shift ;;
  esac
done
die() { print -u2 "run_rpy: $*"; exit 1 }
[[ -f $rpy && -n $out ]] || die "usage: run_rpy.sh <file.rpy> <out_dir> [--drawlog]"
[[ -e $out ]] && die "$out already exists"
[[ -f $CFG && -f $SCORE ]] || die "missing cfg $CFG or score $SCORE"

# probe: rebuild from re/probe when a source is newer than the dll (serialised with a lock dir)
mkdir -p $HOOK $SLOTS
until mkdir $SLOTS/build.lock 2>/dev/null; do sleep 0.2; done
for f in hook.c drawlog.inc session.inc d3d8.def; do
  [[ ! -f $HOOK/$f || $PROBE/$f -nt $HOOK/$f ]] && cp $PROBE/$f $HOOK/$f
done
if [[ ! -f $HOOK/d3d8.dll || $HOOK/hook.c -nt $HOOK/d3d8.dll || $HOOK/drawlog.inc -nt $HOOK/d3d8.dll || $HOOK/session.inc -nt $HOOK/d3d8.dll ]]; then
  (cd $HOOK && i686-w64-mingw32-gcc -O2 -shared -o d3d8.dll hook.c d3d8.def -static-libgcc -Wl,--enable-stdcall-fixup) || { rmdir $SLOTS/build.lock; die "probe build failed"; }
fi
rmdir $SLOTS/build.lock

# slot: first free of 0..TH08_SLOTS-1 (default 10)
slot=""
for n in {0..$(( ${TH08_SLOTS:-10} - 1 ))}; do mkdir $SLOTS/slot$n.lock 2>/dev/null && { slot=$n; break }; done
[[ -n $slot ]] || die "no free slot in $SLOTS"
SD=$SLOTS/slot$slot PG=$SLOTS/slot$slot/game PFX=$SLOTS/slot$slot/pfx disp=:1$slot
xvfb_pid=""
cleanup() {
  WINEPREFIX=$PFX wineserver -k 2>/dev/null
  [[ -n $xvfb_pid ]] && kill $xvfb_pid 2>/dev/null
  rmdir $SLOTS/slot$slot.lock 2>/dev/null
}
trap 'cleanup; exit 130' INT TERM
mkdir -p $SD
[[ -d $PFX ]] || cp -a $REPO/re/work/wine $PFX || { cleanup; die "prefix copy failed"; }

rm -rf $PG; mkdir -p $PG/replay
for f in th08.exe th08.dat thbgm.dat; do ln -s $GAME/$f $PG/$f; done
cp $HOOK/d3d8.dll $PG/
cp $CFG $PG/th08.cfg
python3 -I -c "import sys;p=sys.argv[1];b=bytearray(open(p,'rb').read());b[0x22]=1;b[0x1F]=0;b[0x20]=0;open(p,'wb').write(b)" $PG/th08.cfg
cp $SCORE $PG/score.dat
cp $rpy $PG/replay/th8_ud0001.rpy

mkdir -p $out/start/replay $out/end $out/replays
cp $PG/th08.cfg $PG/score.dat $out/start/
cp $PG/replay/th8_ud0001.rpy $out/start/replay/
(cd $out/start/replay && md5sum *.rpy > $out/start/replay.md5)
md5=$(md5sum < $rpy | cut -d' ' -f1)
size=$(stat -c %s $rpy)
# time limit: ~200 fps worst case over the frames the file could hold, plus boot/menu/loading
[[ -z $tmo ]] && tmo=$(( 240 + size / 40 ))
{
  print "name: ${out:t}"
  print "recorded: $(date -Iseconds)"
  print "wine: $WINE"
  print "prefix: $PFX"
  print "display: $disp (Xvfb)"
  print "renderer: wined3d (wine builtin d3d8 behind the probe)"
  print "drawlog: $drawlog"
  print "noclip: 0"
  print "sound: off (no audio driver)"
  print "fast: $fast"
  print "score_source: $SCORE"
  print "cfg_source: $CFG"
  print "cfg_patch: +0x22 windowed=1, +0x1F music=0, +0x20 SE=0"
  print "rpy_source: ${srclabel:-$rpy}"
  print "rpy_file: $rpy"
  print "rpy_md5: $md5"
  print "rpy_size: $size"
} > $out/meta.txt

Xvfb $disp -screen 0 1024x768x24 >/dev/null 2>&1 &
xvfb_pid=$!
sleep 1
t0=$(date +%s.%N)
(
  cd $PG
  export WINEPREFIX=$PFX DISPLAY=$disp WINEDEBUG=-all
  export WINEDLLOVERRIDES='d3d8=n,b;c:\windows\syswow64\d3d8.dll=b;winepulse.drv=d;winealsa.drv=d;wineoss.drv=d'
  export TH08_SESS=rec TH08_SESS_OUT="Z:${out//\//\\}" TH08_SESS_DL=$drawlog TH08_SESS_CHUNK=1800 TH08_NOCLIP=0
  export TH08_FAST=$fast TH08_AUTOKEY=replay
  timeout $tmo $WINE th08.exe > $out/wine.log 2>&1
)
rc=$?
wall=$(( $(date +%s.%N) - t0 ))
sleep 0.5
cleanup
trap - INT TERM

cp $PG/th08.cfg $out/end/ 2>/dev/null
cp $PG/score.dat $out/end/ 2>/dev/null
[[ -f $PG/log.txt ]] && iconv -f cp932 -t utf-8 $PG/log.txt > $out/end/log.txt 2>/dev/null
result=error
if grep -q '# autokey: exit (end)' $out/probe_log.txt 2>/dev/null; then result=accepted
elif grep -q '# autokey: exit (rejected)' $out/probe_log.txt 2>/dev/null; then result=rejected
elif (( rc == 124 )); then result=timeout
fi
start=$(sed -n 's/.*autokey: replay start at poll \([0-9]*\).*/\1/p' $out/probe_log.txt | head -1)
end=$(sed -n 's/.*autokey: replay end, scene [0-9]* at poll \([0-9]*\).*/\1/p' $out/probe_log.txt | head -1)
python3 -I $REPO/re/tools/session.py finalize $out >/dev/null
{
  print "rpy_result: $result"
  print "wine_exit: $rc"
  print "replay_start_poll: ${start:--}"
  print "replay_end_poll: ${end:--}"
  [[ -n $start && -n $end ]] && print "replay_frames: $(( end - start ))"
  print "wall_seconds: $(LC_ALL=C printf %.1f $wall)"
} >> $out/meta.txt
lp=$(sed -n 's/^last_present: //p' $out/meta.txt)
print "run_rpy: ${rpy:t} -> $out: $result, ${lp:-0} presents, $(LC_ALL=C printf %.1f $wall) s (slot $slot)"
