export WINEPREFIX=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../work" && pwd)/wine
export WINE=wine
export WINEDEBUG=-all
export WINEARCH=win64
