---
format: aep.planning-md/3
id: review-result:w21-adv-store
kind: review-result
status: active
title: Wave 21 store and read performance, adversary pass 1
relations:
- reviews: story:view-read-without-log-scan
- reviews: story:postgres-store
- reviews: story:view-related-fields
- reviews: story:metrics-observed-series
revision: 1
---
needs-revision

Adversary pass on wave 21's store and read-performance merges (a06c4ead, 9199f616, 325a687c,
2dd8ba88); cases on review/w21-adv-store, commit 98053aff.

- PostgreSQL reading handle — the reading handle shares the writer's pool, whose 32-request queue
  refuses the next request at once; 48 concurrent view reads refused 40 of 40 commits and 49,692
  reads with "event store resource budget exhausted" — crates/spindle-kernel/src/postgres.rs:258.
  Fixed in 85307944 (reads capped at database_pool - 1, waiting over the cap).

Attacked without a break: SQLite resting rows (feed position, groups across pages, refused groups,
parity with the whole-feed read, files from 0.3.0 and 0.6.0), the PostgreSQL reset gate, related
view fields, metric route labels and the scrape gauge.

```findings
- file: crates/spindle-kernel/src/postgres.rs
  line: 258
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the PostgreSQL reading handle shares the writer's pool, whose 32-request queue refuses the next request at once, so 48 concurrent view reads refuse every commit (40 of 40) and most reads with "event store resource budget exhausted"
```
