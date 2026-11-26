#!/bin/sh
# Holds the wasm lanes of `task check` on one compile-cache key (story:wasm-lanes-within-budget).
#
#   wasm-cache-key.sh check [<root>]   checks <root>/Taskfile.yml and <root>/.cargo/config.toml
#   wasm-cache-key.sh test             runs this script's stub cases
#
# sccache keys a crate on every CARGO_* variable in rustc's environment and on its target path, so
# a lane that sets one, or builds in a profile directory of its own, compiles every crate again.
# `check` fails when:
#   - Taskfile.yml sets a CARGO_* variable, in an `env:` block or before a command;
#   - a cargo command for wasm32-unknown-unknown in a task `check` reaches builds in a profile other
#     than wasm-gate (cargo clippy: wasm-lint);
#   - .cargo/config.toml names no runner for wasm32-unknown-unknown, which the lanes that run
#     `cargo test` on that target need.
set -eu

check() {
    cd "$1"
    taskfile=Taskfile.yml
    config=.cargo/config.toml
    status=0

    awk -v file="$taskfile" '
        function fail(n, message) {
            printf "wasm-cache-key: %s:%d: %s\n", file, n, message > "/dev/stderr"
            failed = 1
        }
        function indent(s) {
            match(s, /^ */)
            return RLENGTH
        }
        function edge(from, to) {
            gsub(/["'\'' \t}]/, "", to)
            if (to != "") {
                edges[from, ++edge_count[from]] = to
            }
        }
        function references(line, s, rest) {
            s = line
            while (match(s, /task:[ \t]*[^ \t,}]+/)) {
                rest = substr(s, RSTART + 5, RLENGTH - 5)
                edge(task, rest)
                s = substr(s, RSTART + RLENGTH)
            }
            s = line
            while (match(s, /[{][{][.]TASK_EXE[}][}][ \t]+[^ \t;&|]+/)) {
                rest = substr(s, RSTART + 13, RLENGTH - 13)
                edge(task, rest)
                s = substr(s, RSTART + RLENGTH)
            }
        }
        { line = $0 }
        /^[ \t]*#/ { next }
        /^[^ \t]/ {
            top = $0
            sub(/:.*/, "", top)
            task = ""
            section = ""
            next
        }
        top == "env" && match(line, /^ +[A-Za-z_][A-Za-z0-9_]*:/) {
            name = substr(line, RSTART, RLENGTH - 1)
            sub(/^ +/, "", name)
            if (name ~ /^CARGO_/) {
                fail(NR, "the Taskfile sets " name " for every task; a CARGO_* variable is part of the compile-cache key, so set it in .cargo/config.toml")
            }
            next
        }
        top != "tasks" { next }
        indent(line) == 2 && line ~ /^  [^ #-][^ ]*:[ \t]*$/ {
            task = line
            sub(/^  /, "", task)
            sub(/:[ \t]*$/, "", task)
            gsub(/["'\'']/, "", task)
            section = ""
            next
        }
        indent(line) == 4 && match(line, /^    [A-Za-z_]+:/) {
            section = substr(line, 5, RLENGTH - 5)
            if (section == "env" && line ~ /CARGO_[A-Z0-9_]*[ \t]*:/) {
                fail(NR, "task " task " sets a CARGO_* variable in env:; it is part of the compile-cache key, so set it in .cargo/config.toml")
            }
            if (section == "deps" || section == "cmds") {
                references(line)
            }
            next
        }
        task == "" { next }
        section == "env" && match(line, /^ +[A-Za-z_][A-Za-z0-9_]*:/) {
            name = substr(line, RSTART, RLENGTH - 1)
            sub(/^ +/, "", name)
            if (name ~ /^CARGO_/) {
                fail(NR, "task " task " sets " name " in env:; a CARGO_* variable is part of the compile-cache key, so set it in .cargo/config.toml")
            }
            next
        }
        section == "deps" {
            references(line)
            if (match(line, /^ +- +[^ \t{]+[ \t]*$/)) {
                plain = line
                sub(/^ +- +/, "", plain)
                edge(task, plain)
            }
            next
        }
        section == "cmds" {
            references(line)
            s = " " line
            if (match(s, /[^A-Za-z0-9_]CARGO_[A-Z0-9_]*=/)) {
                name = substr(s, RSTART + 1, RLENGTH - 2)
                fail(NR, "task " task " sets " name " before a command; a CARGO_* variable is part of the compile-cache key, so set it in .cargo/config.toml")
            }
            if (s ~ /wasm32-unknown-unknown/ && match(s, /[ \t'\''"(;&|]cargo[ \t]+(\+[^ \t]+[ \t]+)?[a-z][a-z-]*/)) {
                word = substr(s, RSTART + 1, RLENGTH - 1)
                sub(/^cargo[ \t]+(\+[^ \t]+[ \t]+)?/, "", word)
                profile = "dev"
                if (match(s, /--profile[ \t=]+[^ \t'\''"]+/)) {
                    profile = substr(s, RSTART, RLENGTH)
                    sub(/^--profile[ \t=]+/, "", profile)
                } else if (s ~ /[ \t]--release([ \t'\''"]|$)/) {
                    profile = "release"
                }
                want = (word == "clippy") ? "wasm-lint" : "wasm-gate"
                builds[++build_count] = task
                build_line[build_count] = NR
                build_profile[build_count] = profile
                build_want[build_count] = want
                build_word[build_count] = word
            }
            next
        }
        END {
            reached["check"] = 1
            queue[1] = "check"
            head = 1
            tail = 1
            while (head <= tail) {
                from = queue[head++]
                for (i = 1; i <= edge_count[from]; i++) {
                    to = edges[from, i]
                    if (!(to in reached)) {
                        reached[to] = 1
                        queue[++tail] = to
                    }
                }
            }
            for (i = 1; i <= build_count; i++) {
                if ((builds[i] in reached) && build_profile[i] != build_want[i]) {
                    fail(build_line[i], "task " builds[i] ", which task check reaches, runs cargo " build_word[i] " for wasm32-unknown-unknown in the profile " build_profile[i] "; a wasm lane builds in " build_want[i])
                }
            }
            exit (failed ? 1 : 0)
        }
    ' "$taskfile" || status=1

    awk -v file="$config" '
        /^[ \t]*\[/ {
            header = $0
            gsub(/[ \t"]/, "", header)
            in_target = (header == "[target.wasm32-unknown-unknown]")
            next
        }
        in_target && /^[ \t]*runner[ \t]*=/ && /wasm-bindgen-test-runner/ { found = 1 }
        /^[ \t]*"?target"?\."?wasm32-unknown-unknown"?\.runner[ \t]*=/ && /wasm-bindgen-test-runner/ { found = 1 }
        END {
            if (!found) {
                printf "wasm-cache-key: %s: names no runner for wasm32-unknown-unknown; write [target.wasm32-unknown-unknown] runner = \"wasm-bindgen-test-runner\" there, not CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER in a task\n", file > "/dev/stderr"
                exit 1
            }
        }
    ' "$config" || status=1

    if [ "$status" -eq 0 ]; then
        echo "wasm-cache-key: no CARGO_* variable in $taskfile, every wasm lane in wasm-gate (lint: wasm-lint), the wasm runner in $config"
    fi
    return "$status"
}

# --- stub cases -------------------------------------------------------------------------------

failures=0
cases=0

good_taskfile() {
    cat <<'EOF'
version: '3'

env:
  ESS_TOOLCHAIN: '0.99.1'

tasks:
  check:
    cmds:
      - task: check:conformance
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:lane_build
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:lane_test
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:lane_bundle
      - cargo clippy --workspace --all-targets --locked -- -D warnings
      - defer: { task: test:slowest }

  check:conformance:
    cmds:
      - cargo run -q --locked -p a -- conform

  test:slowest:
    cmds:
      - echo slowest

  check:lane_build:
    desc: builds for wasm32-unknown-unknown with cargo build --release
    deps:
      - task: wasm:build
      - wasm:lint

  wasm:build:
    cmds:
      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p a

  wasm:lint:
    env:
      CLIPPY_CONF_DIR: '{{.ROOT_DIR}}/clippy/wasm32'
    cmds:
      - cargo clippy --locked --profile wasm-lint --target wasm32-unknown-unknown -p a --lib -- -D warnings

  check:lane_test:
    env:
      WASM_BINDGEN_TEST_TIMEOUT: '60'
    cmds:
      - cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p a --test a

  check:lane_bundle:
    cmds:
      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p b
      - task: web:bind
        vars:
          PROFILE: wasm-gate

  web:
    vars:
      PROFILE: '{{.PROFILE | default "release"}}'
    cmds:
      - cargo build --locked --profile {{.PROFILE}} --target wasm32-unknown-unknown -p b
      - task: web:bind
        vars:
          PROFILE: '{{.PROFILE}}'

  web:bind:
    cmds:
      - wasm-bindgen --target web --out-dir web/src/wasm target/wasm32-unknown-unknown/{{.PROFILE}}/b.wasm
EOF
}

good_config() {
    cat <<'EOF'
[env]
ESS_TOOLCHAIN = "0.99.1"

[target.wasm32-unknown-unknown]
runner = "wasm-bindgen-test-runner"
EOF
}

# expect <0|1> <what> <message the failure names, or ''> <sed script on Taskfile.yml> <sed script on config.toml>
expect() {
    want=$1
    what=$2
    names=$3
    dir=$work/case
    rm -rf "$dir"
    mkdir -p "$dir/.cargo"
    good_taskfile | sed "$4" >"$dir/Taskfile.yml"
    good_config | sed "$5" >"$dir/.cargo/config.toml"
    got=0
    (check "$dir") >"$work/out" 2>"$work/err" || got=$?
    cases=$((cases + 1))
    if [ "$got" -ne "$want" ]; then
        echo "wasm-cache-key: FAIL: $what: check exits $got, want $want: $(cat "$work/err")" >&2
        failures=$((failures + 1))
    elif [ -n "$names" ] && ! grep -qF -- "$names" "$work/err"; then
        echo "wasm-cache-key: FAIL: $what: the failure does not name '$names': $(cat "$work/err")" >&2
        failures=$((failures + 1))
    fi
}

self_test() {
    work=$root/target/wasm-cache-key-test
    rm -rf "$work"
    mkdir -p "$work"

    # Each side of the CARGO_* rule.
    expect 0 "the stub as written" '' '' ''
    expect 1 "a lane that sets the runner in env:" 'sets CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER in env:' \
        "s/^      WASM_BINDGEN_TEST_TIMEOUT: '60'\$/&\\
      CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER: wasm-bindgen-test-runner/" ''
    expect 1 "a task no lane reaches that sets a CARGO_* variable in env:" 'task web sets CARGO_PROFILE_RELEASE_LTO' \
        's/^  web:$/&\
    env:\
      CARGO_PROFILE_RELEASE_LTO: "true"/' ''
    expect 1 "a CARGO_* variable in an env: flow mapping" 'task wasm:build sets a CARGO_* variable' \
        's/^  wasm:build:$/&\
    env: { CARGO_INCREMENTAL: "0" }/' ''
    expect 1 "a CARGO_* variable in the Taskfile's own env:" 'sets CARGO_BUILD_TARGET for every task' \
        "s/^  ESS_TOOLCHAIN: '0.99.1'\$/&\\
  CARGO_BUILD_TARGET: wasm32-unknown-unknown/" ''
    expect 1 "a CARGO_* variable set before a command" 'sets CARGO_TARGET_DIR before a command' \
        's/^      - cargo test --locked --profile wasm-gate/      - CARGO_TARGET_DIR=target\/t cargo test --locked --profile wasm-gate/' ''
    expect 0 "a variable that is not CARGO_* in env: (CLIPPY_CONF_DIR, WASM_BINDGEN_TEST_TIMEOUT)" '' '' ''

    # Each side of the profile rule.
    expect 1 "a lane build in the release profile" 'runs cargo build for wasm32-unknown-unknown in the profile release' \
        's/cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p a/cargo build --locked --release --target wasm32-unknown-unknown -p a/' ''
    expect 1 "a lane build in no profile (dev)" 'in the profile dev' \
        's/cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p b/cargo build --locked --target wasm32-unknown-unknown -p b/' ''
    expect 1 "a lane test in a profile of its own" 'in the profile wasm-gate-t1' \
        's/cargo test --locked --profile wasm-gate /cargo test --locked --profile=wasm-gate-t1 /' ''
    expect 1 "the lint in wasm-gate" 'runs cargo clippy for wasm32-unknown-unknown in the profile wasm-gate; a wasm lane builds in wasm-lint' \
        's/--profile wasm-lint/--profile wasm-gate/' ''
    expect 1 "a lane test in wasm-lint" 'runs cargo test for wasm32-unknown-unknown in the profile wasm-lint' \
        's/cargo test --locked --profile wasm-gate /cargo test --locked --profile wasm-lint /' ''
    expect 1 "a lane that reaches the release build of web" 'task web, which task check reaches' \
        's/^      - task: web:bind$/      - task: web/' ''
    expect 1 "a lane reached as a plain dep" 'task wasm:lint, which task check reaches' \
        's/--profile wasm-lint/--release/' ''
    expect 0 "the release build of web, which no lane reaches" '' '' ''

    # Each side of the runner rule.
    expect 1 "a config with no wasm target" 'names no runner for wasm32-unknown-unknown' \
        '' '/^\[target/,$d'
    expect 1 "a wasm target with no runner" 'names no runner for wasm32-unknown-unknown' \
        '' '/^runner/d'
    expect 1 "the runner under another target" 'names no runner for wasm32-unknown-unknown' \
        '' 's/^\[target.wasm32-unknown-unknown\]$/[target.x86_64-unknown-linux-gnu]/'
    expect 0 "the runner as a dotted key" '' \
        '' '/^\[target/d; s/^runner = /target.wasm32-unknown-unknown.runner = /'
    expect 0 "the runner under a quoted target" '' \
        '' 's/^\[target.wasm32-unknown-unknown\]$/[target."wasm32-unknown-unknown"]/'

    rm -rf "$work"
    if [ "$failures" -ne 0 ]; then
        echo "wasm-cache-key: $failures of $cases case(s) failed" >&2
        exit 1
    fi
    echo "wasm-cache-key: $cases cases passed"
}

root=$(cd "$(dirname "$0")/.." && pwd)

case ${1:-} in
    check) check "${2:-$root}" ;;
    test) self_test ;;
    *)
        echo "usage: wasm-cache-key.sh check [<root>]|test" >&2
        exit 2
        ;;
esac
