---
format: aep.planning-md/3
id: story:view-read-without-log-scan
kind: story
status: implemented
title: A view read costs the rows of its entity, not the whole log
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T07:41:23Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T07:41:24Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:01Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# A view read costs the rows of its entity, not the whole log

## Outcome

On a SQLite state directory, a view read (and every other `Store::scan`) answers in time
proportional to the resting rows of the entity it reads, not to the number of events in the
tenant's log.

## Why

Reported by a downstream consumer on 2026-11-11 against 0.12.1 (SQLite state, 142 MB): two filtered
view reads over entities of tens of rows take 481 ms and 477 ms on average (80 calls each, max 546
and 641 ms), while `/api/events` (901 KB) takes 16 ms. Its gate calls both per request, so the
process sat at 718m CPU and the app timed out.

Read from the code at 0.13.0: `SqliteStore::scan` (crates/spindle-kernel/src/sqlite.rs ~334)
reads the tenant's whole feed (`self.feed`, pages of `read_feed`) and keeps only the events of the
entity asked for, so every view read costs one read of the whole log. 0.13.0 runs reads beside the
writer but does not change what each costs. The PostgreSQL branch scans by the entity's stream
only where every write of the entity is schema 2, so a state directory written before it keeps
the whole-log read.

## Acceptance

1. A view read on a SQLite state directory of at least 100,000 events whose read entity holds at
   most 50 rows answers in under 20 ms in a debug build (median of 5), where 0.13.0 takes the
   whole-log time; measured before and after in the test.
2. Every `scan` answers what the whole-feed scan answered, after any drawn sequence of commands,
   resets and restarts (a property test comparing both), on a state directory written by the
   0.10.0 release binary as well.
3. Concurrent reads (`Store::reading`) keep their guarantees: a read sees a whole write.
4. Memory stays bounded by the resting rows: a state of 1,300 rows of 1 MiB does not hold its
   history in memory.
5. Opening a state directory reads the log at most once.
