#!/bin/sh
# The PostgreSQL lane's server options have one home, scripts/postgres-lane.sh
# (story:postgres-lane-runs-locally):
#
#   check-postgres-options.sh [<root>]   fails when the PostgreSQL step of <root>/.github/workflows/ci.yml
#                                        names a -c server option itself or does not read them from
#                                        `postgres-lane.sh options`, or when the script's options lack
#                                        ssl=on or max_locks_per_transaction=256, or hold a
#                                        max_connections below 300
#   check-postgres-options.sh test       runs this check on copies of this tree with one of those
#                                        faults each, and expects each refused and the tree passed
set -eu

min_connections=300

check() {
    root=$1
    ci=$root/.github/workflows/ci.yml
    lane=$root/scripts/postgres-lane.sh
    status=0

    # The CI step that starts PostgreSQL: from its name to the next step.
    step=$(awk '/^      - name: PostgreSQL/{on=1; print; next} on && /^      - /{on=0} on' "$ci")
    if [ -z "$step" ]; then
        echo "check-postgres-options: $ci: no step named PostgreSQL… in the postgres job" >&2
        return 1
    fi
    literal=$(printf '%s\n' "$step" | grep -E -e '-c[[:space:]]+[A-Za-z_]+=' || true)
    if [ -n "$literal" ]; then
        echo "check-postgres-options: $ci: the PostgreSQL step names a -c server option itself; take it from scripts/postgres-lane.sh options: $(printf '%s\n' "$literal" | sed 's/^[[:space:]]*//' | head -n 1)" >&2
        status=1
    fi
    if ! printf '%s\n' "$step" | grep -q 'scripts/postgres-lane.sh options'; then
        echo "check-postgres-options: $ci: the PostgreSQL step does not read its server options from scripts/postgres-lane.sh options" >&2
        status=1
    fi

    options=$(sh "$lane" options /cert.pem /key.pem | sed -n '/^-c$/{n;p;}') || {
        echo "check-postgres-options: $lane options failed" >&2
        return 1
    }
    for want in ssl=on max_locks_per_transaction=256; do
        if ! printf '%s\n' "$options" | grep -qx "$want"; then
            echo "check-postgres-options: $lane: the options lack $want" >&2
            status=1
        fi
    done
    connections=$(printf '%s\n' "$options" | sed -n 's/^max_connections=//p' | tail -n 1)
    case $connections in
        '' | *[!0-9]*)
            echo "check-postgres-options: $lane: the options name no max_connections" >&2
            status=1
            ;;
        *)
            if [ "$connections" -lt "$min_connections" ]; then
                echo "check-postgres-options: $lane: max_connections=$connections is below $min_connections" >&2
                status=1
            fi
            ;;
    esac
    if [ "$status" -eq 0 ]; then
        echo "check-postgres-options: $ci reads $(printf '%s\n' "$options" | grep -v '^ssl_.*_file=' | tr '\n' ' ')from $lane"
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
    refused script-200-connections scripts/postgres-lane.sh 's/max_connections=300/max_connections=200/'
    refused script-no-ssl scripts/postgres-lane.sh '/echo "ssl=on"/d'
    refused script-no-locks scripts/postgres-lane.sh '/echo "max_locks_per_transaction=256"/d'
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
