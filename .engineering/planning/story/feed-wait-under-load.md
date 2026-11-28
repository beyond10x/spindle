---
format: aep.planning-md/3
id: story:feed-wait-under-load
kind: story
status: draft
title: The long-log head test passes in a loaded PostgreSQL lane
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`spindle-host postgres::the_head_reads_no_event_of_a_long_log` passes in the PostgreSQL lane while
the long-scan tests run beside it on the same database: it does not depend on the event feed
publishing within a fixed 30 s.

## Why

Found during wave 31's U5 lane runs: on a fresh PostgreSQL 17.12 cluster it failed at 32.1 s with
"the feed still withholds this store's write at position 10100 after 30s", while 13 long-scan tests
held open transactions on the same server. It took 31.1 s on the base lane, 17.5 s and 1.2 s in
other lanes, and 0.74 s alone.

## Acceptance

- The lane passes 10 runs in a row with the test and the long-scan tests run together.
- The test still fails when the head reads an event of the long log.
