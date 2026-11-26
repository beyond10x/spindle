#!/bin/sh
# Classifies the paths a change touches, so a CI job can skip what the change cannot affect.
#
#   changed-paths.sh classify   reads one path per line on stdin and prints planning, docs or code
#   changed-paths.sh github     classifies the pull request GitHub Actions checked out and writes
#                               class=<class> to $GITHUB_OUTPUT; any other event is code
#   changed-paths.sh test       runs this script's test cases
#
# This file is the only place that holds the path list:
#   planning  .engineering/, and *.md outside website/ that no test reads
#   docs      website/
#   code      anything else; an empty list is code too
# A change is code when one path is code, docs when one path is docs and none is code, and
# planning only when every path is planning.
set -eu

# Markdown files outside website/ that a test under crates/ reads; a change to one runs the tests.
read_by_tests='docs/design/spindle.md docs/design/observe.md'

class_of() {
    case $1 in
        .engineering/*) echo planning; return ;;
        website/*) echo docs; return ;;
    esac
    for read in $read_by_tests; do
        if [ "$1" = "$read" ]; then
            echo code
            return
        fi
    done
    case $1 in
        *.md) echo planning ;;
        *) echo code ;;
    esac
}

classify() {
    result=
    while IFS= read -r path || [ -n "$path" ]; do
        [ -n "$path" ] || continue
        case $(class_of "$path") in
            code)
                echo code
                return
                ;;
            docs) result=docs ;;
            planning) [ -n "$result" ] || result=planning ;;
        esac
    done
    echo "${result:-code}"
}

github() {
    class=code
    reason="event ${GITHUB_EVENT_NAME:-unset}: every job runs"
    if [ "${GITHUB_EVENT_NAME:-}" = pull_request ]; then
        if git rev-parse --verify --quiet 'HEAD^2' >/dev/null && git rev-parse --verify --quiet 'HEAD^1' >/dev/null; then
            changed=$(git diff --name-only --no-renames 'HEAD^1' HEAD)
            class=$(printf '%s\n' "$changed" | classify)
            count=$(printf '%s\n' "$changed" | grep -c . || true)
            reason="$count changed path(s) against the base of the merge commit"
        else
            reason="HEAD is not the merge commit of a pull request: every job runs"
        fi
    fi
    echo "changed-paths: $class ($reason)"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
        echo "class=$class" >>"$GITHUB_OUTPUT"
    fi
}

failures=0
cases=0

expect() {
    want=$1
    shift
    got=$(printf '%s\n' "$@" | classify)
    cases=$((cases + 1))
    if [ "$got" != "$want" ]; then
        echo "changed-paths: FAIL: [$*] is $got, want $want" >&2
        failures=$((failures + 1))
    fi
}

expect_github() {
    want=$1
    event=$2
    shift 2
    repo=$work/repo
    rm -rf "$repo"
    mkdir -p "$repo"
    (
        cd "$repo"
        git init -q -b main
        git -c user.name=test -c user.email=test@invalid commit -q --allow-empty -m base
        git checkout -q -b change
        for path in "$@"; do
            mkdir -p "$(dirname "$path")"
            echo change >"$path"
        done
        git add -A
        git -c user.name=test -c user.email=test@invalid commit -q -m change
        git checkout -q main
        git -c user.name=test -c user.email=test@invalid merge -q --no-ff -m merge change
    )
    : >"$work/output"
    (cd "$repo" && GITHUB_EVENT_NAME=$event GITHUB_OUTPUT=$work/output sh "$script" github) >/dev/null
    got=$(sed -n 's/^class=//p' "$work/output")
    cases=$((cases + 1))
    if [ "$got" != "$want" ]; then
        echo "changed-paths: FAIL: github on $event with [$*] is '$got', want $want" >&2
        failures=$((failures + 1))
    fi
}

self_test() {
    expect planning .engineering/planning/stories/a.md
    expect planning README.md
    expect planning .engineering/planning/stories/a.md README.md CHANGELOG.md AGENTS.md docs/hardening/notes.md
    expect docs .engineering/planning/stories/a.md README.md website/docs/concepts/a.md
    expect docs website/docs/concepts/a.md
    expect docs website/src/css/custom.css
    expect code .engineering/planning/stories/a.md README.md crates/spindle-kernel/src/lib.rs
    expect code README.md Taskfile.yml
    expect code README.md .github/workflows/ci.yml
    expect code website/docs/a.md scripts/changed-paths.sh
    expect code .engineering/planning/stories/a.md docs/design/spindle.md
    expect code
    expect code ''

    # Every Markdown file outside website/ and .engineering/ that a Rust file under crates/ names
    # by its path outside a comment is read by a test, so a change to it must run the tests.
    named=$(cd "$root" && git ls-files '*.md' | while IFS= read -r path; do
        case $path in
            website/* | .engineering/* | crates/*) continue ;;
            */*) ;;
            *) continue ;;
        esac
        if grep -rhF --include='*.rs' "$path" crates | grep -qvE '^[[:space:]]*//'; then
            echo "$path"
        fi
    done)
    for path in $named; do
        expect code "$path"
    done

    work=$root/target/changed-paths-test
    rm -rf "$work"
    mkdir -p "$work"
    expect_github planning pull_request .engineering/planning/stories/a.md README.md
    expect_github docs pull_request README.md website/docs/a.md
    expect_github code pull_request README.md crates/a/src/lib.rs
    expect_github code push .engineering/planning/stories/a.md
    rm -rf "$work"

    if [ "$failures" -ne 0 ]; then
        echo "changed-paths: $failures of $cases case(s) failed" >&2
        exit 1
    fi
    echo "changed-paths: $cases cases passed"
}

script=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
root=$(cd "$(dirname "$0")/.." && pwd)

case ${1:-} in
    classify) classify ;;
    github) github ;;
    test) self_test ;;
    *)
        echo "usage: changed-paths.sh classify|github|test" >&2
        exit 2
        ;;
esac
