---
format: aep.planning-md/3
id: story:served-operations-route
kind: story
status: implemented
title: A running Spindle answers which operations it serves and why not
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T20:31:12Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T20:32:22Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T23:08:11Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A running `b10x-spindle run` answers, over HTTP, which views and commands it serves and, for each
it does not, the reason, so an app can show its own implementation status without a build step.

## Why

A downstream consumer on 0.14.0 found no HTTP route listing what the running Spindle does not
serve. Only the start log (`b10x-spindle: not served: view <name> at GET <path>: unsupported: ...`)
and the CLI check (`crates/spindle-cli/src/main.rs:707-746` at tag 0.14.0) print it. On their
specification 53 views are not served (43 aggregation, 4 computed fields, 6 filters).

## Acceptance

1. A console route (name to be decided, for example `GET /api/served`) answers one row per view
   and command of the model: its name, its method and path, `served` true or false, and for one
   not served the same reason text the start log prints.
2. The answer is declared in `spindle.observe` and validates against its generated schema, as the
   other observe answers do.
3. Its rows equal the start log's `not served` lines for the same model (a test over a fixture
   with one view of each refused kind).

## Scope

- `crates/spindle-ingress/src/console.rs`, inferred
- `crates/spindle-cli/src/main.rs`, inferred
- `ess/` (spindle.observe), inferred
