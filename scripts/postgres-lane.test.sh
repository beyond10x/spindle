#!/bin/sh
# The cases of scripts/postgres-lane.sh (story:postgres-lane-runs-locally), on real PostgreSQL
# binaries: SPINDLE_PG_BIN or initdb and pg_ctl on PATH. POSIX sh; run as
# `sh scripts/postgres-lane.test.sh [<lane script>]`, the lane script by default the tree's own.
#
#   1. Around `false` the script exits non-zero; its cluster directory is gone afterwards and its
#      postmaster is not alive.
#   2. Around `sleep 30`, sent SIGINT once the cluster answers pg_isready: the same, well before 30 s.
#   3. Binaries that report a major other than 17 on PATH: the script prints that major and a
#      warning naming 17.
#   4. SPINDLE_PG_BIN naming wrappers that log each call and report 17: the cluster is made and
#      started by those wrappers, and no warning is printed (but the locale's, case 6, on a host
#      without en_US.UTF-8).
#   5. With SPINDLE_TEST_DATABASE_URL set, `lane` runs its command and starts no cluster.
#   6. initdb runs with CI's en_US.UTF-8 where locale -a lists it; with a locale -a that lists only
#      C.utf8, with C.utf8 and exactly one warning, naming en_US. (Run after case 4, on its log.)
#   7. `lane` runs its command and its postmaster on 4 of this host's CPUs, as on CI's runner (all
#      of them on a host of 4 or fewer), and on all of them with SPINDLE_PG_LANE_CPUS=0. Skipped
#      without /proc or taskset.
#
# Without initdb or pg_ctl it fails, naming the missing binary.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lane=${1:-$root/scripts/postgres-lane.sh}
case $lane in /*) ;; *) lane=$PWD/$lane ;; esac
work=$root/target/postgres-lane-test
rm -rf "$work"
mkdir -p "$work"

failures=0
cases=0

fail() {
    echo "postgres-lane-test: FAIL: $*" >&2
    failures=$((failures + 1))
}

# The real binaries: SPINDLE_PG_BIN, else PATH.
real_bin() {
    if [ -n "${SPINDLE_PG_BIN:-}" ]; then
        if [ -x "$SPINDLE_PG_BIN/$1" ]; then
            echo "$SPINDLE_PG_BIN/$1"
            return 0
        fi
        return 1
    fi
    command -v "$1" 2>/dev/null
}
for name in initdb pg_ctl postgres createdb pg_isready; do
    if ! real_bin "$name" >/dev/null; then
        echo "postgres-lane-test: no $name ${SPINDLE_PG_BIN:+in SPINDLE_PG_BIN ($SPINDLE_PG_BIN) }on this host; the cases need PostgreSQL 13 or later's binaries on PATH or in SPINDLE_PG_BIN" >&2
        exit 1
    fi
done
pg_isready=$(real_bin pg_isready)

# wrappers <dir> <major> <log>: a directory of initdb, pg_ctl, postgres and createdb that append
# each call to <log> and run the real binary, and answer --version with <major>.
wrappers() {
    mkdir -p "$1"
    for name in initdb pg_ctl postgres createdb; do
        {
            echo '#!/bin/sh'
            echo "echo \"$name \$*\" >> '$3'"
            echo "if [ \"\${1:-}\" = --version ]; then echo '$name (PostgreSQL) $2.0'; exit 0; fi"
            echo "exec '$(real_bin "$name")' \"\$@\""
        } >"$1/$name"
        chmod 755 "$1/$name"
    done
}

# The line the script prints once its cluster is up: the cluster directory, the port and the PID.
ready_field() {
    sed -n "s/^postgres-lane: ready .* $1 \([^ ]*\).*/\1/p" "$2" | head -n 1
}

# left_behind <case> <out>: the cluster directory the script printed is gone and its postmaster
# is not alive.
left_behind() {
    dir=$(ready_field cluster "$2")
    pid=$(ready_field postmaster "$2")
    cases=$((cases + 1))
    if [ -z "$dir" ] || [ -z "$pid" ]; then
        fail "$1: the script printed no ready line with its cluster and postmaster: $(cat "$2")"
        return
    fi
    if [ -e "$dir" ]; then
        fail "$1: the cluster directory $dir is still there"
    fi
    cases=$((cases + 1))
    if kill -0 "$pid" 2>/dev/null; then
        fail "$1: the postmaster $pid is still alive"
        kill -KILL "$pid" 2>/dev/null || true
    fi
    cases=$((cases + 1))
    if [ -n "$(find "$work/$1/target" -maxdepth 1 -name 'postgres-lane.*' -type d 2>/dev/null)" ]; then
        fail "$1: a directory target/postgres-lane.* is left: $(find "$work/$1/target" -maxdepth 1 -name 'postgres-lane.*' -type d)"
    fi
}

# --- 1. around false ---------------------------------------------------------------------------
mkdir -p "$work/false"
status=0
(cd "$work/false" && sh "$lane" run -- false) >"$work/false.out" 2>&1 || status=$?
cases=$((cases + 1))
[ "$status" -ne 0 ] || fail "false: the script exited 0 around false"
left_behind false "$work/false.out"

# --- 2. around sleep 30, SIGINT once pg_isready answers -----------------------------------------
mkdir -p "$work/sigint"
# A command started with & in a shell without job control ignores SIGINT, and a shell cannot trap
# a signal it was started ignoring: env --default-signal=INT gives the script SIGINT as a terminal would.
if ! env --default-signal=INT true 2>/dev/null; then
    echo "postgres-lane-test: this env has no --default-signal (GNU coreutils 8.31 or later); the SIGINT case needs it" >&2
    exit 1
fi
start=$(date +%s)
(cd "$work/sigint" && exec env --default-signal=INT sh "$lane" run -- sleep 30) >"$work/sigint.out" 2>&1 &
script=$!
port=
answered=no
i=0
while [ "$i" -lt 300 ]; do
    port=$(ready_field port "$work/sigint.out")
    if [ -n "$port" ] && "$pg_isready" -q -h 127.0.0.1 -p "$port"; then
        answered=yes
        break
    fi
    if ! kill -0 "$script" 2>/dev/null; then
        break
    fi
    sleep 0.1
    i=$((i + 1))
done
cases=$((cases + 1))
[ "$answered" = yes ] || fail "sigint: the cluster never answered pg_isready: $(cat "$work/sigint.out")"
kill -INT "$script" 2>/dev/null || true
status=0
wait "$script" || status=$?
took=$(($(date +%s) - start))
cases=$((cases + 1))
[ "$status" -ne 0 ] || fail "sigint: the script exited 0 after SIGINT"
cases=$((cases + 1))
[ "$took" -lt 25 ] || fail "sigint: the script ran $took s after SIGINT, as if it waited for sleep 30"
left_behind sigint "$work/sigint.out"

# --- 3. a major other than 17 on PATH ----------------------------------------------------------
wrappers "$work/bin16" 16 "$work/bin16.log"
mkdir -p "$work/major16"
status=0
(cd "$work/major16" && unset SPINDLE_PG_BIN && PATH="$work/bin16:$PATH" sh "$lane" run -- true) >"$work/major16.out" 2>&1 || status=$?
cases=$((cases + 1))
[ "$status" -eq 0 ] || fail "major16: the script exited $status: $(cat "$work/major16.out")"
cases=$((cases + 1))
grep -q 'PostgreSQL 16' "$work/major16.out" || fail "major16: the script did not print major 16: $(cat "$work/major16.out")"
cases=$((cases + 1))
grep -i 'warning' "$work/major16.out" | grep -q '17' || fail "major16: the script printed no warning naming 17: $(cat "$work/major16.out")"
left_behind major16 "$work/major16.out"

# --- 4. SPINDLE_PG_BIN names wrappers that report 17 -------------------------------------------
wrappers "$work/bin17" 17 "$work/bin17.log"
mkdir -p "$work/major17"
status=0
(cd "$work/major17" && SPINDLE_PG_BIN="$work/bin17" sh "$lane" run -- true) >"$work/major17.out" 2>&1 || status=$?
cases=$((cases + 1))
[ "$status" -eq 0 ] || fail "major17: the script exited $status: $(cat "$work/major17.out")"
cases=$((cases + 1))
grep -q '^initdb .*-D' "$work/bin17.log" 2>/dev/null || fail "major17: the cluster was not made by SPINDLE_PG_BIN's initdb: $(cat "$work/bin17.log" 2>/dev/null)"
cases=$((cases + 1))
grep -q '^pg_ctl start' "$work/bin17.log" 2>/dev/null || fail "major17: the cluster was not started by SPINDLE_PG_BIN's pg_ctl: $(cat "$work/bin17.log" 2>/dev/null)"
# Where this host lists no en_US.UTF-8 the script warns about the locale alone (case 6).
en=$(locale -a 2>/dev/null | grep -i -x 'en_US\.utf-\{0,1\}8' | head -n 1 || true)
cases=$((cases + 1))
if [ -n "$en" ]; then
    warned=$(grep -i 'warning' "$work/major17.out" || true)
else
    warned=$(grep -i 'warning' "$work/major17.out" | grep -v '^postgres-lane: warning: locale ' || true)
fi
[ -z "$warned" ] || fail "major17: the script warned on 17: $warned"
left_behind major17 "$work/major17.out"

# --- 6. the locale: CI's en_US.UTF-8 where locale -a lists it, else C.UTF-8 and one warning ------
cases=$((cases + 1))
if [ -n "$en" ]; then
    grep -q -e "^initdb .*--locale=$en " "$work/bin17.log" ||
        fail "locale: this host lists $en and initdb ran without it: $(grep '^initdb .*-D' "$work/bin17.log")"
else
    [ "$(grep -c '^postgres-lane: warning: locale ' "$work/major17.out")" -eq 1 ] ||
        fail "locale: this host lists no en_US.UTF-8 and the script did not warn once: $(cat "$work/major17.out")"
fi
# A host whose locale -a lists C.utf8 and no en_US: C.utf8, and one warning naming en_US.
mkdir -p "$work/no-en/bin" "$work/no-en/run"
{
    echo '#!/bin/sh'
    printf '%s\n' 'printf "C\nC.utf8\nPOSIX\nde_DE.utf8\n"'
} >"$work/no-en/bin/locale"
chmod 755 "$work/no-en/bin/locale"
: >"$work/bin17.log"
status=0
(cd "$work/no-en/run" && SPINDLE_PG_BIN="$work/bin17" PATH="$work/no-en/bin:$PATH" sh "$lane" run -- true) >"$work/no-en.out" 2>&1 || status=$?
cases=$((cases + 1))
[ "$status" -eq 0 ] || fail "no-en: the script exited $status: $(cat "$work/no-en.out")"
cases=$((cases + 1))
grep -q '^initdb .*--locale=C\.utf8 ' "$work/bin17.log" || fail "no-en: initdb did not run with C.utf8: $(grep '^initdb .*-D' "$work/bin17.log")"
cases=$((cases + 1))
[ "$(grep -c -i 'warning' "$work/no-en.out")" -eq 1 ] && grep -i 'warning' "$work/no-en.out" | grep -q 'en_US' ||
    fail "no-en: the script did not print exactly one warning naming en_US: $(grep -i warning "$work/no-en.out")"
dir=$(ready_field cluster "$work/no-en.out")
cases=$((cases + 1))
[ -n "$dir" ] && [ ! -e "$dir" ] || fail "no-en: the cluster directory '$dir' is left or was never printed"

# --- 5. with SPINDLE_TEST_DATABASE_URL set, lane starts no cluster ------------------------------
: >"$work/given.log"
wrappers "$work/given-bin" 17 "$work/given.log"
mkdir -p "$work/given"
status=0
(cd "$work/given" && SPINDLE_PG_BIN="$work/given-bin" SPINDLE_TEST_DATABASE_URL=postgres://given SPINDLE_TEST_DATABASE_CA=/given.pem \
    sh "$lane" lane -- sh -c 'echo "url=$SPINDLE_TEST_DATABASE_URL"') >"$work/given.out" 2>&1 || status=$?
cases=$((cases + 1))
[ "$status" -eq 0 ] || fail "given: lane exited $status: $(cat "$work/given.out")"
cases=$((cases + 1))
grep -q '^url=postgres://given$' "$work/given.out" || fail "given: the command did not run with the given database: $(cat "$work/given.out")"
cases=$((cases + 1))
[ ! -s "$work/given.log" ] || fail "given: lane called PostgreSQL binaries: $(cat "$work/given.log")"
cases=$((cases + 1))
if [ -d "$work/given/target" ] && [ -n "$(find "$work/given/target" -maxdepth 1 -name 'postgres-lane.*' 2>/dev/null)" ]; then
    fail "given: lane made $(find "$work/given/target" -maxdepth 1 -name 'postgres-lane.*')"
fi

# --- 7. lane runs its cluster and its command on CI's 4 CPUs -----------------------------------
# count_cpus <list>: the CPUs a Cpus_allowed_list names.
count_cpus() {
    echo "$1" | awk '{
        n = 0
        parts = split($0, part, ",")
        for (i = 1; i <= parts; i++) {
            if (split(part[i], range, "-") == 2) n += range[2] - range[1] + 1
            else n++
        }
        print n
    }'
}
show_cpus='echo "command $(sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/self/status)"
for f in target/postgres-lane.*/data/postmaster.pid; do
    echo "postmaster $(sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/$(sed -n 1p "$f")/status)"
done'
host=$(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' /proc/self/status 2>/dev/null || true)
if [ -z "$host" ] || ! command -v taskset >/dev/null 2>&1; then
    echo "postgres-lane-test: no /proc or taskset here: the 4-CPU case is skipped"
else
    own=$(count_cpus "$host")
    want=4
    [ "$own" -gt "$want" ] || want=$own
    for setting in default 0; do
        mkdir -p "$work/cpus-$setting"
        status=0
        if [ "$setting" = default ]; then
            (cd "$work/cpus-$setting" && unset SPINDLE_TEST_DATABASE_URL SPINDLE_PG_LANE_CPUS && sh "$lane" lane -- sh -c "$show_cpus") >"$work/cpus-$setting.out" 2>&1 || status=$?
            expect=$want
        else
            (cd "$work/cpus-$setting" && unset SPINDLE_TEST_DATABASE_URL && SPINDLE_PG_LANE_CPUS=0 sh "$lane" lane -- sh -c "$show_cpus") >"$work/cpus-$setting.out" 2>&1 || status=$?
            expect=$own
        fi
        cases=$((cases + 1))
        [ "$status" -eq 0 ] || fail "cpus-$setting: lane exited $status: $(cat "$work/cpus-$setting.out")"
        for who in command postmaster; do
            list=$(sed -n "s/^$who //p" "$work/cpus-$setting.out" | head -n 1)
            cases=$((cases + 1))
            [ -n "$list" ] && [ "$(count_cpus "$list")" -eq "$expect" ] ||
                fail "cpus-$setting: the $who runs on CPUs '$list', want $expect of this host's $own: $(cat "$work/cpus-$setting.out")"
        done
    done
fi

if [ "$failures" -ne 0 ]; then
    echo "postgres-lane-test: $failures of $cases case(s) failed; the outputs are in $work" >&2
    exit 1
fi
rm -rf "$work"
echo "postgres-lane-test: $cases cases passed"
