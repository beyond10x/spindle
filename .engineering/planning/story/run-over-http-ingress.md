---
format: aep.planning-md/3
id: story:run-over-http-ingress
kind: story
status: active
title: Run over HTTP ingress
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:views-with-filters-and-lists
- depends_on: story:replay-ess-scenarios
scope:
- confidence: cited
  path: crates/spindle-cli
- confidence: inferred
  path: crates/spindle-ingress
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Run over HTTP ingress

## Outcome

`b10x-spindle run` with `ingress.network` bound to `http-loopback` (as in
`examples/environments/laptop.toml`) serves the components' accepted commands and views over HTTP
on 127.0.0.1, request and answer shapes from the ESS projections. A component is served this way
only where `derive` requires `ingress.network` for it (`reached_by: network`).

## Acceptance

Each is a named test with the outcome it observes.

- `a_command_posted_over_http_executes_and_answers` — a POST of an accepted command answers its
  outcome and events, and a second read shows the state moved.
- `a_refused_command_answers_with_its_declared_error` — the answer names the declared error, and
  the state is unchanged.
- `a_view_is_answered_over_http` — a GET of a list view answers every row the store holds.
- `the_listener_binds_loopback_only` — the listener's local address is `127.0.0.1`.
- `an_ingress_binding_other_than_http_loopback_is_refused` — with `ingress.network` bound to
  `http` (as in `examples/environments/microvm.toml`), `run` exits non-zero before listening and
  names the implementation it does not serve (see Open).
- `an_answer_validates_against_its_projected_schema` — command and view answers validate against
  the JSON Schema `ess generate` projects for them.

## Scope

- `crates/spindle-ingress` — inferred, new: the HTTP adapter behind the `ingress.network`
  capability, so the listener does not land in the host or CLI crate.
- `crates/spindle-cli` — cited: `run` starts the listener when the binding asks for it
  (`src/main.rs`).

Shared, not typed: `website/docs/reference/cli.md`, `crates/spindle-docs/src/status.rs`,
`README.md`, `AGENTS.md`, `Cargo.lock`.

## Order

`crates/spindle-cli/src/main.rs` (and `src/cli.rs`) is shared with `story:replay-ess-scenarios`
and `story:rally-runs-as-three-processes`. Order: replay, then this story, then three-processes.

## Depends on

- `story:views-with-filters-and-lists` — implemented.
- `story:replay-ess-scenarios` — the shared `main.rs` order above (added after the round-1
  critics, 2026-11-04).

## Open

The implementation name: `laptop.toml` binds `ingress.network` to `http-loopback`,
`microvm.toml` to `http`. A VM instance must listen on its guest address, not loopback, so `http`
needs its own rule; this story covers `http-loopback` only.
