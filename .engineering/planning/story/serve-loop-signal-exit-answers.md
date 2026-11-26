---
format: aep.planning-md/3
id: story:serve-loop-signal-exit-answers
kind: story
status: draft
title: The serve loop's signal exit takes no exchange and drops no answer
relations:
- serves: vision:spindle
- informed_by: story:reset-reapplies-seed
revision: 1
---
# The serve loop's signal exit takes no exchange and drops no answer

## Outcome

When `run` receives SIGTERM or SIGINT, the serve loop takes no further exchange, and every answer
already handed to the listener is written to its client before the process exits. Today the loop
checks the signal before a seed entry (`crates/spindle-cli/src/serve.rs:420`) but not between
`seeding.step` (`:426`) and `listener.next` (`:433`), so one more exchange is taken: a reset taken
there empties a durable `--state` and is never answered. And the signal exit returns right after an
answer is handed over (`:423`), before the listener writes it, so the client reads EOF.

Found 2026-11-26 by adversary round 2 of wave 25 U5; both red on the base 16bba3d5f, so
pre-existing.

## Acceptance

- `a_reset_taken_after_a_signal_empties_the_state_only_where_it_answers`: a reset queued behind a
  held seed entry, then SIGTERM: the run exits 143, and the reset is either answered 200 or leaves
  the state as it was.
- `a_refused_reset_taken_after_a_signal_is_answered`: the same with a reset refused 400: the client
  reads the 400.
