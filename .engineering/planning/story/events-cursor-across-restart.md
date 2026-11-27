---
format: aep.planning-md/3
id: story:events-cursor-across-restart
kind: story
status: active
title: An events cursor reads the events committed after a restart
relations:
- serves: vision:spindle
- informed_by: story:entity-event-history
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/host/range.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/events_range.rs
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:52:00Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T09:52:01Z", actor: "human:operator", revision: 9, decided_on: {"recorded":{"review_outcome":8}}}
---
## Outcome

A `GET /api/events` cursor taken before a restart reads every event committed after the restart,
on any clock. Today `range::range` (`crates/spindle-host/src/host/range.rs`) starts a cursor from
another run at position 0 and drops events by a key built on the command sequence; a restarted host
numbers its commands from 0 again, and on a fixed clock (`run --clock <at>` with `--state`) the key
of a new event equals one the cursor already passed, so the event is skipped without an error.

## Why

Found in wave 28 by the adversary of `story:entity-event-history`, whose history cursor had the same
flaw and was corrected to resume a cursor from another run by store position. `/api/events` was not
changed in that story. Present before wave 28.

## Acceptance

- `an_events_cursor_from_before_a_restart_reads_the_events_after_it`: on SQLite with a fixed clock,
  a cursor taken before a restart reads, after the restart, every event the restarted host commits,
  once each and in order.
- `a_cursor_from_another_run_reads_a_new_memory_store_from_its_start`: on a memory store with a
  fixed clock, the cursor of the first host reads every event of the second host.
- `a_head_cursor_from_before_a_restart_reads_the_events_after_it`: the same for the cursor
  `published_head` gives.
- A cursor from the same run reads as today: the existing `/api/events` tests stay green.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host`, the events cursor in `src/host/range.rs` — cited
- **Files:** `crates/spindle-host/src/host/range.rs` (`range` :261-381, the run check :281-286, `answered` :204, `Key` :49, `Mark` :59, `head` :218) — cited
- **Symbols:** `range::range`, `Cursor`, `Mark::start`, `answered`, `range::head` — cited
- **Model to reuse:** `range::history` :505-572, which reads a cursor from another run after its store position (`range.rs:513-523`, commit 3c5170d5) — cited
- **Files:** `crates/spindle-host/tests/spindle-host/events_range.rs`, the new tests go next to `a_cursor_resumes_after_a_restart` :457 — inferred, that file holds the `/api/events` cursor tests and its SQLite restart setup
- **Also likely:** `crates/spindle-host/src/host.rs` doc comments of `published_after` :1338-1346 and `published_head` :1360-1368 ("because the host was opened again, it is read from its start once" goes out of date); no code change on the D2 default — inferred
- **Not changed:** `crates/spindle-host/src/host/reader.rs` `Reader::published_after` :161 and `published_head` :191 call `range::range` and `range::head` and get the fix through them; `crates/spindle-ingress/src/console.rs` (`GET /api/events`) is unchanged — cited
- **Documents:** `CHANGELOG.md` gets one `Fixed` entry under `[Unreleased]` — inferred; `website/docs/reference/cli.md:107` already promises "the cursor ... still resumes after a restart", and the fix makes that true, so it stays as it is — cited
- **Tests to add:** `an_events_cursor_from_before_a_restart_reads_the_events_after_it` (story's acceptance: SQLite, `ManualClock::at(<fixed instant>)` on both opens, `script`, cursor, restart, one command, read: exactly its events, once each, in order); `a_cursor_from_another_run_reads_a_new_memory_store_from_its_start` (guards D2: memory, fixed clock, the cursor from the first host reads the second host's events); `a_head_cursor_from_before_a_restart_reads_the_events_after_it` (`published_head` has the same flaw through `range::head`). Each test runs under the 60 s limit and the test step stays under 120 s. Every test in this file loads the console fixture with `spindle_spec::load`, so the implementor times each new test and names any over 5 s. The existing tests in `events_range.rs` and `postgres.rs:190` must stay green — inferred
- **Confidence:** high — the story names the file and the function; the flaw was walked through the tree's code
- **Would collide with:** any unit touching the cursor in `spindle-host/src/host/range.rs` or `tests/spindle-host/events_range.rs`; `CHANGELOG.md` `[Unreleased]` (every unit of the wave); the doc comments of `published_after`/`published_head` in `spindle-host/src/host.rs`, a file wave 29 also changes (escalating bindings)
- **Safety fact:** within one run, command sequences only grow (`following`, `host.rs:2381`), so key order matches each store's order. The change is limited to a cursor whose `run` differs from the host's (`range.rs:284`), so a cursor from the same run takes today's path — step 3 (walked `range` for the same-run case and the fixed-clock restart case), unproven
- **Design to carry over (D2 default):** a cursor from another run reads each store after its mark's position, as `history` does. It skips by key only inside the write at the mark, which is read again because the cursor answered part of it. Keys of the new events are raised above the cursor's last key, and so are the marks of the stores it answers nothing from, so the next page in the same run does not skip them — inferred
- **depends_on:** none — `story:entity-event-history` (the `informed_by`) is merged in wave 28; inferred

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: an events cursor from another run resumes by store position, as the history cursor
  does, in `range.rs` only.
- D2 B: a store from another run that holds no write at the mark's position is read from its
  start.
