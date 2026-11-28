#!/bin/sh
# Adversarial cases for scripts/changed-paths.sh and scripts/within.sh (story:planning-only-prs-skip-tests,
# story:test-step-within-budget). POSIX sh; run as `sh scripts/gate-scripts.adversary-test.sh`.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
paths=${CHANGED_PATHS:-$root/scripts/changed-paths.sh}
within=${WITHIN:-$root/scripts/within.sh}
work=$root/target/gate-scripts-adversary
rm -rf "$work"
mkdir -p "$work"

failures=0
cases=0

fail() {
    echo "gate-scripts-adversary: FAIL: $*" >&2
    failures=$((failures + 1))
}

git_() {
    git -c user.name=test -c user.email=test@invalid -c init.defaultBranch=main "$@"
}

# A repository whose base holds a design document a test reads and one source file.
base_repo() {
    repo=$work/repo
    rm -rf "$repo"
    mkdir -p "$repo/docs/design" "$repo/crates/a/src"
    (
        cd "$repo"
        git_ init -q
        i=0
        while [ "$i" -lt 40 ]; do
            echo "line $i of the design document a test reads" >>docs/design/spindle.md
            echo "pub fn f$i() -> u32 { $i }" >>crates/a/src/lib.rs
            i=$((i + 1))
        done
        git_ add -A
        git_ commit -q -m base
        git_ checkout -q -b change
    )
}

# Commits what the change branch holds and merges it into main as GitHub's merge commit does.
merge_change() {
    (
        cd "$repo"
        git_ add -A
        git_ commit -q --allow-empty -m change
        git_ checkout -q main
        git_ merge -q --no-ff -m merge change
    )
}

github_class() {
    event=$1
    : >"$work/output"
    (cd "$repo" && GITHUB_EVENT_NAME=$event GITHUB_OUTPUT=$work/output sh "$paths" github) >/dev/null
    sed -n 's/^class=//p' "$work/output"
}

expect_github() {
    want=$1
    what=$2
    got=$(github_class "${3:-pull_request}")
    cases=$((cases + 1))
    [ "$got" = "$want" ] || fail "github: $what is '$got', want $want"
}

expect_classify() {
    want=$1
    shift
    got=$(printf '%s\n' "$@" | sh "$paths" classify)
    cases=$((cases + 1))
    [ "$got" = "$want" ] || fail "classify [$*] is '$got', want $want"
}

expect_within() {
    want=$1
    what=$2
    shift 2
    status=0
    sh "$within" "$@" >"$work/within.out" 2>"$work/within.err" || status=$?
    cases=$((cases + 1))
    [ "$status" -eq "$want" ] || fail "within $what exits $status, want $want: $(cat "$work/within.err")"
}

# --- changed-paths.sh github: what git diff --name-only reports for a rename or a delete -------

# Moving the design document rally.rs and adversary_rally.rs read breaks those tests: code.
base_repo
(cd "$repo" && mkdir -p docs/archive && git mv docs/design/spindle.md docs/archive/spindle.md)
merge_change
expect_github code "a rename of docs/design/spindle.md to docs/archive/spindle.md"

# Moving a source file out of crates/ into .engineering/ removes it from the build: code.
base_repo
(cd "$repo" && mkdir -p .engineering/attic && git mv crates/a/src/lib.rs .engineering/attic/lib.rs)
merge_change
expect_github code "a rename of crates/a/src/lib.rs into .engineering/attic/"

# Deleting the design document a test reads: code.
base_repo
(cd "$repo" && git rm -q docs/design/spindle.md)
merge_change
expect_github code "a delete of docs/design/spindle.md"

# A planning file whose name holds a space, alone: planning.
base_repo
(cd "$repo" && mkdir -p .engineering/planning && echo x >".engineering/planning/a b.md")
merge_change
expect_github planning "one .engineering/ file with a space in its name"

# A merge commit that changes nothing: code, never a skip on no evidence.
base_repo
merge_change
expect_github code "a merge commit with no changed path"

# A pull_request checkout that is not a merge commit: code.
base_repo
(cd "$repo" && echo x >README.md && git_ add -A && git_ commit -q -m one)
expect_github code "a pull_request HEAD with one parent"

# Any event but pull_request: code.
base_repo
(cd "$repo" && mkdir -p .engineering && echo x >.engineering/a.md)
merge_change
expect_github code "a merge_group event with a planning-only change" merge_group

# --- changed-paths.sh classify: odd path shapes ------------------------------------------------

expect_classify code -- crates/a/src/lib.rs
expect_classify code '-n' crates/a/src/lib.rs
expect_classify planning '-n.md'
expect_classify code 'crates/a/src/x y.rs' '.engineering/a.md'
expect_classify code docs/design/observe.md
# A planning path after a docs path keeps docs; the self-test lists docs paths last only.
expect_classify docs website/docs/a.md README.md

# --- within.sh: the child's status, a signal, the bound's form ---------------------------------

expect_within 3 "on a child that exits 3" 5 sh -c 'exit 3'
expect_within 143 "on a child that ends on SIGTERM" 5 sh -c 'kill -TERM $$'
expect_within 127 "on a command that does not exist" 5 "$work/no-such-command"
expect_within 2 "with a bound of 1.5" 1.5 true
expect_within 2 "with a bound of -1" -1 true
expect_within 2 "with an empty bound" '' true
expect_within 2 "with no command" 5
expect_within 0 "on true within 5 s" 5 true
expect_within 124 "on a 2-second sleep with a 1-second bound" 1 sleep 2

sh "$within" 5 sh -c 'echo out; echo err >&2' >"$work/within.out" 2>"$work/within.err"
cases=$((cases + 1))
[ "$(sed -n 1p "$work/within.out")" = out ] || fail "within does not pass the child's stdout through first"
cases=$((cases + 1))
grep -q '^err$' "$work/within.err" || fail "within does not pass the child's stderr through"
cases=$((cases + 1))
grep -Eq '^within: [0-9]+\.[0-9]{3} s of 5 s: ' "$work/within.out" || fail "within does not print its wall time against the bound: $(cat "$work/within.out")"

# --- check:postgres and CI's PostgreSQL job (story:postgres-lane-kills-slow-tests) -------------

ci=${CI_YML:-$root/.github/workflows/ci.yml}
taskfile=${TASKFILE:-$root/Taskfile.yml}
nextest_toml=${NEXTEST_TOML:-$root/.config/nextest.toml}

# Every job pins one cargo-nextest: one version and one digest, in the Gate and the PostgreSQL job.
cases=$((cases + 1))
versions=$(sed -n "s/^ *NEXTEST_VERSION: *'\{0,1\}\([^']*\)'\{0,1\} *$/\1/p" "$ci" | sort -u)
pins=$(grep -c '^ *NEXTEST_VERSION:' "$ci" || true)
[ "$pins" -ge 2 ] && [ "$(printf '%s\n' "$versions" | wc -l)" -eq 1 ] ||
    fail "ci.yml pins cargo-nextest $pins time(s) with the version(s) [$(echo $versions)], want one version in the Gate and the PostgreSQL job"
cases=$((cases + 1))
[ "$(sed -n 's/^ *NEXTEST_SHA256: *//p' "$ci" | sort -u | wc -l)" -eq 1 ] ||
    fail "ci.yml pins cargo-nextest with more than one digest"

# The PostgreSQL job installs cargo-nextest before it runs the lane.
postgres_job=$(awk '/^  postgres:/{on=1; next} on && /^  [a-z]/{on=0} on' "$ci")
cases=$((cases + 1))
printf '%s\n' "$postgres_job" | awk '/Install cargo-nextest/{seen=1} /run: task check:postgres/{if (seen) ok=1} END{exit !ok}' ||
    fail "ci.yml's postgres job does not install cargo-nextest before it runs task check:postgres"

# check:postgres builds first, then runs its tests under nextest's postgres profile within 120 s.
lane=$(awk '/^  check:postgres:/{on=1; next} on && /^  [a-z]/{on=0} on' "$taskfile")
cases=$((cases + 1))
printf '%s\n' "$lane" | grep -q -E 'NEXTEST: (env -u NEXTEST_TEST_THREADS )?cargo nextest run .*--profile postgres' ||
    fail "check:postgres does not run cargo nextest under the postgres profile"
cases=$((cases + 1))
printf '%s\n' "$lane" | awk '/--no-run/{built=1} /sh scripts\/within.sh [0-9]+ \{\{\.NEXTEST\}\}/{if (built) ok=1} END{exit !ok}' ||
    fail "check:postgres does not build with --no-run before its run under within.sh"

# The within.sh bounds of check:postgres's runs sum to the step's 120 s at most.
cases=$((cases + 1))
bounds=$(printf '%s\n' "$lane" | grep -o 'sh scripts/within.sh [0-9][0-9]*' | awk '{sum += $3; n++} END{print n + 0, sum + 0}')
set -- $bounds
[ "$1" -ge 1 ] && [ "$2" -le 120 ] ||
    fail "check:postgres runs $1 call(s) under within.sh whose bounds sum to $2 s, want at least one and at most 120 s"

# Every within.sh call of check:postgres runs {{.NEXTEST}}, the package set its --no-run built: another
# package set resolves other features and rebuilds inside the bound (a 2.9 s test took 26 s).
cases=$((cases + 1))
printf '%s\n' "$lane" | grep 'sh scripts/within.sh [0-9]' | grep -v -q 'within.sh [0-9][0-9]* {{\.NEXTEST}} ' &&
    fail "check:postgres runs a call under within.sh that is not {{.NEXTEST}}, so it may rebuild inside its bound"
cases=$((cases + 1))
printf '%s\n' "$lane" | grep -q 'test:slowest, vars: { NEXTEST_PROFILE: postgres }' ||
    fail "check:postgres does not print the ten slowest tests of its own profile"

# The postgres profile kills a test at 60 s as the default does: it sets no timeout of its own.
cases=$((cases + 1))
awk '/^\[/{section=$0} /slow-timeout/{print section " " $0}' "$nextest_toml" >"$work/timeouts"
[ "$(cat "$work/timeouts")" = '[profile.default] slow-timeout = { period = "30s", terminate-after = 2 }' ] ||
    fail "nextest.toml's slow-timeout is not the default profile's 30 s x 2 alone: $(cat "$work/timeouts")"

rm -rf "$work"
if [ "$failures" -ne 0 ]; then
    echo "gate-scripts-adversary: $failures of $cases case(s) failed" >&2
    exit 1
fi
echo "gate-scripts-adversary: $cases cases passed"
