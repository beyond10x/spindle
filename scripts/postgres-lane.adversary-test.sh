#!/bin/sh
# Adversarial cases for scripts/check-postgres-options.sh and scripts/postgres-lane.sh
# (story:postgres-lane-runs-locally, wave 32 unit U1). POSIX sh.
#
#   postgres-lane.adversary-test.sh options [<check script>]
#       copies of this tree's ci.yml and postgres-lane.sh, each with one more server option set in
#       CI's PostgreSQL step in a form other than `-c name=value`, or with the options read and not
#       used, or overridden later in the script; the check must refuse each one
#   postgres-lane.adversary-test.sh lane [<lane script>]
#       the lane script on real PostgreSQL binaries (SPINDLE_PG_BIN, else PATH): lane's own
#       left-behind assertion, SIGTERM to run, Ctrl-C through lane, a failing initdb, a failing
#       pg_ctl start, a port taken by a second lane, and an empty SPINDLE_TEST_DATABASE_URL
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
failures=0
cases=0

fail() {
    echo "postgres-lane-adversary: FAIL: $*" >&2
    failures=$((failures + 1))
}

finish() {
    if [ "$failures" -ne 0 ]; then
        echo "postgres-lane-adversary: $failures of $cases case(s) failed; the outputs are in $work" >&2
        exit 1
    fi
    rm -rf "$work"
    echo "postgres-lane-adversary: $cases cases passed"
    exit 0
}

# --- options ----------------------------------------------------------------------------------

# options_copy <name>: a root under the work directory with this tree's ci.yml and postgres-lane.sh.
options_copy() {
    mkdir -p "$work/$1/.github/workflows" "$work/$1/scripts"
    cp "$root/.github/workflows/ci.yml" "$work/$1/.github/workflows/ci.yml"
    cp "$root/scripts/postgres-lane.sh" "$work/$1/scripts/postgres-lane.sh"
}

# ci_appended <name> <text>: the copy's docker run line passes <text> (awk escapes: \n, \\) after
# the options it reads from the script; the check must refuse it.
ci_appended() {
    options_copy "$1"
    awk -v add="$2" 'index($0, "sh \"${options[@]}\"") { print $0 add; next } { print }' \
        "$root/.github/workflows/ci.yml" >"$work/$1/.github/workflows/ci.yml"
    expect_refused "$1" .github/workflows/ci.yml
}

# ci_edited <name> <sed expression>: the copy's ci.yml with that edit; the check must refuse it.
ci_edited() {
    options_copy "$1"
    sed "$2" "$root/.github/workflows/ci.yml" >"$work/$1/.github/workflows/ci.yml"
    expect_refused "$1" .github/workflows/ci.yml
}

# ci_awk_copy <name> <awk program> <text>: the copy's ci.yml through that program, the text in its
# variable add (taken as written: ENVIRON, not -v). ci_awk: the same, and the check must refuse it.
ci_awk_copy() {
    options_copy "$1"
    add=$3 awk "BEGIN { add = ENVIRON[\"add\"] } $2" "$root/.github/workflows/ci.yml" >"$work/$1/.github/workflows/ci.yml"
}
ci_awk() {
    ci_awk_copy "$@"
    expect_refused "$1" .github/workflows/ci.yml
}

# script_edited <name> <sed expression>: the copy's postgres-lane.sh with that edit; refused.
script_edited() {
    options_copy "$1"
    sed "$2" "$root/scripts/postgres-lane.sh" >"$work/$1/scripts/postgres-lane.sh"
    expect_refused "$1" scripts/postgres-lane.sh
}

expect_refused() {
    cases=$((cases + 1))
    if cmp -s "$root/$2" "$work/$1/$2"; then
        fail "options $1: the edit changed nothing in $2"
        return
    fi
    if sh "$check" "$work/$1" >"$work/$1.out" 2>&1; then
        fail "options $1: the check passed a copy whose $2 reads: $(diff "$root/$2" "$work/$1/$2" | sed -n 's/^> *//p' | tr '\n' ' ')"
    fi
}

options_cases() {
    check=${1:-$root/scripts/check-postgres-options.sh}
    case $check in /*) ;; *) check=$PWD/$check ;; esac
    work=$root/target/postgres-options-adversary
    rm -rf "$work"
    mkdir -p "$work"

    # The control: the unchanged copy passes, so a refusal below is the edit's.
    cases=$((cases + 1))
    options_copy unchanged
    sh "$check" "$work/unchanged" >"$work/unchanged.out" 2>&1 \
        || fail "options unchanged: the check refused the unchanged copy: $(cat "$work/unchanged.out")"

    # -c and its value in other spellings postgres takes the same way.
    ci_appended single-quoted " -c 'max_connections=100'"
    ci_appended quoted-pair ' "-c" "max_connections=100"'
    ci_appended attached ' -cmax_connections=100'
    ci_appended continued-line ' -c \\\n            max_connections=100'
    # The same option without -c: postgres --name=value and its -N switch for max_connections.
    ci_appended long-form ' --max_connections=100'
    ci_appended short-n ' -N 100'
    # The step reads the options and the container does not get them: the server runs on defaults.
    ci_edited options-not-passed 's/sh "\${options\[@\]}"/sh/'
    # The script names max_locks_per_transaction=256 and then a smaller value, which postgres takes.
    script_edited locks-overridden 's/^    echo "max_connections=300"$/&\
    echo "max_locks_per_transaction=64"/'

    # Round 2. A second job that starts its own PostgreSQL with a literal option and runs the lane.
    ci_awk second-job '{ print } END { print add }' '  postgres-13:
    name: PostgreSQL 13
    runs-on: ubuntu-24.04
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7
        with:
          persist-credentials: false
      - name: PostgreSQL 13 on loopback
        run: |
          set -euo pipefail
          docker run -d --name postgres -p 127.0.0.1:5432:5432 -e POSTGRES_USER=owner -e POSTGRES_PASSWORD=owner -e POSTGRES_DB=spindle postgres:13 -c max_connections=100
          echo "SPINDLE_TEST_DATABASE_URL=postgres://owner:owner@127.0.0.1:5432/spindle" >> "$GITHUB_ENV"
      - name: The PostgreSQL store
        run: task check:postgres'
    # A service container in the postgres job, image defaults (no TLS, 100 connections), and the
    # lane pointed at it; the checked docker run still starts, unused.
    ci_awk_copy service-container '
        /^  postgres:/ { job = 1 }
        { print }
        job && /^    timeout-minutes:/ { print add; job = 0 }
        ' '    services:
      db:
        image: postgres:17
        env:
          POSTGRES_USER: owner
          POSTGRES_PASSWORD: owner
          POSTGRES_DB: spindle
        ports:
          - 127.0.0.1:5433:5432'
    sed 's/127\.0\.0\.1:5432\/spindle/127.0.0.1:5433\/spindle/' "$work/service-container/.github/workflows/ci.yml" \
        >"$work/service-container.yml"
    mv "$work/service-container.yml" "$work/service-container/.github/workflows/ci.yml"
    expect_refused service-container .github/workflows/ci.yml

    finish
}

# --- lane -------------------------------------------------------------------------------------

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

ready_field() {
    sed -n "s/^postgres-lane: ready .* $1 \([^ ]*\).*/\1/p" "$2" | head -n 1
}

# wait_ready <out> <pid>: waits until the lane printed its ready line and pg_isready answers on
# its port; prints the port.
wait_ready() {
    i=0
    while [ "$i" -lt 300 ]; do
        p=$(ready_field port "$1")
        if [ -n "$p" ] && "$pg_isready" -q -h 127.0.0.1 -p "$p"; then
            echo "$p"
            return 0
        fi
        kill -0 "$2" 2>/dev/null || return 1
        sleep 0.1
        i=$((i + 1))
    done
    return 1
}

# gone <case> <dir> <pid>: the directory is gone and the PID is not alive (killed if it is), and
# no target/postgres-lane.* directory is left in the case's directory.
gone() {
    cases=$((cases + 1))
    if [ -n "$2" ] && [ -e "$2" ]; then
        fail "$1: the cluster directory $2 is still there"
    fi
    if [ -n "$3" ] && kill -0 "$3" 2>/dev/null; then
        fail "$1: the postmaster $3 is still alive"
        kill -KILL "$3" 2>/dev/null || true
    fi
    left=$(find "$work/$1/target" -maxdepth 1 -name 'postgres-lane.*' -type d 2>/dev/null || true)
    if [ -n "$left" ]; then
        fail "$1: a directory target/postgres-lane.* is left: $left"
    fi
}

# lane_copy <name> <sed expression>...: a copy of the lane script with those edits.
lane_copy() {
    name=$1
    shift
    cp "$lane" "$work/$name.sh"
    for edit in "$@"; do
        sed "$edit" "$work/$name.sh" >"$work/$name.sh.next"
        if cmp -s "$work/$name.sh" "$work/$name.sh.next"; then
            echo "postgres-lane-adversary: the edit '$edit' changed nothing in a copy of $lane" >&2
            exit 1
        fi
        mv "$work/$name.sh.next" "$work/$name.sh"
    done
}

# wrapper <dir> <name> <body>: an executable <dir>/<name> whose body runs before it execs the
# real binary; $real names that binary. The other binaries of the lane are plain links.
wrapper_dir() {
    mkdir -p "$1"
    for name in initdb pg_ctl postgres createdb pg_isready; do
        ln -sf "$(real_bin "$name")" "$1/$name"
    done
}
wrapper() {
    rm -f "$1/$2"
    {
        echo '#!/bin/sh'
        echo "real='$(real_bin "$2")'"
        printf '%s\n' "$3"
        echo 'exec "$real" "$@"'
    } >"$1/$2"
    chmod 755 "$1/$2"
}

lane_cases() {
    lane=${1:-$root/scripts/postgres-lane.sh}
    case $lane in /*) ;; *) lane=$PWD/$lane ;; esac
    work=$root/target/postgres-lane-adversary
    rm -rf "$work"
    mkdir -p "$work"
    for name in initdb pg_ctl postgres createdb pg_isready; do
        if ! real_bin "$name" >/dev/null; then
            echo "postgres-lane-adversary: no $name ${SPINDLE_PG_BIN:+in SPINDLE_PG_BIN ($SPINDLE_PG_BIN) }on this host; the lane cases need PostgreSQL 13 or later's binaries on PATH or in SPINDLE_PG_BIN" >&2
            exit 1
        fi
    done
    pg_isready=$(real_bin pg_isready)
    if ! env --default-signal=INT true 2>/dev/null || ! command -v setsid >/dev/null; then
        echo "postgres-lane-adversary: the signal cases need env --default-signal and setsid" >&2
        exit 1
    fi

    # 1. lane fails when run leaves its cluster directory: a copy whose cleanup removes nothing.
    lane_copy keeps-dir 's/^        rm -rf "\$dir"$/        :/'
    mkdir -p "$work/keeps-dir"
    status=0
    (cd "$work/keeps-dir" && sh "$work/keeps-dir.sh" lane -- true) >"$work/keeps-dir.out" 2>&1 || status=$?
    cases=$((cases + 1))
    [ "$status" -ne 0 ] || fail "keeps-dir: lane exited 0 though run left its cluster directory: $(cat "$work/keeps-dir.out")"
    grep -q 'is still there' "$work/keeps-dir.out" || fail "keeps-dir: lane did not name the directory left: $(cat "$work/keeps-dir.out")"
    rm -rf "$work/keeps-dir/target"

    # 2. lane fails when run leaves its postmaster alive: a copy whose cleanup stops nothing.
    lane_copy keeps-postmaster \
        's/^        "\$pg_ctl" stop -D .*$/        :/' \
        's/^    if \[ -n "\$postmaster" \] && kill -0 .*$/    if false; then/'
    mkdir -p "$work/keeps-postmaster"
    status=0
    (cd "$work/keeps-postmaster" && sh "$work/keeps-postmaster.sh" lane -- true) >"$work/keeps-postmaster.out" 2>&1 || status=$?
    pid=$(ready_field postmaster "$work/keeps-postmaster.out")
    cases=$((cases + 1))
    [ "$status" -ne 0 ] || fail "keeps-postmaster: lane exited 0 though run left its postmaster $pid alive: $(cat "$work/keeps-postmaster.out")"
    grep -q 'is still alive' "$work/keeps-postmaster.out" || fail "keeps-postmaster: lane did not name the postmaster left: $(cat "$work/keeps-postmaster.out")"
    if [ -n "$pid" ]; then
        kill -KILL "$pid" 2>/dev/null || true
    fi

    # 3. SIGTERM to run while its command runs.
    mkdir -p "$work/sigterm"
    start=$(date +%s)
    (cd "$work/sigterm" && exec sh "$lane" run -- sleep 30) >"$work/sigterm.out" 2>&1 &
    script=$!
    cases=$((cases + 1))
    wait_ready "$work/sigterm.out" "$script" >/dev/null || fail "sigterm: the cluster never answered: $(cat "$work/sigterm.out")"
    kill -TERM "$script" 2>/dev/null || true
    status=0
    wait "$script" || status=$?
    took=$(($(date +%s) - start))
    cases=$((cases + 1))
    [ "$status" -ne 0 ] && [ "$took" -lt 25 ] || fail "sigterm: run exited $status after $took s"
    gone sigterm "$(ready_field cluster "$work/sigterm.out")" "$(ready_field postmaster "$work/sigterm.out")"

    # 4. Ctrl-C through lane: SIGINT to the whole process group, as a terminal sends it.
    mkdir -p "$work/ctrl-c"
    start=$(date +%s)
    (cd "$work/ctrl-c" && exec setsid env --default-signal=INT sh "$lane" lane -- sleep 30) >"$work/ctrl-c.out" 2>&1 &
    script=$!
    cases=$((cases + 1))
    wait_ready "$work/ctrl-c.out" "$script" >/dev/null || fail "ctrl-c: the cluster never answered: $(cat "$work/ctrl-c.out")"
    kill -INT -- "-$script" 2>/dev/null || fail "ctrl-c: the lane is not the leader of its own process group"
    status=0
    wait "$script" || status=$?
    took=$(($(date +%s) - start))
    cases=$((cases + 1))
    [ "$status" -ne 0 ] && [ "$took" -lt 25 ] || fail "ctrl-c: lane exited $status after $took s: $(cat "$work/ctrl-c.out")"
    gone ctrl-c "$(ready_field cluster "$work/ctrl-c.out")" "$(ready_field postmaster "$work/ctrl-c.out")"
    left=$(find "$work/ctrl-c/target" -maxdepth 1 -name 'postgres-lane-record.*' 2>/dev/null || true)
    cases=$((cases + 1))
    [ -z "$left" ] || fail "ctrl-c: lane left its record $left"

    # 5. initdb fails after it made part of the data directory.
    wrapper_dir "$work/bin-initdb-fails"
    wrapper "$work/bin-initdb-fails" initdb 'if [ "${1:-}" != --version ]; then "$real" "$@" >/dev/null 2>&1; echo "initdb: made up failure" >&2; exit 1; fi'
    mkdir -p "$work/initdb-fails"
    status=0
    (cd "$work/initdb-fails" && SPINDLE_PG_BIN="$work/bin-initdb-fails" sh "$lane" run -- true) >"$work/initdb-fails.out" 2>&1 || status=$?
    cases=$((cases + 1))
    [ "$status" -ne 0 ] || fail "initdb-fails: run exited 0: $(cat "$work/initdb-fails.out")"
    gone initdb-fails "" ""

    # 6. pg_ctl start reports a failure after the server came up.
    wrapper_dir "$work/bin-start-fails"
    wrapper "$work/bin-start-fails" pg_ctl "if [ \"\${1:-}\" = start ]; then \"\$real\" \"\$@\"; d=; prev=; for a in \"\$@\"; do [ \"\$prev\" = -D ] && d=\$a; prev=\$a; done; sed -n 1p \"\$d/postmaster.pid\" >'$work/start-fails.pid'; exit 1; fi"
    mkdir -p "$work/start-fails"
    status=0
    (cd "$work/start-fails" && SPINDLE_PG_BIN="$work/bin-start-fails" sh "$lane" run -- true) >"$work/start-fails.out" 2>&1 || status=$?
    cases=$((cases + 1))
    [ "$status" -ne 0 ] || fail "start-fails: run exited 0: $(cat "$work/start-fails.out")"
    gone start-fails "" "$(cat "$work/start-fails.pid" 2>/dev/null || true)"

    # 7. Two lanes at once, the second's first port the one the first holds.
    mkdir -p "$work/first" "$work/second"
    (cd "$work/first" && exec sh "$lane" run -- sleep 30) >"$work/first.out" 2>&1 &
    first=$!
    cases=$((cases + 1))
    taken=$(wait_ready "$work/first.out" "$first") || fail "second: the first lane never answered: $(cat "$work/first.out")"
    wrapper_dir "$work/bin-port-taken"
    wrapper "$work/bin-port-taken" pg_ctl "if [ \"\${1:-}\" = start ] && [ ! -e '$work/port-taken.done' ]; then : >'$work/port-taken.done'; n=\$#; i=0; prev=; while [ \"\$i\" -lt \"\$n\" ]; do a=\$1; shift; [ \"\$prev\" = -o ] && a=\$(echo \"\$a\" | sed 's/-c port=[0-9]*/-c port=$taken/'); set -- \"\$@\" \"\$a\"; prev=\$a; i=\$((i + 1)); done; fi"
    status=0
    (cd "$work/second" && SPINDLE_PG_BIN="$work/bin-port-taken" sh "$lane" run -- true) >"$work/second.out" 2>&1 || status=$?
    cases=$((cases + 1))
    [ "$status" -eq 0 ] || fail "second: a lane whose first port another lane holds exited $status: $(cat "$work/second.out")"
    cases=$((cases + 1))
    got=$(ready_field port "$work/second.out")
    [ -n "$got" ] && [ "$got" != "$taken" ] || fail "second: the second lane is ready on port '$got', the first holds $taken"
    gone second "$(ready_field cluster "$work/second.out")" "$(ready_field postmaster "$work/second.out")"
    kill -TERM "$first" 2>/dev/null || true
    wait "$first" || true
    gone first "$(ready_field cluster "$work/first.out")" "$(ready_field postmaster "$work/first.out")"

    # 8. SPINDLE_TEST_DATABASE_URL set and empty: the command never runs with an empty URL.
    mkdir -p "$work/empty-url"
    status=0
    (cd "$work/empty-url" && SPINDLE_TEST_DATABASE_URL= sh "$lane" lane -- sh -c 'echo "url=$SPINDLE_TEST_DATABASE_URL"') >"$work/empty-url.out" 2>&1 || status=$?
    cases=$((cases + 1))
    [ "$status" -eq 0 ] || fail "empty-url: lane exited $status: $(cat "$work/empty-url.out")"
    cases=$((cases + 1))
    grep -q '^url=postgres://owner:owner@127\.0\.0\.1:[0-9][0-9]*/spindle$' "$work/empty-url.out" \
        || fail "empty-url: the command ran without a database URL: $(cat "$work/empty-url.out")"
    gone empty-url "$(ready_field cluster "$work/empty-url.out")" "$(ready_field postmaster "$work/empty-url.out")"

    finish
}

case ${1:-} in
    options)
        shift
        options_cases "$@"
        ;;
    lane)
        shift
        lane_cases "$@"
        ;;
    *)
        echo "usage: postgres-lane.adversary-test.sh options [<check script>] | lane [<lane script>]" >&2
        exit 2
        ;;
esac
