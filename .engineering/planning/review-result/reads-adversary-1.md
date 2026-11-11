---
format: aep.planning-md/3
id: review-result:reads-adversary-1
kind: review-result
status: active
title: Concurrent reads, adversary pass 1
relations:
- reviews: story:concurrent-reads
revision: 1
---
needs-revision

```
unit: story:concurrent-reads, 18cda67f on unit/concurrent-reads (base a82f246e), uncommitted test file only
verdict: NEEDS-CHANGE
cases: executed 92→99 (spindle-host), red 5
origin: introduced 5 / pre-existing 0 / undecided 0
```

Cases in `crates/spindle-host/tests/spindle-host/adversary_concurrent_reads.rs`:

- red A: during a reset from 5 to 300 loans, 7,794 of 29,526 `Reader::rows()` counts were neither 5 nor 300 (0, 1, 2, … 299).
- red B: during one purge removing 100 loans in one write, 225 of 83,992 counts were partial (93, 58, 9, …).
- red C: with 20,000 loans held on the memory store, a `Join` command takes 27.8 ms (trimmed mean of 40) while a thread lists an empty view, against 0.127 ms with the thread idle.
- red D: with 20,000 events published, a command takes 5.7 ms while a thread reads the events, against 0.122 ms.
- red E: after a burst of 96 concurrent view reads on SQLite the process holds 195 descriptors on the state (5 before).
- green F: 30 rounds of lend, read, reset on SQLite leave no old store file open.
- green G: after `submit` returns, a read on another thread sees the write, by view and by row count.

Suite: `cargo test -p spindle-host`: 94 passed, 5 failed, 4 ignored; without these cases 92 passed.

Held: views during multi-row writes, set outcomes, refused groups and resets show whole states on memory and SQLite; route split (only GET views, `GET /api/model`, `GET /api/events`, `GET /__spindle/metrics` and `/topology` on the reader); trusted-caller proof, `x-ess-may-read` grants and the fixed caller on the read path; proxy-secret check before the split; a long read cannot starve a write; `play --component` and wasm unchanged.

```findings
- file: crates/spindle-host/src/host/rows.rs
  line: 157
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: Counted::commit_all moves the shared row counts one write at a time under separate locks, so GET /__spindle/metrics read beside the host reports a set outcome half applied (93, 58, 9 of 100 removed).
- file: crates/spindle-host/src/host/rows.rs
  line: 81
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: replace recounts the new store into the counts readers share before publish, so during a reset to a world the reader counts 0 then each loaded row while its views still read the old stores, contrary to the Reader doc.
- file: crates/spindle-kernel/src/store.rs
  line: 538
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: Arc::make_mut on the whole Held makes any write that lands during any read, even of another empty entity, copy every row of the memory store (28 ms against 0.13 ms at 20k rows), contrary to the Reading contract that the writer never waits beyond handing over the state.
- file: crates/spindle-host/src/host.rs
  line: 1521
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: Reader::published holds the Arc while it clones the history, so each command that publishes while GET /api/events reads copies every event since the last reset (5.7 ms against 0.12 ms at 20k events).
- file: crates/spindle-kernel/src/sqlite.rs
  line: 451
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: The SQLite reading handle opens a connection per concurrent read with no cap and keeps every one open until the store drops, so a burst of 96 reads leaves 195 descriptors open, and about 330 overlapping reads exhaust a 1024 soft limit.
```
