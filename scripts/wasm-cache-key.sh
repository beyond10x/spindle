#!/bin/sh
# Holds the wasm lanes of `task check` on one compile-cache key (story:wasm-lanes-within-budget).
#
#   wasm-cache-key.sh check [<root>]   checks <root>/Taskfile.yml and <root>/.cargo/config.toml
#   wasm-cache-key.sh test             runs this script's stub cases
#
# sccache keys a crate on every CARGO_* variable in rustc's environment and on its target path, so
# a lane that sets one, or builds in a profile directory of its own, compiles every crate again.
# `check` fails when:
#   - Taskfile.yml sets a CARGO_* variable anywhere: a key of an `env:` block in any form, or an
#     assignment in a command;
#   - a cargo command for wasm32-unknown-unknown in a task `check` reaches builds in a profile other
#     than wasm-gate (cargo clippy: wasm-lint);
#   - one of the six lanes is not run by `check` under `scripts/within.sh 120`, or runs no cargo
#     command for wasm32-unknown-unknown this script can read, so a form it does not read fails
#     rather than passing with nothing checked;
#   - .cargo/config.toml names no runner for wasm32-unknown-unknown, which the lanes that run
#     `cargo test` on that target need.
#
# The Taskfile is read as YAML lines: block and flow sequences, block scalars (`|`, `>-`) and
# commands folded over several lines are joined into one command before they are read.
set -eu

lanes='check:the_wasm_build_of_the_host_succeeds check:a_command_runs_in_wasm_on_the_memory_store check:the_browser_store_keeps_state_in_indexeddb check:a_service_worker_serves_a_specification check:the_static_bundle_builds check:node_serves_a_specification'

taskfile_program=$(
    cat <<'AWK'
function fail(n, message) {
    printf "wasm-cache-key: %s:%d: %s\n", file, n, message > "/dev/stderr"
    failed = 1
}
function indent(s) {
    match(s, /^ */)
    return RLENGTH
}
function trim(s) {
    sub(/^[ \t]+/, "", s)
    sub(/[ \t]+$/, "", s)
    return s
}
function unquote(s) {
    s = trim(s)
    sub(/^["']/, "", s)
    sub(/["']$/, "", s)
    return s
}
function edge(from, to) {
    to = unquote(to)
    if (from != "" && to != "") {
        edges[from, ++edge_count[from]] = to
    }
}
# Starts the entry of a deps: or cmds: list whose first line is `text`.
function start(text, n) {
    flush()
    item_task = task
    item_section = section
    item_line = n
    item_indent = indent($0)
    item_text = trim(text)
    item_literal = (item_text ~ /(^|[ \t:])\|[-+]?$/)
}
function append(s) {
    s = trim(s)
    if (s == "") {
        return
    }
    if (item_text ~ /\\$/) {
        sub(/\\$/, "", item_text)
        item_text = item_text " " s
    } else if (item_literal) {
        item_text = item_text " ; " s
    } else {
        item_text = item_text " " s
    }
    if (s ~ /(^|[ \t:])\|[-+]?$/) {
        item_literal = 1
    }
}
# Plain names in a flow sequence `[a, b]` that follows `key:` in text, or that text is.
function flow_names(text, key, s, parts, n, i, p) {
    s = text
    if (key != "") {
        if (!match(s, key ":[ \t]*\\[[^]]*\\]")) {
            return
        }
        s = substr(s, RSTART, RLENGTH)
        sub(/^[^[]*\[/, "", s)
    } else {
        if (s !~ /^\[/) {
            return
        }
        sub(/^\[/, "", s)
    }
    sub(/\].*$/, "", s)
    n = split(s, parts, ",")
    for (i = 1; i <= n; i++) {
        p = trim(parts[i])
        if (p !~ /[ \t{}]/) {
            edge(item_task, p)
        }
    }
}
function flush(s, seg, word, parts) {
    if (item_task == "" || item_text == "") {
        item_text = ""
        return
    }
    # `task: <name>` in any form: a list entry, a flow mapping, a defer.
    s = item_text
    while (match(s, /task:[ \t]*[^] \t,}]+/)) {
        edge(item_task, substr(s, RSTART + 5, RLENGTH - 5))
        s = substr(s, RSTART + RLENGTH)
    }
    # The task executable run from a command: {{.TASK_EXE}}, task or a path ending in /task.
    s = " " item_text
    while (match(s, /[ \t;&|(]([^ \t;&|()]*\/)?(task|[{][{][.]TASK_EXE[}][}])[ \t]+[^- \t;&|)'"][^ \t;&|)'"]*/)) {
        seg = substr(s, RSTART + 1, RLENGTH - 1)
        sub(/^[^ \t]+[ \t]+/, "", seg)
        edge(item_task, seg)
        s = substr(s, RSTART + RLENGTH)
    }
    if (item_task == "check" && item_section == "cmds") {
        s = item_text
        while (match(s, /within\.sh[ \t]+120[ \t]+[^ \t;&|]+[ \t]+[^ \t;&|)'"]+/)) {
            seg = substr(s, RSTART, RLENGTH)
            s = substr(s, RSTART + RLENGTH)
            split(seg, parts, /[ \t]+/)
            word = parts[3]
            if (word == "{{.TASK_EXE}}" || word == "task" || word ~ /\/task$/) {
                within[unquote(parts[4])] = item_line
            }
        }
    }
    if (item_section == "deps") {
        flow_names(item_text, "")
        if (item_text !~ /[ \t{}[]/) {
            edge(item_task, item_text)
        }
    }
    flow_names(item_text, "deps")
    if (item_section == "cmds") {
        cargo(item_text)
    }
    item_text = ""
}
# Records each cargo command for wasm32-unknown-unknown in text, with its profile.
function cargo(text, s, seg, word, profile, cut) {
    # A template may hold `|`; it is never a profile this script accepts.
    s = " " text
    gsub(/[{][{][^}]*[}][}]/, "{{template}}", s)
    while (match(s, /[^A-Za-z0-9_.-]cargo[ \t]+(\+[^ \t]+[ \t]+)?[a-z][a-z-]*/)) {
        word = substr(s, RSTART + 1, RLENGTH - 1)
        sub(/^cargo[ \t]+(\+[^ \t]+[ \t]+)?/, "", word)
        s = substr(s, RSTART + RLENGTH)
        seg = s
        cut = length(seg) + 1
        if (match(seg, /[;&|]|[^A-Za-z0-9_.-]cargo[ \t]/)) {
            cut = RSTART
        }
        seg = " " substr(seg, 1, cut - 1) " "
        if (seg !~ /wasm32-unknown-unknown/) {
            continue
        }
        profile = "dev"
        if (match(seg, /[ \t]--profile[ \t=]+[^ \t'"]+/)) {
            profile = substr(seg, RSTART, RLENGTH)
            sub(/^[ \t]--profile[ \t=]+/, "", profile)
        } else if (seg ~ /[ \t]--release[ \t'"]/) {
            profile = "release"
        }
        builds++
        build_task[builds] = item_task
        build_line[builds] = item_line
        build_profile[builds] = profile
        build_word[builds] = word
        build_want[builds] = (word == "clippy") ? "wasm-lint" : "wasm-gate"
    }
}
# Fills reached[] with every task `from` runs, itself included.
function reach(from, head, tail, queue, i, to) {
    split("", reached)
    reached[from] = 1
    queue[1] = from
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
}

# A CARGO_* name set anywhere outside a comment or a description: a key, quoted or not, in a block
# or a flow mapping, or an assignment in a command.
{
    line = $0
    if (line !~ /^[ \t]*#/ && line !~ /^[ \t]*(desc|summary|prompt|label):/) {
        s = line
        while (match(s, /CARGO_[A-Z0-9_]+["']?[ \t]*[:=]/)) {
            name = substr(s, RSTART, RLENGTH)
            sub(/["']?[ \t]*[:=]$/, "", name)
            s = substr(s, RSTART + RLENGTH)
            if (line ~ /^[^ \t]/ || task == "") {
                fail(NR, "the Taskfile sets " name " for every task; a CARGO_* variable is part of the compile-cache key, so set it in .cargo/config.toml")
            } else {
                fail(NR, "task " task " sets " name "; a CARGO_* variable is part of the compile-cache key, so set it in .cargo/config.toml")
            }
        }
    }
}
/^[ \t]*#/ || /^[ \t]*$/ { next }
/^[^ \t]/ {
    flush()
    top = line
    sub(/:.*/, "", top)
    task = ""
    section = ""
    next
}
top != "tasks" { next }
indent(line) == 2 && line !~ /^  -/ {
    flush()
    rest = substr(line, 3)
    if (!match(rest, /:([ \t]|$)/)) {
        next
    }
    task = unquote(substr(rest, 1, RSTART - 1))
    tasks[task] = NR
    section = ""
    rest = trim(substr(rest, RSTART + 1))
    if (rest != "" && rest !~ /^#/) {
        section = "cmds"
        start(rest, NR)
        flush()
        section = ""
    }
    next
}
task == "" { next }
# A line deeper than the entry it follows continues it: a block scalar, a folded command, a
# mapping's keys, the rest of a flow sequence.
item_text != "" && indent(line) > item_indent {
    append(line)
    next
}
indent(line) <= 4 && match(line, /^ *[A-Za-z_]+:/) {
    flush()
    section = substr(line, RSTART, RLENGTH - 1)
    sub(/^ */, "", section)
    rest = trim(substr(line, RSTART + RLENGTH))
    if ((section == "cmds" || section == "deps") && rest != "" && rest !~ /^#/) {
        start(rest, NR)
    }
    next
}
(section == "cmds" || section == "deps") && line ~ /^ *- / {
    rest = line
    sub(/^ *- /, "", rest)
    start(rest, NR)
    next
}
{ flush() }
END {
    flush()
    reach("check")
    checked = 0
    for (i = 1; i <= builds; i++) {
        if (!(build_task[i] in reached)) {
            continue
        }
        checked++
        if (build_profile[i] != build_want[i]) {
            fail(build_line[i], "task " build_task[i] ", which task check reaches, runs cargo " build_word[i] " for wasm32-unknown-unknown in the profile " build_profile[i] "; a wasm lane builds in " build_want[i])
        }
    }
    n = split(lanes, lane, " ")
    for (j = 1; j <= n; j++) {
        if (!(lane[j] in tasks)) {
            fail(0, "names no task " lane[j] ", one of the six wasm lanes")
            continue
        }
        if (!(lane[j] in within)) {
            fail(tasks[lane[j]], "task check does not run " lane[j] " under scripts/within.sh 120")
        }
        reach(lane[j])
        count = 0
        for (i = 1; i <= builds; i++) {
            if (build_task[i] in reached) {
                count++
            }
        }
        if (count == 0) {
            fail(tasks[lane[j]], "task " lane[j] " runs no cargo command for wasm32-unknown-unknown that this script reads; write it as a list entry of cmds: so its profile is checked")
        }
    }
    if (!failed) {
        printf "wasm-cache-key: %s: the %d wasm lanes run under within.sh 120, %d cargo command(s) for wasm32-unknown-unknown checked, no CARGO_* variable\n", file, n, checked
    }
    exit (failed ? 1 : 0)
}
AWK
)

config_program=$(
    cat <<'AWK'
/^[ \t]*\[/ {
    header = $0
    gsub(/[ \t"']/, "", header)
    in_target = (header == "[target.wasm32-unknown-unknown]")
    next
}
in_target && /^[ \t]*runner[ \t]*=/ && /wasm-bindgen-test-runner/ { found = 1 }
/^[ \t]*["']?target["']?\.["']?wasm32-unknown-unknown["']?\.runner[ \t]*=/ && /wasm-bindgen-test-runner/ { found = 1 }
END {
    if (!found) {
        printf "wasm-cache-key: %s: names no runner for wasm32-unknown-unknown; write [target.wasm32-unknown-unknown] runner = \"wasm-bindgen-test-runner\" there, not CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER in a task\n", file > "/dev/stderr"
        exit 1
    }
    printf "wasm-cache-key: %s: the runner of wasm32-unknown-unknown is wasm-bindgen-test-runner\n", file
}
AWK
)

check() {
    status=0
    awk -v file=Taskfile.yml -v lanes="$lanes" "$taskfile_program" "$1/Taskfile.yml" || status=1
    awk -v file=.cargo/config.toml "$config_program" "$1/.cargo/config.toml" || status=1
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
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:the_wasm_build_of_the_host_succeeds
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:a_command_runs_in_wasm_on_the_memory_store
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:the_browser_store_keeps_state_in_indexeddb
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:a_service_worker_serves_a_specification
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:the_static_bundle_builds
      - sh scripts/within.sh 120 {{.TASK_EXE}} check:node_serves_a_specification
      - cargo clippy --workspace --all-targets --locked -- -D warnings
      - defer: { task: test:slowest }

  check:conformance:
    cmds:
      - cargo run -q --locked -p a -- conform

  test:slowest:
    cmds:
      - echo slowest

  check:the_wasm_build_of_the_host_succeeds:
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

  check:a_command_runs_in_wasm_on_the_memory_store:
    env:
      WASM_BINDGEN_TEST_TIMEOUT: '60'
    cmds:
      - cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p a --test a

  check:the_browser_store_keeps_state_in_indexeddb:
    env:
      WASM_BINDGEN_TEST_TIMEOUT: '60'
    cmds:
      - cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p c --test c

  check:a_service_worker_serves_a_specification:
    env:
      WASM_BINDGEN_TEST_TIMEOUT: '60'
    cmds:
      - cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p b --test b

  check:node_serves_a_specification:
    cmds:
      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d
      - wasm-bindgen --target web --out-dir target/d target/wasm32-unknown-unknown/wasm-gate/d.wasm
      - node --test --test-timeout=60000 'd/*.test.mjs'

  check:the_static_bundle_builds:
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
    cases=$((cases + 1))
    # Each file is written only when the case changes it, so a case costs few processes.
    if [ -n "$4" ]; then
        sed "$4" "$work/good/Taskfile.yml" >"$dir/Taskfile.yml"
    elif [ "$taskfile_changed" = yes ]; then
        cat "$work/good/Taskfile.yml" >"$dir/Taskfile.yml"
    fi
    if [ -n "$4" ]; then taskfile_changed=yes; else taskfile_changed=no; fi
    if [ -n "$5" ]; then
        sed "$5" "$work/good/.cargo/config.toml" >"$dir/.cargo/config.toml"
    elif [ "$config_changed" = yes ]; then
        cat "$work/good/.cargo/config.toml" >"$dir/.cargo/config.toml"
    fi
    if [ -n "$5" ]; then config_changed=yes; else config_changed=no; fi
    if [ "$want" -ne 0 ] && { [ -z "$4" ] || cmp -s "$work/good/Taskfile.yml" "$dir/Taskfile.yml"; } &&
        { [ -z "$5" ] || cmp -s "$work/good/.cargo/config.toml" "$dir/.cargo/config.toml"; }; then
        echo "wasm-cache-key: FAIL: $what: the change does not apply to the stub" >&2
        failures=$((failures + 1))
        return 0
    fi
    got=0
    check "$dir" >"$work/out" 2>"$work/err" || got=$?
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
    mkdir -p "$work/good/.cargo" "$work/case/.cargo"
    good_taskfile >"$work/good/Taskfile.yml"
    good_config >"$work/good/.cargo/config.toml"
    cat "$work/good/.cargo/config.toml" >"$work/case/.cargo/config.toml"
    cat "$work/good/Taskfile.yml" >"$work/case/Taskfile.yml"
    config_changed=no
    taskfile_changed=no

    # Each side of the CARGO_* rule.
    expect 0 "the stub as written" '' '' ''
    expect 1 "a lane that sets the runner in env:" 'task check:a_command_runs_in_wasm_on_the_memory_store sets CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER' \
        "s/^      WASM_BINDGEN_TEST_TIMEOUT: '60'\$/&\\
      CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER: wasm-bindgen-test-runner/" ''
    expect 1 "a task no lane reaches that sets a CARGO_* variable in env:" 'task web sets CARGO_PROFILE_RELEASE_LTO' \
        's/^  web:$/&\
    env:\
      CARGO_PROFILE_RELEASE_LTO: "true"/' ''
    expect 1 "a CARGO_* variable in an env: flow mapping" 'task wasm:build sets CARGO_INCREMENTAL' \
        's/^  wasm:build:$/&\
    env: { CARGO_INCREMENTAL: "0" }/' ''
    expect 1 "a CARGO_* variable in the Taskfile's own env:" 'sets CARGO_BUILD_TARGET for every task' \
        "s/^  ESS_TOOLCHAIN: '0.99.1'\$/&\\
  CARGO_BUILD_TARGET: wasm32-unknown-unknown/" ''
    expect 1 "a CARGO_* variable set before a command" 'sets CARGO_TARGET_DIR' \
        's/^      - cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p a/      - CARGO_TARGET_DIR=target\/t cargo test --locked --profile wasm-gate --target wasm32-unknown-unknown -p a/' ''
    expect 0 "a variable that is not CARGO_* in env: (CLIPPY_CONF_DIR, WASM_BINDGEN_TEST_TIMEOUT)" '' '' ''
    expect 1 "a quoted CARGO_* key in env:" 'task wasm:lint sets CARGO_INCREMENTAL' \
        "s/^      CLIPPY_CONF_DIR: .*/&\\
      \"CARGO_INCREMENTAL\": \"1\"/" ''
    expect 1 "a CARGO_* variable in the Taskfile's env: as a flow mapping" 'sets CARGO_INCREMENTAL for every task' \
        "/^env:\$/{N;s/.*/env: { ESS_TOOLCHAIN: '0.99.1', CARGO_INCREMENTAL: '1' }/;}" ''
    expect 1 "a CARGO_* variable exported in a block scalar" 'task check:node_serves_a_specification sets CARGO_INCREMENTAL' \
        's/^      - node --test .*/      - |\
        export CARGO_INCREMENTAL=1\
        node --test x/' ''

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
    expect 1 "deps as a flow sequence, a build in --release" 'task wasm:build, which task check reaches, runs cargo build for wasm32-unknown-unknown in the profile release' \
        '/^    deps:$/{N;N;s/.*/    deps: [wasm:build, wasm:lint]/;}; s/--profile wasm-gate --target wasm32-unknown-unknown -p a$/--release --target wasm32-unknown-unknown -p a/' ''
    expect 1 "a lane as a flow mapping with deps, a build in --release" 'task wasm:build, which task check reaches, runs cargo build for wasm32-unknown-unknown in the profile release' \
        '/^  check:the_wasm_build_of_the_host_succeeds:$/{N;N;N;N;s/.*/  check:the_wasm_build_of_the_host_succeeds: { deps: [wasm:build, wasm:lint] }/;}; s/--profile wasm-gate --target wasm32-unknown-unknown -p a$/--release --target wasm32-unknown-unknown -p a/' ''
    expect 0 "deps as a flow sequence" '' \
        '/^    deps:$/{N;N;s/.*/    deps: [wasm:build, wasm:lint]/;}' ''
    expect 1 "a build folded over two lines with >-, in --release" 'task check:node_serves_a_specification, which task check reaches, runs cargo build for wasm32-unknown-unknown in the profile release' \
        's/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d$/      - >-\
        cargo build --locked --release\
        --target wasm32-unknown-unknown -p d/' ''
    expect 1 "a build continued with a backslash in a | block, in --release" 'in the profile release' \
        's/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d$/      - |\
        cargo build --locked --release \\\
          --target wasm32-unknown-unknown -p d/' ''
    expect 1 "cmds as a flow sequence on the section line, in --release" 'task check:node_serves_a_specification, which task check reaches, runs cargo build' \
        '/^  check:node_serves_a_specification:$/{n;N;N;N;s/.*/    cmds: [cargo build --locked --release --target wasm32-unknown-unknown -p d]/;}' ''
    expect 1 "a task in the short form, in --release" 'task wasm:build, which task check reaches, runs cargo build' \
        '/^  wasm:build:$/{N;N;s/.*/  wasm:build: cargo build --locked --release --target wasm32-unknown-unknown -p a/;}' ''
    expect 0 "a native cargo command beside a wasm-bindgen line in a | block" '' \
        's/^      - node --test .*/      - |\
        cargo fetch --locked\
        wasm-bindgen --out-dir x target\/wasm32-unknown-unknown\/wasm-gate\/d.wasm/' ''
    expect 1 "a build as a cmd: entry, in --release" 'in the profile release' \
        's/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d$/      - cmd: cargo build --locked --release --target wasm32-unknown-unknown -p d/' ''
    expect 1 "two cargo commands in one entry, the second in --release" 'in the profile release' \
        's/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d$/      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d \&\& cargo build --release --target wasm32-unknown-unknown -p e/' ''

    # Each side of the rule that every lane is run and read.
    expect 1 "a lane run by the task command, its build in --release" 'in the profile release' \
        's/{{.TASK_EXE}} check:node_serves_a_specification$/task check:node_serves_a_specification/; s/--profile wasm-gate --target wasm32-unknown-unknown -p d$/--release --target wasm32-unknown-unknown -p d/' ''
    expect 0 "a lane run by the task command" '' \
        's/{{.TASK_EXE}} check:node_serves_a_specification$/task check:node_serves_a_specification/' ''
    expect 1 "a lane check does not run under within.sh 120" 'task check does not run check:node_serves_a_specification under scripts/within.sh 120' \
        's/^      - sh scripts\/within.sh 120 {{.TASK_EXE}} check:node_serves_a_specification$/      - task: check:node_serves_a_specification/' ''
    expect 1 "a lane missing from the Taskfile" 'names no task check:the_static_bundle_builds' \
        's/^  check:the_static_bundle_builds:$/  check:the_static_bundle_built:/' ''
    expect 1 "a lane whose build this script cannot read" 'task check:node_serves_a_specification runs no cargo command for wasm32-unknown-unknown' \
        's/^      - cargo build --locked --profile wasm-gate --target wasm32-unknown-unknown -p d$/      - sh build-node.sh/' ''

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
