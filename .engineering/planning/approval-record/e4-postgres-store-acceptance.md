---
format: aep.planning-md/3
id: approval-record:e4-postgres-store-acceptance
kind: approval-record
status: accepted
title: Timo accepts PostgreSQL as the network store of a split run
relations:
- decides: story:postgres-store
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T00:49:50Z", actor: "human:timo", revision: 2}
---
# Timo accepts PostgreSQL as the network store of a split run

## Words

Timo, 2026-11-10, relayed through the requesting session. Asked about "PostgreSQL as the proposed
store deployment", he answered:

> "Yeah, exactly. Postgres. That's the deal."

## What it accepts

- `story:postgres-store`: `store.durable` bound to `postgres`, through `eventlog-postgres` at the
  eventlog tag Spindle pins, on a database the operator deploys.
- That there is no Spindle-owned store server, and no other network store is considered in
  `epic:split-run`.

## What it does not accept

The other architecture items of the epics drafted the same day still wait for Timo's word:
`story:front-ingress`, `story:trace-nouns-specified` (and `story:request-span-tree`,
`story:otlp-trace-export` on it), `story:console-server`, `story:console-image-published` and
`story:kubernetes-render`. It moves no story; `story:postgres-store` stays a draft until it is
proposed through its lifecycle.
