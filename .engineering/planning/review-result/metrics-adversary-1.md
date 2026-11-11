---
format: aep.planning-md/3
id: review-result:metrics-adversary-1
kind: review-result
status: active
title: Metrics read without scan, adversary pass 1
relations:
- reviews: story:metrics-read-without-scan
revision: 1
---
needs-revision

```
unit: story:metrics-read-without-scan, 30ed87d9 on fix/metrics-read-without-scan, uncommitted test files only
verdict: INFEASIBLE (2 red cases on a state nothing in the repo reaches); CONFIRMED warning (a mutant the unit's suite misses)
cases: executed 445→449, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
```

Cases: `crates/spindle-host/tests/spindle-host/adversary_rows.rs` and `crates/spindle-kernel/tests/spindle-kernel/adversary_counts.rs`.

- red: two hosts sharing one supplied store, and two hosts on one SQLite state directory: `rows()` on the first answers 1 Loan where a scan answers 3 (supplied) and 2 (SQLite).
- green: rally on SQLite through ticks, cross-component bindings, a reset, a restart and a reset to a world, compared after each phase with a scan by a separate `SqliteStore`; SQLite `counts` of 60 entities costs about one scan (51 ms against 44 ms), and with the trait's default per-entity `counts` 2.15 s against 20.6 ms.
- probe: a state file written by the 0.10.0 release binary (18 commands) opens with counts equal to its list views.

Suite: spindle-host 91 passed, 2 failed (the red cases); ingress 14 and 40 passed; kernel 275 passed; telemetry 3, 1 and 23 passed.

Decisions by the coordinator: the second writer is out of scope (one Spindle process per state directory; the wasm surface serves no metrics), its two cases ignored with that reason and the limit documented; `adversary_counts` committed; the HELP text corrected. Fixed in 1673276a.

```findings
- file: crates/spindle-host/src/host/rows.rs
  line: 51
  category: concurrency
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: kept row counts miss writes another writer lands in the same store (shared supplied store, two hosts on one SQLite state directory), where the base scan saw them; nothing in the repo reaches Host::rows with a second writer
- file: crates/spindle-kernel/src/sqlite.rs
  line: 339
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: no unit test holds SqliteStore::counts to one feed read; dropping the override keeps the suite green and puts a per-entity full log read on every restart, which adversary_counts now catches
- file: crates/spindle-ingress/src/observe/metrics.rs
  line: 123
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the spindle_entity_rows HELP text and families.json still say the rows are read from the store when scraped, which they no longer are
```
