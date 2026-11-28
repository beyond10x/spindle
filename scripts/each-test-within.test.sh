#!/bin/sh
# Cases of scripts/each-test-within.sh on JUnit reports written here: a report whose slowest test
# took the bound passes; one with a test over the bound fails naming it and its time, and only it;
# a failing test's report (a testcase with children) is read as well; a missing report, one that
# names no test and a bound that is not a whole number fail without passing.
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
script=${EACH_TEST_WITHIN:-$root/scripts/each-test-within.sh}
mkdir -p "$root/target"
work=$(mktemp -d "$root/target/each-test-within.XXXXXX")
failures=0
cases=0

fail() {
    echo "each-test-within-test: $*" >&2
    failures=$((failures + 1))
}

report() {
    {
        echo '<?xml version="1.0" encoding="UTF-8"?>'
        echo '<testsuites name="nextest-run" tests="3" failures="0" errors="0">'
        echo '<testsuite name="spindle-cli::spindle-cli" tests="3" disabled="0" errors="0" failures="0">'
        cat
        echo '</testsuite>'
        echo '</testsuites>'
    } >"$work/$1"
}

report within.xml <<'EOF'
<testcase name="postgres::a_quick_case" classname="spindle-cli::spindle-cli" timestamp="2026-11-28T14:38:51.416+00:00" time="0.250"/>
<testcase name="postgres::a_case_at_the_bound" classname="spindle-cli::spindle-cli" timestamp="2026-11-28T14:38:51.416+00:00" time="15.000"/>
EOF
cases=$((cases + 1))
out=$(sh "$script" 15 "$work/within.xml" 2>&1) || fail "a report whose slowest test took 15.000 s failed: $out"
case $out in
    *"2 tests, the slowest 15.000 s, each within 15 s"*) ;;
    *) fail "a passing report printed: $out" ;;
esac

report over.xml <<'EOF'
<testcase name="postgres::a_quick_case" classname="spindle-cli::spindle-cli" timestamp="2026-11-28T14:38:51.416+00:00" time="0.250"/>
<testcase name="adversary_postgres_named::a_slow_case" classname="spindle-kernel::spindle-kernel" timestamp="2026-11-28T14:38:51.416+00:00" time="15.500"/>
<testcase name="postgres::a_case_at_the_bound" classname="spindle-cli::spindle-cli" timestamp="2026-11-28T14:38:51.416+00:00" time="15.000"/>
EOF
cases=$((cases + 1))
status=0
out=$(sh "$script" 15 "$work/over.xml" 2>&1) || status=$?
[ "$status" -eq 1 ] || fail "a report with a 15.500 s test exited $status, want 1: $out"
case $out in
    *"15.500 s, over the bound of 15 s: spindle-kernel::spindle-kernel adversary_postgres_named::a_slow_case"*) ;;
    *) fail "the 15.500 s test is not named with its time: $out" ;;
esac
case $out in
    *a_case_at_the_bound* | *a_quick_case*) fail "a test within the bound is named: $out" ;;
esac

report failed.xml <<'EOF'
<testcase name="postgres::a_failing_slow_case" classname="spindle-host::spindle-host" timestamp="2026-11-28T14:38:51.416+00:00" time="16.125">
<failure type="test failure">thread panicked</failure>
<system-out></system-out>
</testcase>
EOF
cases=$((cases + 1))
status=0
out=$(sh "$script" 15 "$work/failed.xml" 2>&1) || status=$?
[ "$status" -eq 1 ] || fail "a failing 16.125 s test exited $status, want 1: $out"

cases=$((cases + 1))
status=0
out=$(sh "$script" 15 "$work/absent.xml" 2>&1) || status=$?
[ "$status" -eq 2 ] || fail "a missing report exited $status, want 2: $out"

report empty.xml </dev/null
cases=$((cases + 1))
status=0
out=$(sh "$script" 15 "$work/empty.xml" 2>&1) || status=$?
[ "$status" -eq 2 ] || fail "a report naming no test exited $status, want 2: $out"

cases=$((cases + 1))
status=0
out=$(sh "$script" 15.5 "$work/within.xml" 2>&1) || status=$?
[ "$status" -eq 2 ] || fail "the bound 15.5 exited $status, want 2: $out"

rm -rf "$work"
if [ "$failures" -ne 0 ]; then
    echo "each-test-within-test: $failures of $cases case(s) failed" >&2
    exit 1
fi
echo "each-test-within-test: $cases cases passed"
