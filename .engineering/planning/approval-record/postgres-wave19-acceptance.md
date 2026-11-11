---
format: aep.planning-md/3
id: approval-record:postgres-wave19-acceptance
kind: approval-record
status: accepted
title: Timo accepts the PostgreSQL store and the state move for wave 19
relations:
- decides: story:postgres-store
- decides: story:state-moves-to-postgres
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T15:08:56Z", actor: "human:timo", revision: 2}
---
# Timo accepts the PostgreSQL store and the state move for wave 19

## Words

Timo, 2026-11-10, in the coordinating session, asked whether he accepts `story:postgres-store`
and `story:state-moves-to-postgres` for wave 19, starting right after the 0.10.0 release. He chose:

> "A: both, wave 19 (Recommended)"

The option read: "Accept both stories. Wave 19 starts after 0.10.0 and carries them first,
released as 0.11.0. Existing SQLite data can then be moved."

The same day he wrote in two other sessions, relayed to this one: "ship it fast, spindle boy." and,
asked which session runs the store work, "session should run it".

## What it accepts

- `story:postgres-store`, with its Hosted database section: TLS with a CA bundle, a migrate
  subcommand under a DDL-capable role, a DML-only serving role, a connection budget.
- `story:state-moves-to-postgres`.
- Both go first in wave 19, released as 0.11.0.

## Not covered

`story:state-directory-lock` is not accepted by these words; it stays a draft.
