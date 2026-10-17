#!/usr/bin/env bash
R="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# fn.sh <hexaddr-or-name>... : print function(s) from extracted/all_decomp.c
for a in "$@"; do
awk -v a="$a" 'index($0,"===== ")==1{p=(index($0," @ " a)>0 || index($0,"===== " a " @")>0)} p' "$R/extracted/all_decomp.c"
done
