---
format: aep.planning-md/3
id: story:rally-runs-as-three-processes
kind: story
status: active
title: Rally runs as three processes
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- serves: vision:spindle
- depends_on: story:rally-is-watchable-in-a-terminal
- depends_on: story:bindings-cross-components
- depends_on: story:deployment-target-contract
- depends_on: story:run-over-http-ingress
- depends_on: story:telemetry-transport
scope:
- confidence: cited
  path: crates/spindle-cli
- confidence: cited
  path: crates/spindle-host
- confidence: inferred
  path: examples/environments
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 6}
- {from: "proposed", to: "active", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 7}
---
# Rally runs as three processes

## Outcome

Three `b10x-spindle` processes, one per rally component, each with its own environment document
using `channel = tcp`, play a match to a winner (design § 12, M2). Each process runs one component
with its timer and its TCP channels, and takes its component, listen address and peers from the
`SPINDLE_*` variables the target contract sets (`SPINDLE_COMPONENT`, `SPINDLE_LISTEN`,
`SPINDLE_PEER_<COMPONENT>`, `crates/spindle-target/src/process.rs`), so the process launcher can
start it.

Today neither verb does this: `run` executes JSON lines from stdin and `play` runs every
component in one process with in-process channels. The per-component mode is this story's; its
form (a `--component` flag on `run`, or a verb of its own) is the implementor's and goes into the
generated CLI reference.

## Acceptance

Each is a named test with the outcome it observes.

- `a_match_between_three_processes_ends_with_a_winner` — three instances started through the
  `process` launcher on loopback ports end with `MatchState.winner` set, read from the table's
  view.
- `the_same_bundle_runs_in_one_process_with_channel_in_process` — the same `examples/rally/ess`
  directory plays to a winner under `play` with `laptop.toml`; the two environment documents
  differ only in `channel` (asserted by comparing the parsed documents).
- `a_component_process_without_its_peers_is_refused` — a process whose component has a
  cross-component binding and no `SPINDLE_PEER_<COMPONENT>` exits non-zero naming the peer.
- `a_component_process_reports_telemetry_when_given_an_endpoint` — with `SPINDLE_TELEMETRY` set
  to a Unix socket, the component process starts the sender `story:telemetry-transport` ships,
  and a collector on the socket receives frames for that component.

This story owns the per-component entry point, so it is the one that wires the telemetry sender
into it (decided after the round-1 critics, 2026-11-04).

## Scope

- `crates/spindle-host` — cited: today composes every component in one process
  (`src/host.rs`); gains the one-component mode with TCP channels.
- `crates/spindle-cli` — cited: the per-component entry point (`src/cli.rs`, `src/main.rs`) and
  the test.
- `examples/environments` — inferred: a `channel = tcp` environment for processes on one host.

Shared, not typed: `website/docs/reference/cli.md`, `crates/spindle-docs/src/status.rs` (the
"Three processes" item), `website/docs/examples/`, `README.md`, `AGENTS.md`.

## Order

`crates/spindle-cli/src/cli.rs` and `src/main.rs` are also edited by `story:replay-ess-scenarios`
(`--scenario` on `run`) and `story:run-over-http-ingress` (`run` starts the listener). The three
run in that order: replay, then ingress, then this story (`depends_on story:run-over-http-ingress`),
so each lands on the `run` entry point the one before it left.

## Depends on

- `story:rally-is-watchable-in-a-terminal`, `story:bindings-cross-components`,
  `story:deployment-target-contract` — all implemented; added by the plan review.
- `story:run-over-http-ingress` — the shared `main.rs` order above.
- `story:telemetry-transport` — the sender this story wires in.
