---
format: aep.planning-md/3
id: review-result:w15-u3-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on kernel-row-set-commit-conflict (wave 15)
relations:
- reviews: story:kernel-row-set-commit-conflict
revision: 1
---
approve

Adversary pass 1 on story:kernel-row-set-commit-conflict (wave 15 U3), attacking a30c44c3. Eleven
cases in `crates/spindle-kernel/tests/spindle-kernel/adversary_scan_version.rs` (commit 22c8e38f),
all green on a30c44c3; nothing found.

Could not break:
- a SQLite database written before the per-entity stream existed: the missing stream reads as
  version 0 and a stale scan is still refused;
- two SQLite handles on one file: a stale scan is refused with identity `*`;
- `spindle.written` leaking into readers: only `SqliteStore::scan` reads the log feed and it filters
  on the stream type; the telemetry counters count from the `Change`, not the log;
- a refused group moves no version (memory and SQLite); a removal and a landed group each move it;
- a row deleted between scan and commit, two row-set guards over two entities, a write of an
  unscanned entity (no false conflict), and a look-ahead re-scan within one run.

Declared gap, not a finding: set outcomes and `deletes:` stay unconditioned on a scan, as the story
says.

```findings
[]
```
