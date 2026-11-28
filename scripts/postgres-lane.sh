#!/bin/sh
# The PostgreSQL lane's database: the one place that holds the server options of the lane, read by
# `task check:postgres` and by CI's PostgreSQL job (story:postgres-lane-runs-locally).
#
#   postgres-lane.sh options <cert> <key>   prints the server options, one argument per line
#                                           (-c, then name=value), the TLS files named <cert>, <key>
#   postgres-lane.sh tls <dir>              writes a CA (ca.pem, ca.key) and a server certificate
#                                           for localhost and 127.0.0.1 it signed (server.pem,
#                                           server.key) into <dir>
#   postgres-lane.sh run -- <command>...    starts a throwaway cluster under target/, runs the
#                                           command with SPINDLE_TEST_DATABASE_URL and
#                                           SPINDLE_TEST_DATABASE_CA naming it, then stops and
#                                           removes the cluster, on exit, INT and TERM alike; exits
#                                           with the command's status
#   postgres-lane.sh lane -- <command>...   with SPINDLE_TEST_DATABASE_URL set, runs the command
#                                           alone; else `run`, then fails unless the cluster
#                                           directory is gone and its postmaster is not alive.
#                                           The cluster and the command run on 4 CPUs, as on
#                                           CI's runner (taskset, a block drawn at random from
#                                           this process's CPUs); SPINDLE_PG_LANE_CPUS=<n>
#                                           names another count, 0 every CPU
#
# The cluster: initdb and pg_ctl of PostgreSQL 13 or later from SPINDLE_PG_BIN, else PATH; a major
# other than CI's 17 is printed with a warning. The superuser owner (password owner), password
# login on 127.0.0.1, the database spindle, a free port, the socket inside the cluster directory.
set -eu

ci_major=17
floor_major=13
# The CPUs of CI's runner, which runs the lane and its PostgreSQL on them together.
ci_cpus=4

# The server options of the lane. CI's PostgreSQL job and `run` both take them from here.
server_options() {
    echo "ssl=on"
    echo "ssl_cert_file=$1"
    echo "ssl_key_file=$2"
    echo "max_locks_per_transaction=256"
    echo "max_connections=300"
}

options() {
    if [ "$#" -ne 2 ]; then
        echo "usage: postgres-lane.sh options <cert> <key>" >&2
        exit 2
    fi
    server_options "$1" "$2" | while IFS= read -r option; do
        echo "-c"
        echo "$option"
    done
}

tls() {
    if [ "$#" -ne 1 ]; then
        echo "usage: postgres-lane.sh tls <dir>" >&2
        exit 2
    fi
    mkdir -p "$1"
    (
        umask 077
        openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=spindle-test-ca \
            -keyout "$1/ca.key" -out "$1/ca.pem" \
            -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign 2>/dev/null
        openssl req -newkey rsa:2048 -nodes -subj /CN=localhost -keyout "$1/server.key" -out "$1/server.csr" 2>/dev/null
        printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' >"$1/server.ext"
        openssl x509 -req -in "$1/server.csr" -CA "$1/ca.pem" -CAkey "$1/ca.key" -CAcreateserial \
            -days 1 -extfile "$1/server.ext" -out "$1/server.pem" 2>/dev/null
    )
    chmod 600 "$1/server.key"
    chmod 644 "$1/ca.pem" "$1/server.pem"
}

say() {
    echo "postgres-lane: $*"
}

# The binary $1 of the cluster: SPINDLE_PG_BIN's, else PATH's.
pg_binary() {
    if [ -n "${SPINDLE_PG_BIN:-}" ]; then
        if [ -x "$SPINDLE_PG_BIN/$1" ]; then
            echo "$SPINDLE_PG_BIN/$1"
            return 0
        fi
        echo "postgres-lane: no $1 in SPINDLE_PG_BIN ($SPINDLE_PG_BIN)" >&2
        return 1
    fi
    if command -v "$1" 2>/dev/null; then
        return 0
    fi
    echo "postgres-lane: no $1 on PATH; install PostgreSQL $floor_major or later, or set SPINDLE_PG_BIN to its bin directory" >&2
    return 1
}

# locale: CI's en_US.UTF-8 when locale -a lists it (in any case or spelling of utf8), else C.UTF-8
# (else C), with one warning naming the difference.
pick_locale() {
    available=$(locale -a 2>/dev/null || true)
    locale=$(printf '%s\n' "$available" | grep -i -x 'en_US\.utf-\{0,1\}8' | head -n 1 || true)
    if [ -n "$locale" ]; then
        say "locale $locale, as CI's en_US.utf8"
        return
    fi
    locale=$(printf '%s\n' "$available" | grep -i -x 'C\.utf-\{0,1\}8' | head -n 1 || true)
    if [ -z "$locale" ]; then
        locale=C
    fi
    say "warning: locale $locale, not CI's en_US.utf8, which locale -a does not list: text sorts and compares by code point here and by en_US rules on CI"
}

# A port in 20000-59999, drawn at random.
draw_port() {
    n=$(od -An -N2 -tu2 /dev/urandom | tr -d ' ')
    echo $((20000 + n % 40000))
}

dir=
data=
child=
postmaster=

# Stops the command, then the cluster, and removes the cluster directory. Runs once, on exit.
cleanup() {
    trap - EXIT INT TERM HUP
    if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
        kill -TERM "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
    fi
    if [ -n "$data" ] && [ -f "$data/postmaster.pid" ]; then
        postmaster=$(sed -n 1p "$data/postmaster.pid")
        "$pg_ctl" stop -D "$data" -m fast -w -t 30 -s >/dev/null 2>&1 || true
    fi
    if [ -n "$postmaster" ] && kill -0 "$postmaster" 2>/dev/null; then
        kill -KILL "$postmaster" 2>/dev/null || true
        i=0
        while kill -0 "$postmaster" 2>/dev/null && [ "$i" -lt 100 ]; do
            sleep 0.1
            i=$((i + 1))
        done
    fi
    if [ -n "$dir" ]; then
        rm -rf "$dir"
    fi
}

run() {
    if [ "${1:-}" = -- ]; then
        shift
    fi
    if [ "$#" -eq 0 ]; then
        echo "usage: postgres-lane.sh run -- <command> [<argument>...]" >&2
        exit 2
    fi
    initdb=$(pg_binary initdb)
    pg_ctl=$(pg_binary pg_ctl)
    createdb=$(pg_binary createdb)

    version=$("$initdb" --version)
    major=$(echo "$version" | sed -n 's/^[^)]*)[[:space:]]*\([0-9][0-9]*\).*/\1/p')
    case $major in
        '' | *[!0-9]*)
            echo "postgres-lane: cannot read the major version of $initdb from '$version'" >&2
            exit 1
            ;;
    esac
    if [ "$major" -lt "$floor_major" ]; then
        echo "postgres-lane: PostgreSQL $major ($initdb) is older than $floor_major, the oldest Spindle supports" >&2
        exit 1
    fi
    say "PostgreSQL $major ($version) from $(dirname "$initdb")"
    if [ "$major" -ne "$ci_major" ]; then
        say "warning: PostgreSQL $major is not CI's $ci_major; set SPINDLE_PG_BIN to a PostgreSQL $ci_major bin directory to run as CI does"
    fi

    trap cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP

    mkdir -p target
    dir=$(mktemp -d "$PWD/target/postgres-lane.XXXXXX")
    case $dir in
        *"'"* | *'"'* | *' '*)
            echo "postgres-lane: the cluster directory $dir holds a quote or a space" >&2
            exit 1
            ;;
    esac
    if [ -n "${SPINDLE_PG_LANE_RECORD:-}" ]; then
        echo "$dir" >"$SPINDLE_PG_LANE_RECORD"
    fi
    data=$dir/data
    tls "$dir/tls"
    printf 'owner\n' >"$dir/pwfile"
    pick_locale
    "$initdb" -D "$data" -U owner --pwfile="$dir/pwfile" --auth-host=scram-sha-256 --auth-local=trust \
        --encoding=UTF8 --locale="$locale" --no-sync >"$dir/initdb.log" 2>&1 || {
        cat "$dir/initdb.log" >&2
        echo "postgres-lane: initdb failed" >&2
        exit 1
    }

    # The socket directory is relative to the data directory: an absolute path inside a worktree
    # can exceed the 107 bytes of a socket path.
    # The server logs to server.log, where a port another process holds is read from: a
    # postgresql.conf.sample that turns the logging collector on would send the log elsewhere.
    flags="-c listen_addresses=127.0.0.1 -c unix_socket_directories=. -c logging_collector=off"
    for option in $(server_options "$dir/tls/server.pem" "$dir/tls/server.key"); do
        flags="$flags -c '$option'"
    done
    attempt=1
    while :; do
        port=$(draw_port)
        : >"$dir/server.log"
        if "$pg_ctl" start -D "$data" -l "$dir/server.log" -w -t 60 -s -o "$flags -c port=$port" >/dev/null 2>&1; then
            break
        fi
        if [ "$attempt" -lt 5 ] && grep -q 'could not bind\|Address already in use' "$dir/server.log" 2>/dev/null; then
            attempt=$((attempt + 1))
            continue
        fi
        echo "postgres-lane: pg_ctl start failed on port $port (attempt $attempt); the end of the server log:" >&2
        tail -n 20 "$dir/server.log" >&2 2>/dev/null || true
        exit 1
    done
    postmaster=$(sed -n 1p "$data/postmaster.pid")
    if [ -n "${SPINDLE_PG_LANE_RECORD:-}" ]; then
        echo "$postmaster" >>"$SPINDLE_PG_LANE_RECORD"
    fi
    PGPASSWORD=owner "$createdb" -h 127.0.0.1 -p "$port" -U owner spindle || {
        echo "postgres-lane: createdb spindle failed" >&2
        exit 1
    }

    SPINDLE_TEST_DATABASE_URL=postgres://owner:owner@127.0.0.1:$port/spindle
    SPINDLE_TEST_DATABASE_CA=$dir/tls/ca.pem
    export SPINDLE_TEST_DATABASE_URL SPINDLE_TEST_DATABASE_CA
    say "ready on 127.0.0.1 port $port cluster $dir postmaster $postmaster"

    # The command runs in the background so that INT and TERM reach the trap at once; its standard
    # input stays this script's.
    exec 3<&0
    "$@" <&3 3<&- &
    child=$!
    status=0
    wait "$child" || status=$?
    child=
    exit "$status"
}

# The CPUs the lane's cluster and command run on, as a taskset list: a block of $1 drawn at random
# from those this process may use, or nothing when it may use $1 or fewer. The CPUs are taken core
# by core, each with the hyperthreads it shares a core with: CI's 4 CPUs are 2 cores of 2 threads,
# and 4 threads of 4 cores ran CPU-bound cases 2.5 times faster than CI did.
cpu_block() {
    allowed=$(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' /proc/self/status 2>/dev/null || true)
    if [ -z "$allowed" ]; then
        return 0
    fi
    echo "$allowed" | awk -v want="$1" -v draw="$(draw_port)" '
    # expand(list, out): the CPUs of a list such as 0-3,8 into out[0..], their count returned.
    function expand(list, out,    parts, part, range, i, c, n) {
        n = 0
        parts = split(list, part, ",")
        for (i = 1; i <= parts; i++) {
            if (split(part[i], range, "-") == 2) {
                for (c = range[1] + 0; c <= range[2] + 0; c++) out[n++] = c
            } else if (part[i] != "") {
                out[n++] = part[i] + 0
            }
        }
        return n
    }
    {
        count = expand($0, cpu)
        if (count <= want) exit
        for (i = 0; i < count; i++) ok[cpu[i]] = 1
        n = 0
        for (i = 0; i < count; i++) {
            if (cpu[i] in taken) continue
            siblings = cpu[i]
            file = "/sys/devices/system/cpu/cpu" cpu[i] "/topology/thread_siblings_list"
            if ((getline line < file) > 0) siblings = line
            close(file)
            m = expand(siblings, sibling)
            for (j = 0; j < m; j++) {
                if ((sibling[j] in ok) && !(sibling[j] in taken)) {
                    taken[sibling[j]] = 1
                    order[n++] = sibling[j]
                }
            }
        }
        start = want * (draw % int(n / want))
        list = order[start]
        for (i = 1; i < want; i++) list = list "," order[start + i]
        print list
    }'
}

lane() {
    if [ "${1:-}" = -- ]; then
        shift
    fi
    if [ "$#" -eq 0 ]; then
        echo "usage: postgres-lane.sh lane -- <command> [<argument>...]" >&2
        exit 2
    fi
    if [ -n "${SPINDLE_TEST_DATABASE_URL:-}" ]; then
        say "SPINDLE_TEST_DATABASE_URL is set: no cluster of its own"
        exec "$@"
    fi
    cpus=${SPINDLE_PG_LANE_CPUS:-$ci_cpus}
    case $cpus in
        '' | *[!0-9]*)
            echo "postgres-lane: SPINDLE_PG_LANE_CPUS '$cpus' is not a whole number of CPUs" >&2
            exit 2
            ;;
    esac
    pin=
    if [ "$cpus" -gt 0 ]; then
        if command -v taskset >/dev/null 2>&1; then
            pin=$(cpu_block "$cpus")
        else
            say "warning: no taskset here: the cluster and the command run on every CPU, not on $cpus as on CI's runner"
        fi
    fi
    if [ -n "$pin" ]; then
        say "the cluster and the command run on CPUs $pin, $cpus as on CI's runner (SPINDLE_PG_LANE_CPUS)"
    fi
    mkdir -p target
    record=$(mktemp "$PWD/target/postgres-lane-record.XXXXXX")
    # A handler, not an ignore: `run` must still get INT and TERM. The check below runs after it.
    trap ':' INT TERM HUP
    status=0
    if [ -n "$pin" ]; then
        SPINDLE_PG_LANE_RECORD=$record taskset -c "$pin" sh "$0" run -- "$@" || status=$?
    else
        SPINDLE_PG_LANE_RECORD=$record sh "$0" run -- "$@" || status=$?
    fi
    gone_dir=$(sed -n 1p "$record")
    gone_pid=$(sed -n 2p "$record")
    rm -f "$record"
    if [ -n "$gone_dir" ] && [ -e "$gone_dir" ]; then
        echo "postgres-lane: the cluster directory $gone_dir is still there" >&2
        status=1
    fi
    if [ -n "$gone_pid" ] && kill -0 "$gone_pid" 2>/dev/null; then
        echo "postgres-lane: the postmaster $gone_pid is still alive" >&2
        status=1
    fi
    if [ "$status" -eq 0 ]; then
        say "the cluster ${gone_dir:-(none)} is removed and its postmaster ${gone_pid:-(none)} is not alive"
    fi
    exit "$status"
}

case ${1:-} in
    options)
        shift
        options "$@"
        ;;
    tls)
        shift
        tls "$@"
        ;;
    run)
        shift
        run "$@"
        ;;
    lane)
        shift
        lane "$@"
        ;;
    *)
        echo "usage: postgres-lane.sh options <cert> <key> | tls <dir> | run -- <command>... | lane -- <command>..." >&2
        exit 2
        ;;
esac
