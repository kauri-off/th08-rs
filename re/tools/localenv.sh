# Sourced (bash or zsh): exports keys of packaging/local/local.env that are not already set.
_r=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)
_f=$_r/packaging/local/local.env
# A worktree has no local.env (gitignored): use the main checkout's.
if [[ ! -f $_f && -f $_r/.git ]]; then
  _g=$(sed -n 's/^gitdir: //p' "$_r/.git")
  [[ $_g == */.git/worktrees/* ]] && _f=${_g%%/.git/worktrees/*}/packaging/local/local.env
fi
if [[ -f $_f ]]; then
  while IFS='=' read -r _k _v; do
    [[ -n $_k && -z $(printenv "$_k") ]] && export "$_k=$_v"
  done < "$_f"
fi
unset _r _f _g _k _v
