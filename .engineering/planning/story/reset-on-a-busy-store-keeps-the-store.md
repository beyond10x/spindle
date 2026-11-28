---
format: aep.planning-md/3
id: story:reset-on-a-busy-store-keeps-the-store
kind: story
status: draft
title: A reset on a busy store answers 503 and keeps the store
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A `POST /api/reset` whose store misses its deadline (another session holds the lock it needs)
answers `503` with nothing reset and leaves the component's store as it was, so the next command
runs as before. Today the reset answers `500` and replaces the store with a lost one, and every
later command answers `501` until the run restarts.

## Why

Wave 33's U2 adversary (round 2, `story:new-leader-write-while-old-paused`) built the state with a
stub store whose writes are busy: `POST /api/reset` answered `500` "its store could not be
reopened, and every read and write of it now fails", and the next command answered `501` "store:
desk's store was lost in a reset". A reset of a supplied store (PostgreSQL) replaces the store with
`Lost` when its first removal or reset mark fails (`crates/spindle-host/src/host.rs`, reset of a
supplied store). On PostgreSQL the removal and the reset mark go through eventlog's guarded append,
which takes the publication lock shared, so a reset in the window a paused leader leaves can reach
it. Present before wave 33. The case
`adversary_w33_u2::a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store`
(spindle-ingress) holds it, ignored with this story's id.

## Acceptance

- That case passes: a reset held by a busy store answers `503` with `committed: false`, and once
  the store answers again a command is accepted.
- A reset whose store fails otherwise answers as today, and `website/docs/concepts/capabilities.md`
  names both answers.
