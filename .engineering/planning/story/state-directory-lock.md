---
format: aep.planning-md/3
id: story:state-directory-lock
kind: story
status: active
title: A second process on the same SQLite state is refused
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:postgres-store
scope:
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/src/state_lock.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:40:09Z", actor: "human:timo", revision: 5}
- {from: "proposed", to: "active", at: "2026-11-11T09:40:09Z", actor: "human:timo", revision: 6}
---
# A second run on the same SQLite state is refused

## Outcome

`run --state <path>` takes an exclusive lock on a lock file beside its state (in the directory
where `run` serves HTTP, beside the file where it reads stdin) before opening any store, and holds
it until it exits. A second `run` given the same state exits 1 before opening anything, naming the
path and the process id that holds it. Today the directory is created and used with no lock
(`crates/spindle-cli/src/serve.rs:193-200`, `crates/spindle-kernel/src/sqlite.rs:72-88`), so two
runs on one directory each keep their own in-memory outbox and event list over one database.
Component processes are not in this story.

## Acceptance

- `a_second_run_on_the_same_state_exits_1`: with one `run` serving on a state directory, a second
  `run --state` on it exits 1.
- `the_refusal_names_the_holder`: the second run's error names the directory and the first
  process's id.
- `the_holder_keeps_serving`: after the refusal, the first run still answers requests.
- `the_lock_is_released_at_exit`: after the first run exits (SIGTERM), a new run on the directory
  starts.
- `a_killed_holder_does_not_block`: after the first run is killed with SIGKILL, a new run starts,
  since the operating system released the lock.

## Scope

- `crates/spindle-cli/src/serve.rs` (lines 193-200), `crates/spindle-cli/src/main.rs` (the stdin
  path, lines 521-544).
- `crates/spindle-cli/src/state_lock.rs` (new: an advisory file lock).
- `crates/spindle-cli/tests/`.

## Shared files

`story:postgres-store` edits `crates/spindle-cli/src/serve.rs` and `crates/spindle-cli/src/main.rs`
too; this story runs after it.
