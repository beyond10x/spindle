---
format: aep.planning-md/3
id: story:reset-reapplies-seed
kind: story
status: draft
title: A reset of a seeded run applies the seed again
summary: After POST /api/reset on a run started with a seed, the seed runs again before the reset answers.
relations:
- serves: vision:spindle
- informed_by: story:seed-at-start
- informed_by: story:seed-readiness
revision: 1
---
# A reset of a seeded run applies the seed again

## Outcome

After `POST /api/reset` on a run started with a seed (`run --seed <file>` or the environment's
`[seed] file`), the seed runs again before the reset answers, and the readiness signal of
`story:seed-readiness` reports it running until it ends.

## Why

A containerised consumer on 0.19.1 found that `POST /api/reset` empties the store and does not
re-apply the file given with `--seed`. Probed: 11 seeded rows went to 0, after which a command
that needs a seeded row fails and a readiness check on seeded rows goes red. The consumer
re-applies the seed itself.

0.20.0 documents that a reset does not seed again: README, the environment docs page
(`website/docs/concepts/environment-and-admission.md`) and `run --help`
(`crates/spindle-cli/src/cli.rs`). The reset is `crates/spindle-ingress/src/console.rs`, `reset`;
the seed is `crates/spindle-cli/src/seed.rs`.

## Acceptance

- `a_reset_of_a_seeded_run_applies_the_seed_again`: on a run with a seed of rows, a reset with no
  world answers only after the seed ran again, and a view over the seeded entity holds the seeded
  rows after it, as it held them after the start.
- `the_seed_counts_after_a_reset_are_those_of_the_new_run`: the end line and the counts after a
  reset are the counts of the seed run the reset started, not a sum with the start's.
- `the_readiness_signal_reports_the_seed_running_during_a_reset`: while the seed a reset started
  has entries left, the readiness signal of `story:seed-readiness` reports it running, and ended
  once it ends.
- `a_run_without_a_seed_resets_as_before`: a reset on a run without a seed answers as today.

## Decided when scoped

- How a world given in the reset's body and the seed combine: the world's rows first, then the
  seed, or the seed only for a reset with no world.
