#!/bin/zsh
# usage: run.sh <seconds>   (runs th08 copy with probe d3d8.dll under Xvfb :77)
source /home/anon/Projects/th_08/re/work/env.sh
export DISPLAY=:77 WINEDLLOVERRIDES="d3d8=n,b" WINEDEBUG=-all
pgrep -x Xvfb >/dev/null || (Xvfb :77 -screen 0 1024x768x24 >/dev/null 2>&1 &)
cp /home/anon/Projects/th_08/re/work/hook/d3d8.dll /home/anon/Projects/th_08/re/work/game/
cd /home/anon/Projects/th_08/re/work/game
timeout ${1:-60} $WINE th08.exe > ../wine.log 2>&1
wineserver -k 2>/dev/null
exit 0
