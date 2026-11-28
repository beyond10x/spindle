#!/bin/sh
# The PostgreSQL lane's server options have one home, scripts/postgres-lane.sh
# (story:postgres-lane-runs-locally):
#
#   check-postgres-options.sh [<root>]   fails when the postgres job of <root>/.github/workflows/ci.yml
#                                        can set a server option other than through
#                                        `postgres-lane.sh options`, or when the script's options lack
#                                        ssl=on or the TLS files, or end with a
#                                        max_locks_per_transaction below 256 or a max_connections
#                                        below 300
#   check-postgres-options.sh test       runs this check on copies of this tree with one of those
#                                        faults each, and expects each refused and the tree passed
#
# The postgres job is read as shell commands, a line continued with \ joined to the next. In it:
#   - the one `docker run` is exactly $want_run below: the image's arguments are the script's
#     options and nothing else, and no flag of the container sets a configuration;
#   - every other docker command is one of $want_docker;
#   - `options` is assigned only from `postgres-lane.sh options`, as $want_options lists;
#   - nothing names psql, ALTER SYSTEM, PGOPTIONS, POSTGRES_INITDB_ARGS, a postgresql.conf or a
#     docker:// action;
#   - SPINDLE_TEST_DATABASE_URL and _CA are written only as $want_env lists, and the job declares
#     no services: or container:, so the tests reach the container the checked docker run starts.
# Outside that job, no job of any workflow under .github/workflows runs check:postgres, names
# SPINDLE_TEST_DATABASE_*, or names a postgres image (postgres:<tag>, postgres@<digest>).
# A change to the job's database start changes this check with it.
set -eu

min_connections=300
min_locks=256

want_run=$(
    cat <<'EOF'
docker run -d --name postgres -p 127.0.0.1:5432:5432 -e POSTGRES_USER=owner -e POSTGRES_PASSWORD=owner -e POSTGRES_DB=spindle -v "$dir:/certs:ro" --entrypoint sh postgres:17 -c 'install -o postgres -g postgres -m 600 /certs/server.key /tmp/server.key && install -o postgres -g postgres -m 644 /certs/server.pem /tmp/server.pem && exec docker-entrypoint.sh postgres "$@"' sh "${options[@]}"
EOF
)
want_docker=$(
    cat <<'EOF'
if docker exec postgres pg_isready -h 127.0.0.1 -U owner -d spindle >/dev/null 2>&1; then
docker exec postgres pg_isready -h 127.0.0.1 -U owner -d spindle
EOF
)
want_options=$(
    cat <<'EOF'
options=$(sh scripts/postgres-lane.sh options /tmp/server.pem /tmp/server.key)
mapfile -t options <<< "$options"
EOF
)

want_env=$(
    cat <<'EOF'
echo "SPINDLE_TEST_DATABASE_URL=postgres://owner:owner@127.0.0.1:5432/spindle" >> "$GITHUB_ENV"
echo "SPINDLE_TEST_DATABASE_CA=$dir/ca.pem" >> "$GITHUB_ENV"
EOF
)

# The postgres job of a workflow, as written.
job_raw() {
    awk '/^  postgres:/{on=1; next} on && /^  [^ #]/{on=0} on' "$1"
}

# Every line of every workflow under <root> but the postgres job of ci.yml, each as file:line: text.
outside_lines() {
    for file in "$1"/.github/workflows/*.yml "$1"/.github/workflows/*.yaml; do
        [ -f "$file" ] || continue
        if [ "$file" = "$1/.github/workflows/ci.yml" ]; then
            awk '/^  postgres:/{skip=1; next} skip && /^  [^ #]/{skip=0} !skip { print FILENAME ":" FNR ": " $0 }' "$file"
        else
            awk '{ print FILENAME ":" FNR ": " $0 }' "$file"
        fi
    done
}

# The postgres job of a workflow as logical lines: indentation dropped, continuations joined,
# runs of blanks made one.
job_lines() {
    job_raw "$1" |
        awk '{ sub(/^[[:space:]]+/, ""); if (sub(/\\$/, "")) { buf = buf $0 " "; next } print buf $0; buf = "" } END { if (buf != "") print buf }' |
        sed 's/[[:space:]][[:space:]]*/ /g; s/ $//'
}

check() {
    root=$1
    ci=$root/.github/workflows/ci.yml
    lane=$root/scripts/postgres-lane.sh
    status=0

    lines=$(job_lines "$ci")
    if [ -z "$lines" ]; then
        echo "check-postgres-options: $ci: no postgres job" >&2
        return 1
    fi

    runs=$(printf '%s\n' "$lines" | grep 'docker run' || true)
    if [ "$runs" != "$want_run" ]; then
        echo "check-postgres-options: $ci: the postgres job's docker run is not the one that passes the image exactly the options of scripts/postgres-lane.sh; it reads: ${runs:-(none)}" >&2
        status=1
    fi
    others=$(printf '%s\n' "$lines" | grep 'docker' | grep -v 'docker run' || true)
    printf '%s\n' "$others" | while IFS= read -r line; do
        [ -n "$line" ] || continue
        if ! printf '%s\n' "$want_docker" | grep -qxF -e "$line"; then
            echo "check-postgres-options: $ci: the postgres job runs a docker command other than its pg_isready: $line" >&2
            echo 1
        fi
    done | grep -q 1 && status=1
    named=$(printf '%s\n' "$lines" | grep -w 'options' | grep -v 'docker run' || true)
    if [ "$named" != "$want_options" ]; then
        echo "check-postgres-options: $ci: the postgres job sets options other than from scripts/postgres-lane.sh options; it reads: $(printf '%s\n' "$named" | tr '\n' ';')" >&2
        status=1
    fi
    banned=$(printf '%s\n' "$lines" | grep -i -E 'psql|alter system|PGOPTIONS|POSTGRES_INITDB_ARGS|postgresql(\.auto)?\.conf|docker://' || true)
    if [ -n "$banned" ]; then
        echo "check-postgres-options: $ci: the postgres job can set a server option outside the script: $(printf '%s\n' "$banned" | head -n 1)" >&2
        status=1
    fi
    # The database the tests reach is the one the checked docker run starts: the job writes the
    # URL and the CA only as $want_env lists, and has no service or job container of its own.
    database=$(printf '%s\n' "$lines" | grep 'SPINDLE_TEST_DATABASE_' || true)
    if [ "$database" != "$want_env" ]; then
        echo "check-postgres-options: $ci: the postgres job names SPINDLE_TEST_DATABASE_URL or _CA other than by writing the checked container's: $(printf '%s\n' "$database" | tr '\n' ';')" >&2
        status=1
    fi
    containers=$(job_raw "$ci" | grep -E '^    (services|container):' || true)
    if [ -n "$containers" ]; then
        echo "check-postgres-options: $ci: the postgres job declares $(printf '%s\n' "$containers" | sed 's/^ *//; s/:.*//' | head -n 1): its database is not the checked docker run's" >&2
        status=1
    fi
    # No other job of any workflow runs the lane, names its database or starts a PostgreSQL image.
    elsewhere=$(outside_lines "$root" | grep -E 'check:postgres|SPINDLE_TEST_DATABASE_|postgres(:[A-Za-z0-9]|@)' || true)
    if [ -n "$elsewhere" ]; then
        echo "check-postgres-options: a job other than ci.yml's postgres runs the lane or a PostgreSQL of its own: $(printf '%s\n' "$elsewhere" | head -n 1)" >&2
        status=1
    fi

    out=$(sh "$lane" options /cert.pem /key.pem) || {
        echo "check-postgres-options: $lane options failed" >&2
        return 1
    }
    # One -c before each name=value, nothing else: what the image receives is what is read here.
    shape=$(printf '%s\n' "$out" | awk 'NR % 2 == 1 && $0 != "-c" { print NR": "$0 } NR % 2 == 0 && $0 !~ /^[a-z_][a-z0-9_]*=/ { print NR": "$0 } END { if (NR % 2 == 1) print "an odd count of lines" }')
    if [ -n "$shape" ]; then
        echo "check-postgres-options: $lane: options prints something other than -c and name=value pairs: $(printf '%s\n' "$shape" | head -n 1)" >&2
        return 1
    fi
    options=$(printf '%s\n' "$out" | sed -n '/^-c$/{n;p;}')
    # The value postgres takes: the last one given.
    last() {
        printf '%s\n' "$options" | sed -n "s/^$1=//p" | tail -n 1
    }
    for pair in ssl=on ssl_cert_file=/cert.pem ssl_key_file=/key.pem; do
        if [ "$(last "${pair%%=*}")" != "${pair#*=}" ]; then
            echo "check-postgres-options: $lane: the options do not end with $pair (the last ${pair%%=*} is '$(last "${pair%%=*}")')" >&2
            status=1
        fi
    done
    for bound in max_locks_per_transaction=$min_locks max_connections=$min_connections; do
        name=${bound%%=*}
        value=$(last "$name")
        case $value in
            '' | *[!0-9]*)
                echo "check-postgres-options: $lane: the options name no whole $name ('$value')" >&2
                status=1
                ;;
            *)
                if [ "$value" -lt "${bound#*=}" ]; then
                    echo "check-postgres-options: $lane: the last $name is $value, below ${bound#*=}" >&2
                    status=1
                fi
                ;;
        esac
    done
    if [ "$status" -eq 0 ]; then
        echo "check-postgres-options: $ci passes its container exactly the options of $lane: ssl=on max_locks_per_transaction=$(last max_locks_per_transaction) max_connections=$(last max_connections)"
    fi
    return "$status"
}

# copy <name>: a root under the work directory with this tree's ci.yml and postgres-lane.sh.
copy() {
    mkdir -p "$work/$1/.github/workflows" "$work/$1/scripts"
    cp "$here/.github/workflows/ci.yml" "$work/$1/.github/workflows/ci.yml"
    cp "$here/scripts/postgres-lane.sh" "$work/$1/scripts/postgres-lane.sh"
}

# refused <name> <file> <sed expression>: the check refuses the copy with that edit of <file>.
refused() {
    copy "$1"
    sed "$3" "$here/$2" >"$work/$1/$2"
    cases=$((cases + 1))
    if cmp -s "$here/$2" "$work/$1/$2"; then
        echo "check-postgres-options: FAIL: case $1: the edit '$3' changed nothing in $2" >&2
        failures=$((failures + 1))
        return
    fi
    if check "$work/$1" >"$work/$1.out" 2>&1; then
        echo "check-postgres-options: FAIL: case $1: the check passed a copy with '$3' in $2" >&2
        failures=$((failures + 1))
    fi
}

if [ "${1:-}" = test ]; then
    here=$(cd "$(dirname "$0")/.." && pwd)
    work=$here/target/check-postgres-options
    rm -rf "$work"
    mkdir -p "$work"
    failures=0
    cases=0
    refused ci-literal-connections .github/workflows/ci.yml 's/exec docker-entrypoint.sh postgres/& -c max_connections=100/'
    refused ci-literal-ssl .github/workflows/ci.yml 's/exec docker-entrypoint.sh postgres/& -c ssl=on/'
    refused ci-reads-no-options .github/workflows/ci.yml 's/scripts\/postgres-lane.sh options/scripts\/postgres-lane.sh tls/'
    refused ci-env-flag .github/workflows/ci.yml 's/-e POSTGRES_DB=spindle/& -e POSTGRES_INITDB_ARGS=-cmax_connections=100/'
    refused ci-options-appended .github/workflows/ci.yml 's/^\( *\)mapfile -t options <<< "\$options"$/&\n\1options+=(-c max_connections=100)/'
    refused ci-docker-exec .github/workflows/ci.yml 's/^\( *\)docker exec postgres pg_isready -h 127.0.0.1 -U owner -d spindle$/&\n\1docker exec postgres sh -c "echo max_connections=100 >> \/var\/lib\/postgresql\/data\/extra"/'
    refused ci-psql .github/workflows/ci.yml 's/^\( *\)docker exec postgres pg_isready -h 127.0.0.1 -U owner -d spindle$/&\n\1PGPASSWORD=owner psql -h 127.0.0.1 -U owner -d spindle -c "ALTER SYSTEM SET max_connections = 100"/'
    refused script-200-connections scripts/postgres-lane.sh 's/max_connections=300/max_connections=200/'
    refused script-no-ssl scripts/postgres-lane.sh '/echo "ssl=on"/d'
    refused script-no-locks scripts/postgres-lane.sh '/echo "max_locks_per_transaction=256"/d'
    refused script-ssl-off-later scripts/postgres-lane.sh 's/^    echo "max_connections=300"$/&\n    echo "ssl=off"/'
    refused script-bare-argument scripts/postgres-lane.sh 's/^        echo "-c"$/        echo "-N"/'
    refused ci-job-container .github/workflows/ci.yml 's/^    name: PostgreSQL$/&\n    container: ubuntu:24.04/'
    refused ci-step-env-url .github/workflows/ci.yml 's/^        run: task check:postgres$/        env:\n          SPINDLE_TEST_DATABASE_URL: postgres:\/\/owner:owner@127.0.0.1:5433\/spindle\n&/'
    refused ci-gate-runs-lane .github/workflows/ci.yml 's/^        run: task check$/        run: task check \&\& task check:postgres/'
    refused ci-uses-docker .github/workflows/ci.yml 's/^      - name: The PostgreSQL store$/      - uses: docker:\/\/alpine:3\n&/'
    # Another workflow file with a job of its own that runs the lane.
    cases=$((cases + 1))
    copy other-workflow
    {
        echo 'name: Other'
        echo 'on: push'
        echo 'jobs:'
        echo '  lane:'
        echo '    runs-on: ubuntu-24.04'
        echo '    steps:'
        echo '      - run: task check:postgres'
    } >"$work/other-workflow/.github/workflows/other.yml"
    if check "$work/other-workflow" >"$work/other-workflow.out" 2>&1; then
        echo "check-postgres-options: FAIL: case other-workflow: the check passed a second workflow that runs task check:postgres" >&2
        failures=$((failures + 1))
    fi
    cases=$((cases + 1))
    copy unchanged
    if ! check "$work/unchanged" >"$work/unchanged.out" 2>&1; then
        echo "check-postgres-options: FAIL: the unchanged copy is refused: $(cat "$work/unchanged.out")" >&2
        failures=$((failures + 1))
    fi
    if [ "$failures" -ne 0 ]; then
        echo "check-postgres-options: $failures of $cases case(s) failed" >&2
        exit 1
    fi
    rm -rf "$work"
    echo "check-postgres-options: $cases cases passed"
    exit 0
fi

check "${1:-.}"
