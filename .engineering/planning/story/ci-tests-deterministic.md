---
format: aep.planning-md/3
id: story:ci-tests-deterministic
kind: story
status: draft
title: Main CI fails only on a defect
relations:
- serves: vision:spindle
revision: 1
---
# Main CI fails only on a defect

## Outcome

A push to `main` goes red only when the code under test is wrong. The three tests that failed
`main` CI's `Gate` with nothing wrong hold the same property through a measure that does not
depend on the runner's load or on a port coming back: a count of event-store calls, ports held
while the next ones are picked, a wait on the process state the case is about.

## Acceptance

- `spindle-kernel` `sqlite::tests::adversary_counts_sqlite_counts_every_entity_in_about_one_read_of_the_feed`
  (moved from the integration test `adversary_counts`): on a SQLite store of 60 entities of 25 rows,
  `counts` of all 60 makes as many calls of the event store as one `scan` (1), counted by the
  store's test counter `LOG_CALLS`; a scan of each entity, the trait's default, makes 60, so the
  measure catches the mutant that drops the store's `counts`. No time is compared.
- `spindle-cli` `adversary_processes2::end_from_a_player_does_not_end_the_table_before_a_winner`:
  the stand-in west binds its port before the table's and east's ports are picked and holds it,
  and the case asserts the three differ; it waits for the stand-in to say `end` (a channel from
  its thread) or for the table to stop, then for the table to stop on it, each wait a bound on a
  hang only.
- `spindle-target` `adversary_sampling::an_instance_that_ended_without_a_wait_is_not_sampled`:
  the fixture names its process id (`pid <id>`); the case waits until `/proc/<pid>/stat` shows the
  leader as a zombie, then takes one sample and asserts `None`. It no longer stops on a resident
  set of zero, which is read while the process is still exiting.
- Each changed test passes 20 of 20 runs of its one test binary (`--exact`); no threshold was
  raised and no retry added.

## Origin

`main` CI's `Gate` went red three times with no defect:

- run 45278642391 attempt 1 (merge 7906d3e7, 2026-11-23): `adversary_counts_...`, "counting 60
  entities took 1.353908ms, more than eight scans of one (129.07µs)". Wall-clock time compared on a
  shared runner.
- run 45229015662 (2026-11-23): `end_from_a_player_...`, "the stand-in west got 0 envelopes"; the
  table's console shows `rally-player-east` and `rally-player-west` both at `127.0.0.1:40917`:
  `free_addresses(2)` released its ports before the stand-in bound `127.0.0.1:0`, which handed one
  of them out again.
- run 45057106802 (2026-11-22): `an_instance_that_ended_...`, a sample `Some` with
  `memory_bytes: 0`, `cpu_seconds: 0.0`, `wall_seconds: 0.0034`: the case stopped on a resident set
  of zero, which the kernel shows once the process released its memory and before it is a zombie,
  and that sample was rightly of a running instance.

## Found, not in this change

Tests of the same kind that have not failed `main` yet, for a later story:

- `spindle-telemetry` `transport::a_missing_collector_drops_frames_and_counts_the_drops`: the
  slowest of 1,000 recordings under 5 ms (wall clock; one preemption fails it).
- `spindle-ingress` `metrics_read::a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget`:
  median under 200 ms and every read under 1 s (a latency budget measured on the runner).
- `spindle-ingress` `adversary2_caller_proof`: four parses under 5 s.
- PostgreSQL lane: `spindle-kernel` `postgres::a_scan_holds_a_commit_while_an_older_transaction_is_open`
  (under 1 s while a 1.5 s transaction is open) and `state_move` (a scan under 1.5 s while a 4 s
  gate is held; a lock in another schema under 1 s).
- The other `free_addresses` callers in `spindle-cli` (`processes`, `adversary_processes`) pick
  every port at once and bind any stand-in first; a port released by `free_addresses` can still be
  taken by another test's process before the child binds it, which only the child binding port 0
  and reporting it would close.
