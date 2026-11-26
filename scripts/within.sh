#!/bin/sh
# within.sh <seconds> <command> [<argument>...]
#
# Runs the command, prints its wall time against the bound, and exits non-zero when the command
# fails (with its status) or runs longer than the bound (with 124). The command is not stopped at
# the bound: it ends on its own, so its output stays whole, and the step still fails.
set -eu

if [ "$#" -lt 2 ]; then
    echo "usage: within.sh <seconds> <command> [<argument>...]" >&2
    exit 2
fi
bound=$1
shift
case $bound in
    '' | *[!0-9]*)
        echo "within: the bound '$bound' is not a whole number of seconds" >&2
        exit 2
        ;;
esac

now() {
    t=$(date +%s%N)
    case $t in
        *N) echo "$(date +%s)000000000" ;;
        *) echo "$t" ;;
    esac
}

start=$(now)
status=0
"$@" || status=$?
end=$(now)

elapsed_ms=$(((end - start) / 1000000))
elapsed=$(printf '%d.%03d' $((elapsed_ms / 1000)) $((elapsed_ms % 1000)))

if [ "$status" -ne 0 ]; then
    echo "within: $elapsed s of $bound s, the command failed with status $status: $*" >&2
    exit "$status"
fi
if [ "$elapsed_ms" -gt $((bound * 1000)) ]; then
    echo "within: $elapsed s, over the bound of $bound s: $*" >&2
    exit 124
fi
echo "within: $elapsed s of $bound s: $*"
