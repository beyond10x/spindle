#!/bin/sh
# Every place that names the ESS release names the same one: the exact `requires:` of
# ess/ess-inputs.yaml, ESS_TOOLCHAIN in Taskfile.yml and .cargo/config.toml, and ESS_VERSION in
# .github/workflows/ci.yml. Exits 1 naming the file and both values when one differs.
set -eu

root=${1:-.}
inputs="$root/ess/ess-inputs.yaml"
taskfile="$root/Taskfile.yml"
cargo="$root/.cargo/config.toml"
ci="$root/.github/workflows/ci.yml"

read_value() {
    sed -n "$2" "$1" | head -n 1
}

want=$(read_value "$inputs" 's/^requires:[[:space:]]*ess[[:space:]]*\([^[:space:]]*\)[[:space:]]*$/\1/p')
case $want in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "check-ess-pin: $inputs: requires names no exact ess release ('${want}'); write requires: ess <major>.<minor>.<patch>" >&2
        exit 1
        ;;
esac

status=0
check() {
    file=$1
    name=$2
    got=$3
    if [ "$got" != "$want" ]; then
        echo "check-ess-pin: $file: $name is '${got}', $inputs requires ess '$want'" >&2
        status=1
    fi
}

check "$taskfile" ESS_TOOLCHAIN "$(read_value "$taskfile" "s/^[[:space:]]*ESS_TOOLCHAIN:[[:space:]]*['\"]\{0,1\}\([^'\"[:space:]]*\)['\"]\{0,1\}[[:space:]]*\$/\1/p")"
check "$cargo" ESS_TOOLCHAIN "$(read_value "$cargo" 's/^[[:space:]]*ESS_TOOLCHAIN[[:space:]]*=[[:space:]]*"\([^"]*\)"[[:space:]]*$/\1/p')"
check "$ci" ESS_VERSION "$(read_value "$ci" "s/^[[:space:]]*ESS_VERSION:[[:space:]]*['\"]\{0,1\}\([^'\"[:space:]]*\)['\"]\{0,1\}[[:space:]]*\$/\1/p")"

if [ "$status" -eq 0 ]; then
    echo "check-ess-pin: ess $want in $inputs, $taskfile, $cargo and $ci"
fi
exit "$status"
