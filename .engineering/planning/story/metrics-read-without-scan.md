---
format: aep.planning-md/3
id: story:metrics-read-without-scan
kind: story
status: implemented
title: A metrics read answers in milliseconds and blocks nothing
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T01:09:38Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T01:09:50Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T04:45:55Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"review_outcome":1}}}
---
# A metrics read answers in milliseconds and blocks nothing

## Outcome

`GET /__spindle/metrics` answers without scanning any store, in time proportional to the number of
series it writes, and requests to `/api`, commands and views are not held behind it.

## Why

Reported by a downstream consumer on 2026-11-11 against 0.10.0 with `--observe`: one metrics read
takes 42 s at one full core (3 reads: 42,140 ms, 42,120 ms, 41,670 ms), body 1,473,385 bytes,
15,006 sample lines, model of 504 commands, 125 `spindle_entity_rows` series, a SQLite state
directory of a few thousand rows. During the read the readiness probe (`GET /api/model`, 5 s
timeout) answered 499 38 of 47 times. With `--observe` on and no metrics read, CPU is 18-22m and
commands answer in at most 21 ms.

Cause, read from the code at 0.11.0:

- `metrics::exposition` (crates/spindle-ingress/src/observe/metrics.rs) calls `Host::rows`
  (crates/spindle-host/src/host.rs:1085), which runs `store.scan(entity)` for every entity of the
  IR. On the SQLite store a scan reads the tenant's whole feed, so one metrics read reads the whole
  log once per entity (125 times here).
- `exposition` takes `&mut Host`, so the host lock is held for the whole read, and every request
  that needs the host waits.
- `Samples::add` finds a series by a linear search, so building the samples is quadratic in the
  series of one family (5,040 `spindle_command_failures_total` series here).

## Acceptance

1. `spindle_entity_rows` comes from row counts the host keeps: taken once from a scan when the
   store opens, then changed by each committed write's rows and removals. After any sequence of
   commands, resets and restarts, every entity's count equals `store.scan(entity).rows.len()`
   (a property test over drawn command sequences, on the memory and SQLite stores).
2. A metrics read runs no store scan (a test store that counts scans sees 0 during the read).
3. A regression test with a model of at least 500 commands and 100 entities and a store of at least
   5,000 events answers a metrics read in under 200 ms in a debug build, and a command issued
   while the read runs is answered within 200 ms.
4. `Samples::add` finds a series by a map, not a linear search; the exposition's bytes are
   unchanged (an existing exposition test passes unchanged).
5. Released as a patch release.
