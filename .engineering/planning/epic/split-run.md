---
format: aep.planning-md/3
id: epic:split-run
kind: epic
status: draft
title: 'A split run: each component, and the store, in its own process'
summary: run --component for any specification over tcp, one front ingress, PostgreSQL through eventlog-postgres, durable delivery and a lock on SQLite state.
relations:
- serves: vision:spindle
- informed_by: epic:deployment-targets
revision: 3
---
# A split run: each component, and the store, in its own process

## Outcome

- `b10x-spindle run --component <name>` runs one component of any specification in its own
  process, its event bindings carried over `channel = tcp`, and one front ingress serves the whole
  surface, forwarding each operation to the process that accepts it.
- `store.durable` can be bound to `postgres`: the component's state lives in a PostgreSQL database
  the operator deploys, reached through `eventlog-postgres`. Spindle ships no store server.
- What a single process keeps in memory is either made durable or stated as a limit: events
  committed before a crash are delivered after the restart; `/api/events` is read from the store;
  pending timer ticks are not persisted, as ESS's periodic profile says.
- Two processes never open one SQLite state directory: the second is refused.
- A view whose answer needs a store in another process is refused with a named refusal.

The point it proves: the components of one specification run split across deployments with no
change to the specification.

## Why

Timo, 2026-11-10, relayed by the requesting session (verbatim excerpts):

> "what about persistency? … we should have a store kind of deployment … prove the point of
> Spindle, that it's basically able to split these components across deployments."

The store, Timo, 2026-11-10, relayed by the requesting session, asked about "PostgreSQL as the
proposed store deployment":

> "Yeah, exactly. Postgres. That's the deal."

Recorded in `approval-record:e4-postgres-store-acceptance`.

## What exists at tag 0.8.0

| fact | where |
|---|---|
| `run` runs every component in one process | `crates/spindle-cli/src/cli.rs:99-100` |
| The only per-component process is `play --component`, and `play` plays the rally example only | `crates/spindle-cli/src/cli.rs:230`, `cli.rs:240-248`, `crates/spindle-cli/src/component.rs:1`, `component.rs:16` (`rally`) |
| The host itself can run one component over TCP links for any IR | `crates/spindle-host/src/host/component.rs:440` (`Host::component`), `:542` (`connect`), `:578` (`accept`), `:719` (`deliver`) |
| `deploy` launches every instance as `play --component` | `crates/spindle-cli/src/deploy.rs:56` |
| The outbox is memory; its documentation leaves crash recovery to "the producer's log", which "replays the events of every durable position that was not yet sent", and no code in `spindle-host` or `spindle-channel` replays | `crates/spindle-channel/src/outbox.rs:8-9` |
| The timer holds no durable state by ESS's periodic profile: `lifetime: host_instance`, "a new host instance is a new activation" | `crates/spindle-timer/src/lib.rs:31` |
| `/api/events` answers from a list in memory, though every committed event is already appended to the store beside its row | `crates/spindle-ingress/src/console.rs:84-86`, `crates/spindle-host/src/host.rs:431`; `crates/spindle-kernel/src/sqlite.rs:3-5` |
| The state directory is created and used with no lock | `crates/spindle-cli/src/serve.rs:193-200`, `crates/spindle-kernel/src/sqlite.rs:72-88` |
| SQLite is the only durable store; one fixed tenant | `crates/spindle-kernel/src/sqlite.rs:33`, `crates/spindle-cli/src/serve.rs:216` |
| `eventlog-postgres` exists at the pinned tag 0.9.2 with the traits the SQLite store uses (`EventStore`, `AtomicEventStore`) | `Cargo.lock` (eventlog `b10ed05`), eventlog `crates/eventlog-postgres/src/lib.rs:58,78,346`, `atomic_group.rs:463`; `crates/spindle-kernel/src/sqlite.rs:18-22` |
| A view is read from the store of the component that owns its source, all in one process | `crates/spindle-host/src/host.rs:1206-1208`, `host.rs:1719-1738` |

## Stories, in delivery order

| order | story | depends on |
|---|---|---|
| 1 | `story:postgres-store` | none (accepted, `approval-record:e4-postgres-store-acceptance`) |
| 2 | `story:state-directory-lock` | 1 (shares `crates/spindle-cli/src/serve.rs` and `main.rs`) |
| 2 | `story:events-read-from-store` | 1 (the same `Store` port, `sqlite.rs`) |
| 3 | `story:run-one-component` | none |
| 4 | `story:committed-events-survive-restart` | 3, 2 (events: the `Store` read it owns) |
| 4 | `story:front-ingress` | 3, `story:http-caller-proves-actor`; needs Timo's acceptance |
| 5 | `story:cross-component-view-refused` | 3, 4 (front ingress) |
| 5 | `story:deploy-launches-any-model` | 3 |

Across the four epics: stories 1-2 come after the metrics of `epic:runtime-observability-depth`
and before the console; stories 3-5 come after the console and before the Kubernetes renderer of
`epic:kubernetes-target`, which depends on 1, 3 and the front ingress.

`story:events-read-from-store` owns the one `Store` read of committed events
(`crates/spindle-kernel/src/store.rs`); `story:committed-events-survive-restart` reads through it.
`story:run-one-component` and `story:deploy-launches-any-model` both touch
`crates/spindle-cli/src/component.rs`.

## Acceptance

- Rally run as three `run --component` processes behind one `run --front` reaches the same
  `MatchState` as one `run` process after the same commands and ticks.
- With `store.durable` bound to `postgres`, a restarted component answers the rows it held before
  the stop.
- An event committed before a SIGKILL of its sender is delivered after the sender restarts.
- A second `run` on a held SQLite state directory exits 1.
- A view that reads another process's store answers `501` with its named refusal.
- `deploy` runs a placement of a specification other than rally.

## No new noun

A component, its bindings and the environment's bindings are declared already (`ess/` and the
`spindle-environment/1` document); a database URL and a lock file are runtime details and never
enter a specification (AGENTS.md: no technical words in a specification).

## Not in this epic

- A store server owned by Spindle, or any network store but PostgreSQL.
- Durable periodic timers: they would contradict ESS's `lifetime: host_instance`.
- Views joined across processes: refused, not served.
- High availability, several instances of one component, ownership leases with fencing
  (`docs/design/spindle.md:415`, M5).

## Needs Timo's acceptance

`story:front-ingress`: a new process in front of the components changes how a split run is
reached.
