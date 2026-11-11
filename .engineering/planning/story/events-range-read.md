---
format: aep.planning-md/3
id: story:events-range-read
kind: story
status: implemented
title: /api/events answers a range after a cursor
relations:
- serves: vision:spindle
- depends_on: story:events-read-from-store
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T13:02:04Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T13:02:04Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:05Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

`GET /api/events?after=<cursor>&limit=<n>` answers at most `n` events committed after the cursor,
with the cursor of the last one, so a poller reads only what is new and an idle poll costs a small
answer whatever the log's size.

## Why

Reported by a downstream consumer on 2026-11-11 against 0.13.0: `GET /api/events` ignores every
query (`after`, `limit`, `since`, `cursor`, `offset` and others answer the same 942 bytes and all 5
events), and the request log records no query. A proxy filtering events per caller reads, parses
and filters the whole log on every poll: 331 to 960 KB per answer, 575 MB in 34 minutes from one or
two browser tabs, and the endpoint fails for everyone at the proxy's 32 MB buffer.
`story:events-read-from-store` (wave 21) reads events from each store after a position, which this
read builds on.

## Acceptance

1. `?after=<cursor>&limit=<n>` answers the events after the cursor, at most `n`, in the order
   `/api/events` lists them, and a `next` cursor; the cursor is opaque and stable across a restart.
2. `limit` has a documented default and maximum; a larger `limit` is answered with the maximum, a
   malformed `after` or `limit` with 400.
3. With the cursor at the end, the answer is the empty list and the same cursor, and its cost does
   not grow with the log (a test on 10,000 events states the bound).
4. Without a query the answer is as before.
5. The request log records the query string of a console request (bounded as the path is).
6. `website/docs` states the parameters, the default and the maximum.

## Scope

- `crates/spindle-ingress/src/console.rs`, `crates/spindle-ingress/src/router.rs`, inferred
- `crates/spindle-host/src/host/reader.rs`, inferred
- `crates/spindle-ingress/src/observe.rs` (request log query), inferred
