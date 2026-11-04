# Sourced (bash or zsh): exports keys of packaging/local/local.env that are not already set.
_r=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)
if [[ -f $_r/packaging/local/local.env ]]; then
  while IFS='=' read -r _k _v; do
    [[ -n $_k && -z $(printenv "$_k") ]] && export "$_k=$_v"
  done < "$_r/packaging/local/local.env"
fi
unset _r _k _v
