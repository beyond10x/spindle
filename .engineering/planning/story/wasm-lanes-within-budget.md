---
format: aep.planning-md/3
id: story:wasm-lanes-within-budget
kind: story
status: active
title: The wasm lanes of the gate end within the step budget
relations:
- serves: vision:spindle
- depends_on: story:test-step-within-budget
scope:
- confidence: cited
  path: .cargo/config.toml
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: scripts/wasm-cache-key.sh
revision: 22
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":19}}}
---
# The wasm lanes of the gate end within the step budget

## Outcome

Each wasm lane of `task check` ends within 120 s of wall time on a cold cache, and no test in
them runs longer than 60 s: `check:node_serves_a_specification`,
`check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store`,
`check:the_browser_store_keeps_state_in_indexeddb`, `check:a_service_worker_serves_a_specification`
and `check:the_static_bundle_builds`.

Measured 2026-11-25 on the workstation pool:
`check:node_serves_a_specification` ran 626 s, of which `node --test` took 0.28 s (23 tests)
and the cold `cargo build --release --target wasm32-unknown-unknown -p spindle-node` about 624 s.
Without `slot --timeout 30m` the pool's 10-minute limit ends the lane.

The cost is a compile, so the fix is in how the lanes build: one wasm build shared by the lanes
that need the same artifact, a build profile for the gate that is not the release profile, or a
committed artifact with a digest of its sources, as the repository rules allow.

## Acceptance

- `check:node_serves_a_specification` ends within 120 s on an emptied `target/` in a tree the
  shared compile cache has built (D1 A), measured with `time` in the change's report beside the
  `slot status` header line of that moment, and its `node --test` tests pass unchanged.
- `check:the_static_bundle_builds` ends within 120 s the same way, and `web/dist` builds as today.
- `check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store`,
  `check:the_browser_store_keeps_state_in_indexeddb` and `check:a_service_worker_serves_a_specification`
  each end within 120 s the same way; the service-worker lane measured 120.9 s before this change.
- `check:the_wasm_lanes_share_one_cache_key` (new, POSIX sh, under 1 s): fails when a task in
  `Taskfile.yml` sets a `CARGO_*` variable in `env:`, when a wasm lane builds in a profile other than
  `wasm-gate` (lint: `wasm-lint`), or when `.cargo/config.toml` names no wasm runner; one stub case
  for each side.
- Kept as they hold since wave 25: the six lanes run under `scripts/within.sh 120` in `task check`
  and CI's `Gate`; the three `wasm-bindgen-test-runner` lanes set `WASM_BINDGEN_TEST_TIMEOUT` to 60;
  no wasm lane names `slot --timeout`.
- A new worktree's first `task check` is reported with its lane times beside the slot story that
  serves a crate built in one worktree to the next; it is not held to the bound until that story is
  closed.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `Taskfile.yml`, the six wasm lanes (`:79-142`) and their `within.sh 120` lines in `check` (`:27-32`) — cited
- **Files:** `Taskfile.yml:100`, `:108`, `:116` set `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER` in the three `wasm-bindgen-test-runner` lanes; the story's "Found after wave 25" item 1 moves it to `.cargo/config.toml` as `[target.wasm32-unknown-unknown] runner = "wasm-bindgen-test-runner"` (the file holds only `[env]` today) — cited
- **Files:** `Cargo.toml:27`, the workspace `serde_json`; the three test lanes pull `wasm-bindgen-test` 0.3.80 as a wasm dev-dependency (`crates/spindle-host/Cargo.toml:38-39`, `crates/spindle-store-browser/Cargo.toml:26-27`, `crates/spindle-wasm/Cargo.toml:39-40`), whose manifest asks for `serde_json` with `features = ["alloc"]`, so `cargo test` builds `serde_json`, `schemars` and every ESS crate in a second feature set beside the `cargo build` lanes; giving the workspace `serde_json` the `alloc` feature makes it one set — inferred (the cause is read; the remedy is a reading)
- **Symbols:** tasks `wasm:host:build`, `wasm:host:lint`, `check:a_command_runs_in_wasm_on_the_memory_store`, `check:the_browser_store_keeps_state_in_indexeddb`, `check:a_service_worker_serves_a_specification`, `check:node_serves_a_specification`, `web` — cited
- **Also likely:** `scripts/wasm-cache-key.sh` and a task `check:the_wasm_lanes_share_one_cache_key` (POSIX sh, under 1 s): it fails when a task in `Taskfile.yml` sets a `CARGO_*` variable in `env:`, when a wasm lane builds in a profile other than `wasm-gate` (lint: `wasm-lint`), or when `.cargo/config.toml` names no wasm runner, with one stub case for each side, in the style of `scripts/gate-scripts.adversary-test.sh` — inferred
- **Also likely:** `Cargo.toml:35-44`, the profiles: the story's item 2 (one profile directory) already holds for the six lanes, and the lint's `wasm-lint` builds metadata only, which a build never reuses, so no change is expected — inferred
- **Not touched:** `.github/workflows/ci.yml`, because `Gate` runs `task check` (`:129-131`) and so already runs the timed lanes — cited
- **Documents:** `CHANGELOG.md`, one entry — inferred
- **Confidence:** medium. The runner variable and the dev-dependency feature are read from the tree and from the measurements recorded in the story; that the two remove every second ESS compile is measured only on the build node, not yet in this tree.
- **Would collide with:** any unit touching the wasm lanes or `check` in `Taskfile.yml`, `.cargo/config.toml`, or the `[workspace.dependencies]` block of the root `Cargo.toml`
- **Safety fact:** a runner set in `.cargo/config.toml` for `wasm32-unknown-unknown` only affects `cargo test`/`cargo run` on that target, and only the three lanes do that (`Taskfile.yml:103`, `:111`, `:119`); the Node lane runs `node --test` (`:126`). `scripts/check-ess-pin.sh:38` reads `.cargo/config.toml` one line at a time for `ESS_TOOLCHAIN`, so a new section leaves it alone. `crates/spindle-cli/tests/spindle-cli/conform.rs:619` needs `- task: check:conformance` (`Taskfile.yml:26`) kept as written. The `alloc` feature adds no entry to `Cargo.lock` — step 2, unproven

### Tests and measurements

- `check:the_wasm_lanes_share_one_cache_key`: the guard above, under 1 s.
- No new Rust test. The six lanes keep their tests; the 60 s test bound and the 120 s step bound stay as they are.
- In the change's report: `time` of each lane on an emptied `target/`, beside the `slot status` header of that moment, both run alone and in `task check` order. Expected from the node's measurements: about 105 s for the first lane, then about 16-18 s for each of the others.

## Shared files

- `Taskfile.yml` (task `check`) and `.github/workflows/ci.yml` (job `Gate`): shared with
  `story:planning-only-prs-skip-tests` and `story:test-step-within-budget`, built before this story
  on the same branch; this story uses the timer `scripts/within.sh` that
  `story:test-step-within-budget` adds.

## Found after wave 25

Measured on the build node, 2026-11-26, at 117e166fb, lane `wasm:host:build`, 93 rustc
calls: the shared sccache serves the ESS crates within one tree, one target path and one set of
`CARGO_*` variables (first build 1 min 46 s, ESS 86.67 s; again on an emptied target in the same
tree 18.15 s, ESS 11.03 s). sccache 0.18.0 keys on every `CARGO_*` variable and every `env!` value,
and `OUT_DIR` (absolute, under `target/<profile>/`) reaches serde_core, serde and thiserror, so a
new profile name or target path misses for every crate above serde. Three changes in this
repository, untested here:

1. The wasm runner in `.cargo/config.toml` (`[target.wasm32-unknown-unknown] runner = …`), not in
   the lanes' environment: a lane with `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER` shares nothing
   with one without (service worker 109 s, ESS 88.98 s; the lane before it 17 s).
2. Every lane in one profile directory; measure on an emptied directory, never a fresh profile name
   (wave 25's measurements used `wasm-gate-t1` and `-t2`, which can never hit).
3. One feature set of `serde_json` and `schemars` across the lanes: the memory-store lane compiles 7
   of 8 ESS crates again in the same target directory under `--test` (99 s, ess-conformance
   50.32 s).

With these, measured order 105 s, then 18 s, 17 s, 16 s. The first ESS compile in each new worktree
stays until slot's own story lands.

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D1 A: the bound is held on an emptied `target/` in a tree the shared cache has built; a
  new worktree's first run is reported beside the slot story.
- D2 A: the workspace `serde_json` gets the `alloc` feature.
