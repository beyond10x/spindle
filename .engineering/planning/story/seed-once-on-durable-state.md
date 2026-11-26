---
format: aep.planning-md/3
id: story:seed-once-on-durable-state
kind: story
status: draft
title: A seed is applied once to a durable state
relations:
- serves: vision:spindle
revision: 1
---
# A seed is applied once to a durable state

## Outcome

`run --seed` on a durable `--state` applies the seed once. A restart on the same state does not send
the seed's entries again, so an entry that updates, moves or removes a row is not applied a second
time. Today the restart replays every entry: creations are counted `already-there`, but every other
entry runs again and changes the rows a second time.

Decided when drafted: the run records in the state that the seed ran, with the digest of the seed
file. A restart with the same digest skips the seed and prints one line saying so; a restart with a
different digest exits 1 before listening, naming both digests, unless the state holds no row. A
reset with no world still applies the seed again (story:reset-reapplies-seed).

Reported 2026-11-26 by a consumer that keeps its state across restarts.

## Acceptance

- `a_restart_does_not_apply_the_seed_again`: a seed that creates a row and then updates it, run on a
  state, stopped and run again on that state: the row shows one update, and the second start prints
  that the seed already ran.
- `a_changed_seed_on_a_seeded_state_stops_the_run`: a restart with an edited seed file exits 1 before
  listening, naming both digests.
- `a_reset_applies_the_seed_again_after_a_restart`: after the skipped seed, a reset with no world
  applies it from its first entry.
