#!/bin/sh
# each-test-within.sh <seconds> <junit.xml>
#
# Reads a cargo-nextest JUnit report and fails when any test took longer than the bound: one line
# per such test, its time and its name, and exit 1. A report that is missing or names no test
# fails too (exit 2), so a run that wrote nothing is never read as one whose tests were all quick.
# The PostgreSQL lane runs it after its tests (`task check:postgres`, story:postgres-lane-margin-ci).
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: each-test-within.sh <seconds> <junit.xml>" >&2
    exit 2
fi
bound=$1
report=$2
case $bound in
    '' | *[!0-9]*)
        echo "each-test-within: the bound '$bound' is not a whole number of seconds" >&2
        exit 2
        ;;
esac
if [ ! -f "$report" ]; then
    echo "each-test-within: no JUnit report at $report" >&2
    exit 2
fi

# One line per test case: its time, its binary and its name.
cases=$(grep -o '<testcase [^>]*>' "$report" | sed -n \
    's/.* name="\([^"]*\)".* classname="\([^"]*\)".* time="\([0-9.]*\)".*/\3 \2 \1/p') || true
count=$(printf '%s' "$cases" | grep -c . || true)
if [ "$count" -eq 0 ]; then
    echo "each-test-within: $report names no test" >&2
    exit 2
fi

over=$(printf '%s\n' "$cases" | awk -v bound="$bound" '$1 + 0 > bound + 0' | sort -rn)
if [ -n "$over" ]; then
    printf '%s\n' "$over" | while read -r time binary name; do
        echo "each-test-within: $time s, over the bound of $bound s: $binary $name" >&2
    done
    echo "each-test-within: $(printf '%s\n' "$over" | grep -c .) of $count tests over $bound s" >&2
    exit 1
fi
slowest=$(printf '%s\n' "$cases" | sort -rn | head -n 1 | cut -d ' ' -f 1)
echo "each-test-within: $count tests, the slowest $slowest s, each within $bound s"
