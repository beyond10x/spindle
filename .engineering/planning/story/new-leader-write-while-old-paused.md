---
format: aep.planning-md/3
id: story:new-leader-write-while-old-paused
kind: story
status: draft
title: A write to a new leader while the old one is paused never answers 501
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

While a paused former leader still holds an open database session, a write sent to the new leader
either commits or is refused with a status the docs name for a takeover in progress, never `501`
with "event store deadline exceeded during database statement or lock".

## Why

The test-time budget work of 2026-11-28 ran the PostgreSQL lane many times on a 4-CPU slice.
`a_resumed_leader_posts_no_event_the_new_leader_committed` failed once: the new leader's write got
`501` with "event store deadline exceeded during database statement or lock" while the old leader
was paused (SIGSTOP). Not reproduced since and not diagnosed: a lock the paused process holds, or
the lane's load. A `501` is the status for an unsupported feature, not for a lock wait.

## Acceptance

- A red-capable reproduction first (`aep:diagnosing`): the same test or a narrower one that fails
  at least once in 20 runs on 0.26.0 under the lane's load, or a written finding that it does not.
- If it reproduces: the cause named in the `CHANGELOG.md` entry, and the test passes 20 of 20 runs.
- A write that waits on a lock past its deadline answers `503` (retryable), not `501`.
