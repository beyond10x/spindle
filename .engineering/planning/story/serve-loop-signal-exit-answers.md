---
format: aep.planning-md/3
id: story:serve-loop-signal-exit-answers
kind: story
status: draft
title: The serve loop's signal exit takes no exchange and drops no answer
relations:
- serves: vision:spindle
- informed_by: story:reset-reapplies-seed
scope:
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/adversary_w25_reseed_r2.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/signal_exit.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/close.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 14
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

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` serve loop — cited
- **Files:** `crates/spindle-cli/src/serve.rs:427-466` (loop: signal check `:428-432`, `seeding.step` `:433-443`, `listener.next` `:450`, reset branch `:451-463`) and `:494-516` (`reset_seeded`) — cited (story's `:420/:426/:433/:423` are this loop, shifted by later merges)
- **Files:** `crates/spindle-ingress/src/listener.rs` — `Listener` `:217`, server thread `:334-345` (JoinHandle discarded, runtime dropped on shutdown), `Drop` `:368-375`, `handed` 503 paths `:622-627` — inferred: the answer is written on that thread, so a drain that waits for handed answers lands here
- **Symbols:** `serve`, `reset_seeded`, `Seeding::finish` (`crates/spindle-cli/src/seed.rs:292`), `SIGNALLED`/`SIGNAL_POLL` (`crates/spindle-cli/src/main.rs:436-438`) — cited; new `Listener::close(self, grace)` (or similar), stop accepting, answer the queue 503, wait for handed answers, join the thread — inferred
- **Tests:** new `crates/spindle-cli/tests/spindle-cli/seed/signal_exit.rs` + `mod` line in `crates/spindle-cli/tests/spindle-cli/seed.rs:1224-1226`, using `holding_desk`/`held_desk`, `environment(.., "sqlite", ..)` with `--state` as at `seed.rs:412`: `a_reset_taken_after_a_signal_empties_the_state_only_where_it_answers`, `a_refused_reset_taken_after_a_signal_is_answered` — cited (names), inferred (place)
- **Tests:** new `crates/spindle-ingress/tests/spindle-ingress/close.rs` + `mod` in `main.rs`: `a_closed_listener_writes_every_answer_it_was_handed`, `a_request_still_queued_at_close_reads_503` — inferred; deterministic proof of the flush that the CLI tests can only race at
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/seed/adversary_w25_reseed_r2.rs:70` (`sigint_during_a_resets_seed_exits_130_between_entries`) gains an assertion on the reset's answer under D2 — inferred
- **Documents:** `crates/spindle-cli/src/cli.rs:119` (`run` help on SIGINT/SIGTERM) and the generated `website/docs/reference/cli.md:101` — inferred; `listener.rs:215` doc corrected — inferred
- **Confidence:** high for `serve.rs`/`seed.rs` (story names the site, tree read); medium for `listener.rs` (drain mechanism is a design reading)
- **Would collide with:** any unit touching `spindle-cli`'s `serve.rs` serve loop or `reset_seeded`, `seed.rs` (`Seeding::finish`/`step`), the `mod` list of `tests/spindle-cli/seed.rs`, `run` help in `cli.rs` and generated `cli.md`; and any unit touching `spindle-ingress`'s `listener.rs` (`Listener`, `Exchange`, server thread)
- **Safety fact:** `Listener` is also used by `crates/spindle-cli/src/component.rs:261` and `crates/spindle-control/src/bin/spindle-control-fixture.rs:102`, which rely on `Drop` alone; a new consuming method beside an unchanged `Drop` changes neither — step 2 (`git grep -n 'Listener::bind'`), unproven

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D1 A: a request still queued at the signal is answered 503 before exit; acceptance 2
- D2 A: a seeded reset cut by the signal answers 200 after the entry under way.
