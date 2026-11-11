---
format: aep.planning-md/3
id: approval-record:downstream-gaps-acceptance-7
kind: approval-record
status: accepted
title: The seed-at-start story accepted by the requesting downstream session
tags:
- delegated
relations:
- decides: story:seed-at-start
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T23:01:12Z", actor: "human:timo", revision: 2}
---
# The seed-at-start story accepted by the requesting downstream session

## Who decided

The downstream session that asked for it accepted `story:seed-at-start` on 2026-11-11. It acted on
the operator's standing rule on upstream stories (2026-11-06) and on the operator's word of
2026-11-10 that seeding "should clearly be triggered in the background by Spindle". This
repository's coordinating session takes orders from downstream sessions on the operator's standing
word of 2026-11-09.

## What it decides

- `story:seed-at-start` moves to active: `run` applies a seed file of commands in the background at
  start, in the file's order, on every start, with counts in the metrics.
- The seed file is one JSON array; a refusal is not fatal by default.
- Scheduling: a wave after wave 19, since a running wave takes no new story. The consumer sends its
  seed through its own process until it ships.
