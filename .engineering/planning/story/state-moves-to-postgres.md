---
format: aep.planning-md/3
id: story:state-moves-to-postgres
kind: story
status: active
title: A state directory's streams move into a PostgreSQL store
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:postgres-store
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T15:08:57Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T15:08:57Z", actor: "human:timo", revision: 3}
---
# A state directory's streams move into a PostgreSQL store

## Outcome

An operator moves the state an existing `--state` directory holds (one `<component>.sqlite` file
per component, `crates/spindle-host/src/host.rs:728`) into the PostgreSQL store of
`story:postgres-store`, with Spindle stopped, and the run started on PostgreSQL answers every view
as the run on the directory did.

## Why

Asked for by a downstream consumer on 2026-11-10 that runs Spindle with `--state` on a volume and
wants its state in a managed PostgreSQL before real data is entered. Both stores are eventlog
stores holding the same streams (`crates/spindle-kernel/src/sqlite.rs:1-13`: one stream per
instance, `spindle.row`, `spindle.removed`, `spindle.written` per entity), so the move is a copy of
streams, versions kept, not a translation. No command for it exists today.

## Acceptance

- After commands on rally with `--state <dir>`, the move into an empty PostgreSQL database exits 0,
  and `run` bound to `postgres` on that database answers every view as the run on `<dir>` did.
- A command decided after the move names the version the directory's last write left, and a
  second writer of the same instance is refused as on SQLite.
- A move into a database that already holds streams under the component's prefix exits 1 and
  writes nothing.
- A move while another process holds the directory (`story:state-directory-lock`) exits 1.

## Open

- Whether this is a `b10x-spindle` subcommand or an eventlog tool: not established.
