#!/bin/bash
# run_rpy_batch.sh <selection.tsv> <out_root> [-j N] [--drawlog NAMES_FILE] [--log FILE]
# Runs every row of selection.tsv (source, id, md5, category, file[, ...]; file relative to th08_ext_replays) through
# run_rpy.sh into <out_root>/<source>_<id>/. Resumable: finished outputs (meta.txt with rpy_result) are skipped,
# unfinished and timed-out ones are removed and rerun. One log line per replay. NAMES_FILE lists <source>_<id> recorded with --drawlog.
# llvmpipe threads per game: LP_NUM_THREADS (default 2; trace unaffected, best fps per core).
set -u
TOOLS=$(cd "$(dirname "$0")" && pwd)
E=/home/anon/Projects/th08_ext_replays
sel="" root="" jobs=4 dl="" log=""
while (( $# )); do
  case $1 in
    -j) jobs=$2; shift 2 ;;
    --drawlog) dl=$(readlink -f "$2"); shift 2 ;;
    --log) log=$(readlink -f "$2"); shift 2 ;;
    -h|--help) sed -n 2,5p "$0"; exit 0 ;;
    *) if [[ -z $sel ]]; then sel=$(readlink -f "$1"); else root=$(readlink -f "$1"); fi; shift ;;
  esac
done
[[ -f $sel && -n $root ]] || { echo "usage: run_rpy_batch.sh <selection.tsv> <out_root> [-j N]" >&2; exit 2; }
mkdir -p "$root"
[[ -z $log ]] && log=$root/batch.log
export LP_NUM_THREADS=${LP_NUM_THREADS:-2}
export TH08_SLOTS=$(( jobs > 10 ? jobs : 10 )) TOOLS E root log dl
one() {
  local src=$1 id=$2 file=$3 name=$1_$2 out=$root/$1_$2 opt=() t0 line
  if [[ -f $out/meta.txt ]] && grep -q '^rpy_result: [^t]' "$out/meta.txt"; then return 0; fi
  rm -rf "$out"
  [[ -n $dl ]] && grep -qx "$name" "$dl" && opt=(--drawlog)
  # time limit for ~40 fps (many slots in parallel), not run_rpy's 200 fps default
  local tmo=$(( 600 + $(stat -c %s "$E/$file") / 8 ))
  line=$(zsh "$TOOLS/run_rpy.sh" "$E/$file" "$out" --source "th08_ext_replays/$file" --timeout $tmo "${opt[@]}" 2>&1 | tail -1)
  local res=$(sed -n 's/^rpy_result: //p' "$out/meta.txt" 2>/dev/null)
  local fr=$(sed -n 's/^replay_frames: //p' "$out/meta.txt" 2>/dev/null)
  local w=$(sed -n 's/^wall_seconds: //p' "$out/meta.txt" 2>/dev/null)
  printf '%s\t%s\t%s\t%s\tframes %s\twall %s\t%s\n' "$(date +%T)" "$name" "${res:-fail}" "${opt[*]:-}" "${fr:--}" "${w:--}" "$line" >> "$log"
}
export -f one
# largest files first (better slot balance at the end)
tail -n +2 "$sel" | awk -F'\t' '{print $1"\t"$2"\t"$5}' | while IFS=$'\t' read -r s i f; do
  printf '%s\t%s\t%s\t%s\n' "$(stat -c %s "$E/$f")" "$s" "$i" "$f"; done | sort -rn | cut -f2- | xargs -d '\n' -P "$jobs" -I{} bash -c 'IFS=$'"'"'\t'"'"' read -r s i f <<< "$1"; one "$s" "$i" "$f"' _ {}
echo "batch done: $(grep -c . "$log") log lines" >&2
