---
format: aep.planning-md/3
id: review-result:wave27-adversary-U6-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U6
relations:
- reviews: story:wasm-lanes-within-budget
revision: 1
---
unit: story:wasm-lanes-within-budget (U6). Findings cover 9449dd9ff, and the test commit f6e4980d0 is on top.
verdict: CONFIRMED
cases: executed 20→40, red 6
origin: introduced 6 / pre-existing 0 / undecided 0
wrote-outside-worktree: 3 paths, all in the assigned adv-scratch
needs-coordinator: the new script `scripts/adversary_w27_u6.sh` is not wired into `Taskfile.yml`, which is the implementor's file. Run it by hand with `slot sh scripts/adversary_w27_u6.sh`.

**1. `git --no-pager diff --stat 9449dd9ff HEAD`**
```
 scripts/adversary_w27_u6.sh | 122 ++++++++++++++++++++++++++++++++++++++++++++
 1 file changed, 122 insertions(+)
```
That is one test file and no other path.

**2. Cases added** in `<local path>` (POSIX sh, about 4 s, written before anything ran):

Each case copies the tree's real `Taskfile.yml` and `.cargo/config.toml` into `target/adversary_w27_u6`, changes one thing, and expects `wasm-cache-key.sh check` on the copy to exit 1. A case whose change does not apply to the copy fails, so it cannot pass vacuously.

- **14 green**, kept as proof that the guard reads the real lanes:
  - the unchanged tree passes;
  - a release, missing or foreign profile in each of the six lanes is caught, and so is the lint;
  - `task: web` put back in the bundle lane is caught;
  - the runner put back in a lane's `env:` is caught;
  - a config with no runner is caught;
  - a `- cmd:` entry is caught;
  - `task --dry web` still builds and binds `release`;
  - `task --dry check:the_static_bundle_builds` builds and binds `wasm-gate` only.
- **6 red.** The first run of the case file alone printed this verbatim:
```
adversary_w27_u6: FAIL: the host lane's deps as a flow sequence, its build in --release: check exits 0, want 1: wasm-cache-key: no CARGO_* variable in Taskfile.yml, every wasm lane in wasm-gate (lint: wasm-lint), the wasm runner in .cargo/config.toml
adversary_w27_u6: FAIL: the Node lane run by the task command, its build in --release: check exits 0, want 1: ...
adversary_w27_u6: FAIL: the Node lane build folded over two lines, in --release: check exits 0, want 1: ...
adversary_w27_u6: FAIL: the Node lane cmds as a flow sequence, in --release: check exits 0, want 1: ...
adversary_w27_u6: FAIL: a quoted CARGO_* key in a lane env:: check exits 0, want 1: ...
adversary_w27_u6: FAIL: a CARGO_* variable in the Taskfile env: as a flow mapping: check exits 0, want 1: ...
adversary_w27_u6: 6 of 20 case(s) failed
```
- **Task accepts each red form.** `task --dry` on the mutant copies printed the hidden `cargo build --locked --release --target wasm32-unknown-unknown …` for each lane form. The quoted-key and flow-env copies parsed without error.

**3. Suite run, after the cases existed**
- `slot task check:the_wasm_lanes_share_one_cache_key` exited 0: "wasm-cache-key: 20 cases passed", and `check` passed on the tree. Took 3.9 s.
- `slot sh scripts/adversary_w27_u6.sh` exited 1: "adversary_w27_u6: 6 of 20 case(s) failed". Took 4.1 s.

**4. Findings.** All are in `scripts/wasm-cache-key.sh` at 9449dd9ff.
- **What reaches them:** no form is in the tree today. Each is a valid Taskfile form that the next edit to `Taskfile.yml` could write, and the guard then passes a release or `CARGO_*` lane, against its acceptance read literally.
- **:83-85 and :97-103 (warning):** `deps: [a, b]`, a common Taskfile idiom, adds no edges, so the lane's builds are never checked.
- **:46 and :147-152 (warning):** lanes are reached only through `task:` or `{{.TASK_EXE}}`. Nothing checks that the six named lanes were found at all, so `within.sh 120 task check:…` (what `{{.TASK_EXE}}` expands to) passes with nothing checked.
- **:113 (note):** the check is per line. A cargo command folded with `>-`, with `--target wasm32-unknown-unknown` on the next line, is missed.
- **:83-86 (note):** `cmds: [ … ]` on the section line is never checked for cargo.
- **:89 (note):** a quoted key such as `"CARGO_INCREMENTAL":` does not match the key regex.
- **:54-61 (note):** a flow-mapped top-level `env: { …, CARGO_X: … }` is skipped. The task-level flow form is caught at :80.
- **Fix to name (not applied):** check that each of the six lane names is reached and holds at least one wasm cargo command. Or read the commands from `task --dry <lane>`, which resolves flow forms, folding and `task` invocations.

**5. Attacked and not broken**
- **POSIX portability:** the self-test and `check` pass under dash, and under `gawk --traditional` with dash on the build node. mawk and busybox exist on neither this host nor the node, so they are not tested.
- **The runner in `.cargo/config.toml`:** it applies only to `wasm32-unknown-unknown`. Native clippy, nextest and the release builds don't read it. CI's Gate already installs `wasm-bindgen-test-runner`. `check-ess-pin.sh` and `conform.rs:619` were green in the implementor's gate run.
- **`task web`:** it still builds and binds `release`. Shown by `task --dry`, and kept as a test.
- **CI and local read the same lanes:** CI's Gate runs `task check`, which holds the new step and the six `within.sh 120` lanes.
- **One compile set (budget mechanism):** the implementor's ordered logs on an emptied `target/` show no ESS crate compiled again after the first lane. The memory-store lane compiled 14 crates: `wasm-bindgen-test` and `spindle-host`, no ESS. The bundle lane compiled 0.
- **Budget times:** in order, 37.6, 10.5, 17.0, 17.9, 36.9 and 10.1 s. Alone, 17 to 24 s each. All are under 120 s.
- **Not re-run cold:** I did not re-measure the cold budget myself, because I may not delete build directories. The implementor's first run in this worktree, 134 s, is excluded under D1 A.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>` (mutant copies)
- `<local path>`

On the node, the `target/adversary_w27_u6` and `target/adv-u6-awk` directories my runs created were removed by those runs.

**7. Findings block**
```findings
- file: scripts/wasm-cache-key.sh
  line: 83
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "a lane whose deps are written as a flow sequence (deps: [a, b]) adds no edges, so a release wasm build under it passes the guard"
- file: scripts/wasm-cache-key.sh
  line: 147
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "lanes are reached only through task: or {{.TASK_EXE}} and no lane count is checked, so a lane run as `task check:…` is unreached and the guard passes with nothing checked"
- file: scripts/wasm-cache-key.sh
  line: 113
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "the profile check reads one line at a time, so a cargo command folded over two lines with the wasm target on the second is missed"
- file: scripts/wasm-cache-key.sh
  line: 84
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "a cmds: flow sequence on the section line is never checked for a cargo wasm build"
- file: scripts/wasm-cache-key.sh
  line: 89
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "a quoted CARGO_* key in a task env: does not match the key regex and passes"
- file: scripts/wasm-cache-key.sh
  line: 54
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "a CARGO_* variable in a flow-mapped top-level env: is skipped with the top-level line and passes"
```
