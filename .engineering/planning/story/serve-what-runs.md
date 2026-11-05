---
format: aep.planning-md/3
id: story:serve-what-runs
kind: story
status: active
title: A run serves what it can run and answers the rest with 501
relations:
- depends_on: story:run-over-http-ingress
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-05T09:55:48Z", actor: "agent:sys-mon", revision: 6}
- {from: "proposed", to: "active", at: "2026-11-05T09:55:48Z", actor: "agent:sys-mon", revision: 7}
---
# A run serves what it can run and answers the rest with 501

## Outcome

`b10x-spindle run` with HTTP ingress no longer refuses to start because one operation uses a
construct this ingress or the kernel does not run. It serves every command and view that runs,
answers each other operation with `501` naming the construct, and prints that list at startup.
Requested by a downstream session (2026-11-05): on a 44-command specification, 21 commands use a
construct the kernel does not run, and the run stopped at the first one, a command answering with
its declared `response`.

Today the refusals sit in three places: `crates/spindle-ingress/src/surface.rs` (`SurfaceError::Unsupported`,
a route needing what the ingress does not serve), `crates/spindle-kernel/src/supported.rs`
(`check`, refusing a command before anything is read or written) and
`crates/spindle-host/src/host.rs` (`Host::new`, refusing an event binding whose `on_failure` is
not `drop`).

Rules:

- A command or view is unserved when the ingress cannot route it or the kernel's `check` refuses
  it. Each unserved operation keeps its route and answers `501` with ESS's envelope, `error` naming
  the operation and the construct. Nothing is read or written for it.
- Startup prints one line per unserved operation on stderr, then listens. A run in which no
  operation is served is still refused.
- An event binding with `on_failure: retry` stays refused by default. With `--drop-retries`, the
  run starts, prints one line per such binding saying it is not retried, and drops a failed
  invocation as a `drop` binding does. `on_failure: escalate` stays refused.
- `--state` becomes optional where `store.durable` is bound to `sqlite`: without it, the run keeps
  its state in a temporary directory removed when the run ends, as `play` does, and prints the
  path.

## Acceptance

- `an_unrunnable_command_answers_501_and_the_rest_serve` — a fixture with one runnable and one
  unrunnable command: the first answers its outcome, the second `501` naming the construct; no
  event published, no row written.
- `startup_lists_every_unserved_operation` — stderr names each one, in declaration order.
- `a_run_with_nothing_to_serve_is_refused`.
- `retry_bindings_are_refused_without_drop_retries`.
- `drop_retries_starts_and_drops_a_failed_invocation` — the binding is named at startup; a
  failing invocation is not retried.
- `sqlite_state_defaults_to_a_removed_temporary_directory`.

The fixtures are neutral; nothing from the downstream specification enters the repository.

## Scope

- `crates/spindle-ingress/src/surface.rs` — cited: `SurfaceError::Unsupported`.
- `crates/spindle-ingress` — inferred: the 501 answer.
- `crates/spindle-kernel/src/supported.rs` — cited: `check`, exposed so the host can ask before
  serving.
- `crates/spindle-host/src/host.rs` — cited: the `on_failure` refusal in `Host::new`.
- `crates/spindle-cli/src/cli.rs` — cited: `--state`, new `--drop-retries`.

Shared, not typed: `website/docs/reference/cli.md`, `crates/spindle-docs/src/status.rs`.

## Depends on

- `story:run-over-http-ingress` (implemented).
- `story:event-bindings-retry-on-failure` retires `--drop-retries` once retries run.
