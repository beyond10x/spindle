---
format: aep.planning-md/3
id: review-result:w19-u1-adversary-3
kind: review-result
status: active
title: Wave 19 U1 adversary pass 3
relations:
- reviews: story:postgres-store
revision: 1
---
needs-revision

```
unit: wave 19 U1 story:postgres-store, d5e9b1e2 (fix pass 2), uncommitted test files only
verdict: NEEDS-CHANGE
cases: executed 307→314 (kernel, --include-ignored), red 6; CLI postgres lane 12→13, red 0
origin: introduced 5 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-kernel/tests/spindle-kernel/adversary_postgres_named.rs` and `crates/spindle-cli/tests/spindle-cli/adversary_postgres.rs`. Measured in release unless noted:

- Scan by the entity's stream: 1,000 instances 0.54 s, 10,000 6.72 s, 50,000 29.2 s (about 0.6 ms per row); the whole-log read of the same log 0.05 s, 0.41 s, 1.23 s; SQLite 0.04 s, 0.23 s, 1.33 s.
- 1,300 rows of 1 MiB: PostgreSQL by stream 32.2 s, peak +1.43 GiB; SQLite 5.4 s; a log moved from 0.10.0 (whole-log fallback) fails at 10.3 s.
- An entity holding a schema-1 `spindle.written` over a 400k-event log: scan 5.4 s and another process's commit fails at 2.0 s.
- One append group of writes: 1,000 2.26 s (SQLite 0.22 s), 5,000 14.5 s (1.44 s), 20,000 fails at the deadline (9.1 s).
- A query-string password with an unencoded `@` is printed as the host: "PostgreSQL at CRET:5432".

Suite: kernel `--include-ignored --test-threads=1` 304 passed, 10 failed (6 new, 4 pre-existing `adversary_look_ahead`); CLI `postgres` 13 passed.

Attacked and not broken: pass-2 findings 1 to 4; 2,400 random command groups scan as on SQLite; concurrent scans hold SQLite's state at the version they name; 4,000 instances in one write; fallback detection; the 0.10.0 release binary reads and writes a SQLite state directory this build wrote; 24 URL forms besides the one above print nothing.

Findings 1 and 3 depend on `state move`: U4's fix pass 2 (0f2a6b79 on unit/w19-state-move, not in this tree) rewrites schema-1 events to schema 2 and writes the mark; U4's final pass checks that.

```findings
- file: crates/spindle-kernel/src/sqlite.rs
  line: 415
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: an entity whose stream holds a schema-1 spindle.written, as state move copies every 0.7.0-0.10.0 write, is scanned from the whole log under the publication lock forever, 5.4 s at 400k events with another process's commit failing at 2 s, and 1,300 rows of 1 MiB fail every scan at the 10 s deadline
- file: crates/spindle-kernel/src/sqlite.rs
  line: 428
  category: property
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the scan by stream reads one statement per instance at about 0.6 ms each, 29.2 s at 50k instances against 1.2 s whole log and 1.3 s SQLite, and 32.2 s against SQLite's 5.4 s for 1,300 rows of 1 MiB, paid by every list view, set outcome and metrics scrape
- file: crates/spindle-kernel/src/sqlite.rs
  line: 65
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the docs say state move writes the spindle:log/unnamed-writes mark but nothing writes it, so rows written before 0.7.0 and moved into PostgreSQL are silently missing from every scan while load still finds them
- file: crates/spindle-kernel/src/postgres.rs
  line: 287
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a query-string password with an unencoded @ and no user info in the authority passes the check, and the unreachable-server message prints its rest as the host (cret:5432)
- file: crates/spindle-kernel/src/sqlite.rs
  line: 336
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: one append group of 20,000 writes (5,000 in debug) exceeds eventlog's 10 s store-operation deadline on PostgreSQL where SQLite lands it, which a set outcome over a large entity reaches
```
