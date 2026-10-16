#!/usr/bin/env bash
# fn.sh <hexaddr-or-name>... : print function(s) from extracted/all_decomp.c
for a in "$@"; do
awk -v a="$a" 'index($0,"===== ")==1{p=(index($0," @ " a)>0 || index($0,"===== " a " @")>0)} p' /home/anon/Projects/th_08/specs/opus/extracted/all_decomp.c
done
