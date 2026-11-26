#!/bin/sh
# Adversarial cases for scripts/wasm-cache-key.sh and the wasm lanes of Taskfile.yml
# (story:wasm-lanes-within-budget, wave 27 unit U6). POSIX sh; run as `sh scripts/adversary_w27_u6.sh`.
#
# Each mutant case copies this tree's Taskfile.yml and .cargo/config.toml, changes one thing a wasm
# lane could be written as, and expects `wasm-cache-key.sh check` on the copy to exit 1. A case whose
# change does not apply to the copy fails too, so no case passes on an unchanged file.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
guard=${WASM_CACHE_KEY:-$root/scripts/wasm-cache-key.sh}
work=$root/target/adversary_w27_u6
rm -rf "$work"
mkdir -p "$work"

failures=0
cases=0

fail() {
    echo "adversary_w27_u6: FAIL: $*" >&2
    failures=$((failures + 1))
}

# mutant <want> <what> <awk program run on Taskfile.yml> [<awk program run on config.toml>]
mutant() {
    want=$1
    what=$2
    dir=$work/case
    rm -rf "$dir"
    mkdir -p "$dir/.cargo"
    awk "$3" "$root/Taskfile.yml" >"$dir/Taskfile.yml"
    awk "${4:-1}" "$root/.cargo/config.toml" >"$dir/.cargo/config.toml"
    cases=$((cases + 1))
    if [ "$want" -ne 0 ] && cmp -s "$root/Taskfile.yml" "$dir/Taskfile.yml" &&
        cmp -s "$root/.cargo/config.toml" "$dir/.cargo/config.toml"; then
        fail "$what: the change does not apply to this tree's files"
        return 0
    fi
    got=0
    sh "$guard" check "$dir" >"$work/out" 2>"$work/err" || got=$?
    if [ "$got" -ne "$want" ]; then
        fail "$what: check exits $got, want $want: $(cat "$work/err" "$work/out")"
    fi
}

# Probes: the guard reads the real lanes, so a change to each real lane is caught.
mutant 0 "this tree as it is" 1
mutant 1 "the host build lane in --release" \
    '/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown --no-default-features -p spindle-spec/ { sub(/--profile wasm-gate/, "--release") } 1'
mutant 1 "the lint in wasm-gate" \
    '{ sub(/--profile wasm-lint/, "--profile wasm-gate") } 1'
mutant 1 "the memory-store lane in its own profile" \
    '/-p spindle-host --test spindle-host$/ { sub(/--profile wasm-gate/, "--profile wasm-gate-t1") } 1'
mutant 1 "the IndexedDB lane in no profile" \
    '/-p spindle-store-browser --test/ { sub(/--profile wasm-gate /, "") } 1'
mutant 1 "the Service Worker lane in --release" \
    '/-p spindle-wasm --test spindle-wasm$/ { sub(/--profile wasm-gate/, "--release") } 1'
mutant 1 "the static bundle lane in --release" \
    '/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p spindle-wasm$/ { sub(/--profile wasm-gate/, "--release") } 1'
mutant 1 "the static bundle lane through task web again" \
    'prev == "  check:the_static_bundle_builds:" || inlane { inlane = 1 } /^      - task: web:bind$/ && inlane { sub(/web:bind/, "web"); inlane = 0 } { prev = $0 } 1'
mutant 1 "the Node lane in --release" \
    '/-p spindle-node$/ { sub(/--profile wasm-gate/, "--release") } 1'
mutant 1 "the runner back in a lane env:" \
    '{ print } /^      WASM_BINDGEN_TEST_TIMEOUT:/ && !done { print "      CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER: wasm-bindgen-test-runner"; done = 1 }'
mutant 1 "no wasm runner in .cargo/config.toml" 1 \
    '!/^runner = "wasm-bindgen-test-runner"$/'

# Taskfile forms the guard reads past.
mutant 1 "the host lane's deps as a flow sequence, its build in --release" \
    '/^  check:the_wasm_build_of_the_host_succeeds:$/ { lane = 1 }
     lane && /^    deps:$/ { print "    deps: [wasm:host:build, wasm:host:lint]"; skip = 2; lane = 0; next }
     skip > 0 { skip--; next }
     /^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown --no-default-features -p spindle-spec/ { sub(/--profile wasm-gate/, "--release") }
     1'
mutant 1 "the Node lane run by the task command, its build in --release" \
    '/within.sh 120 [{][{][.]TASK_EXE[}][}] check:node_serves_a_specification$/ { sub(/[{][{][.]TASK_EXE[}][}]/, "task") }
     /-p spindle-node$/ { sub(/--profile wasm-gate/, "--release") }
     1'
mutant 1 "the Node lane build folded over two lines, in --release" \
    '/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p spindle-node$/ {
         print "      - >-"
         print "        cargo build --locked --release"
         print "        --target wasm32-unknown-unknown -p spindle-node"
         next
     }
     1'
mutant 1 "the Node lane cmds as a flow sequence, in --release" \
    '/^  check:node_serves_a_specification:$/ { lane = 1 }
     lane && /^    cmds:$/ { print "    cmds: [cargo build --locked --release --target wasm32-unknown-unknown -p spindle-node]"; skip = 3; lane = 0; next }
     skip > 0 { skip--; next }
     1'
mutant 1 "a quoted CARGO_* key in a lane env:" \
    '{ print } /^      WASM_BINDGEN_TEST_TIMEOUT:/ && !done { print "      \"CARGO_INCREMENTAL\": \"1\""; done = 1 }'
mutant 1 "a CARGO_* variable in the Taskfile env: as a flow mapping" \
    '/^env:$/ { print "env: { ESS_TOOLCHAIN: '\''0.99.1'\'', CARGO_INCREMENTAL: '\''1'\'' }"; skip = 1; next }
     skip > 0 { skip--; next }
     1'
mutant 1 "a lane build as a cmd: entry, in --release" \
    '/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p spindle-node$/ { sub(/- cargo/, "- cmd: cargo"); sub(/--profile wasm-gate/, "--release") } 1'

# task web still builds and binds the release profile; the bundle lane builds and binds wasm-gate.
if command -v task >/dev/null 2>&1; then
    cases=$((cases + 1))
    (cd "$root" && task --dry web) >"$work/web" 2>&1 || true
    grep -q -- '--profile release --target wasm32-unknown-unknown -p spindle-wasm' "$work/web" &&
        grep -q 'target/wasm32-unknown-unknown/release/spindle_wasm.wasm' "$work/web" ||
        fail "task web does not build and bind the release profile: $(cat "$work/web")"
    cases=$((cases + 1))
    (cd "$root" && task --dry check:the_static_bundle_builds) >"$work/bundle" 2>&1 || true
    grep -q -- '--profile wasm-gate --target wasm32-unknown-unknown -p spindle-wasm' "$work/bundle" &&
        grep -q 'target/wasm32-unknown-unknown/wasm-gate/spindle_wasm.wasm' "$work/bundle" &&
        ! grep -q 'wasm32-unknown-unknown/release' "$work/bundle" ||
        fail "the static bundle lane does not build and bind wasm-gate alone: $(cat "$work/bundle")"
fi

rm -rf "$work"
if [ "$failures" -ne 0 ]; then
    echo "adversary_w27_u6: $failures of $cases case(s) failed" >&2
    exit 1
fi
echo "adversary_w27_u6: $cases cases passed"
