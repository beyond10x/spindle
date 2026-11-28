---
format: aep.planning-md/3
id: story:postgres-lane-margin-ci
kind: story
status: draft
title: The PostgreSQL lane keeps a quarter of its bound free on CI
relations:
- serves: vision:spindle
revision: 4
---
## Outcome

The PostgreSQL lane's parallel part on the CI runner ends within 75 s of its 100 s bound, with no
test over 15 s there, so a slower runner or one more lane test does not turn the release gate red.

## Why

PR 109 already delivered the 15 s per-test check (`check:postgres` fails naming each lane test over
15 s, `scripts/each-test-within.sh`) and eleven cheaper tests. The CI lane still ran 89.8 s of its
100 s bound on PR 109 and 98.5 s on PR 108. A step over its budget is a defect; this one is under it
by 1.5 to 10.2 s.

Diagnosis, 2026-11-28, with eventlog 0.10.0 (`crates/spindle-kernel/Cargo.toml:22-24`) and the
local lane pinned to 4 CPUs:

- Each lane run makes 1,459 store opens, about 7.1 per test. Each open runs eventlog's
  `schema::migrate_with_blob_migration`.
- That migration takes 185 s of the 294 s of summed statement time.
  - Creating the `eventlog_expected_*` temp tables takes 75.4 s, dropping them 11.6 s.
  - The catalog comparison takes 56.6 s.
- A probe patch that skips the shape check for a current schema brought server CPU from 151-171
  CPU-s to about 60 CPU-s, and the local lane from 63-74 s to about 45 s.
- Ruled out: `fsync` and WAL settings (no gain), and a template database per run (at most 27
  CPU-s).

So the cut lands in eventlog, not in Spindle's tests: this story left wave 35.

## Acceptance

- The CI run of this change's PR: the PostgreSQL job's `within:` line is at most 75 s of 100 s, and
  its ten slowest tests each take at most 15 s.
- Locally, three runs of `slot --timeout 30m --tokens 4 task check:postgres` (pinned to 4 CPUs)
  each pass and end their parallel part within 60 s.
- `check:postgres` fails, locally and on CI, when any lane test took over 15 s, naming each such
  test with its time, so a slow new test goes red on its own PR (operator's decision D-LM = A,
  2026-11-28).
- Each changed test is shown red against a deliberately broken build (not committed), and its
  before and after times are in its commit body.
- No bound, thread count or timeout is raised, no test is ignored, and no shard is added.

## Waits on

An eventlog release where opening a current schema costs no DDL: no `eventlog_expected_*` temp
tables and no catalog comparison when the schema is already current. The story was sent to the
eventlog owner on 2026-11-28. When it is released, this story pins that release in
`crates/spindle-kernel/Cargo.toml` and checks the CI lane against 75 s.
