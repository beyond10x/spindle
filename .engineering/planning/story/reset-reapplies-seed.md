---
format: aep.planning-md/3
id: story:reset-reapplies-seed
kind: story
status: active
title: A reset of a seeded run applies the seed again
summary: After POST /api/reset on a run started with a seed, the seed runs again before the reset answers.
relations:
- serves: vision:spindle
- informed_by: story:seed-at-start
- informed_by: story:seed-readiness
scope:
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 29
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 26, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:43Z", actor: "human:operator", revision: 27, decided_on: {"recorded":{"review_outcome":19}}}
---
# A reset of a seeded run applies the seed again

## Outcome

After `POST /api/reset` with no world on a run started with a seed (`run --seed <file>` or the
environment's `[seed] file`), the seed runs again before the reset answers. A reset whose body
gives a world loads that world and does not run the seed.

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
- `the_seed_counts_after_a_reset_are_those_of_the_new_run`: the end line a reset's seed run prints
  counts that run alone, not a sum with the start's. The metric `spindle_seed_commands_total` is a
  counter and keeps counting across resets: no series falls.
- `a_reset_with_a_world_does_not_seed`: on a run with a seed, a reset whose body gives a world
  answers with that world's rows and none of the seed's, also when the start's seed has entries
  left: a reset drops the entries of an earlier seed run that were not sent.
- `a_run_without_a_seed_resets_as_before`: a reset on a run without a seed answers as today.
- A SIGTERM while a reset's seed runs ends the run between two entries, as it does during the
  start's seed.

## Decided when scoped

- How a world given in the reset's body and the seed combine: the seed runs only for a reset with no
  world; a reset with a world loads that world alone (decided 2026-11-26 when scoped for wave 25).
- Where the reset waits for the seed: in the CLI's serve loop. It takes a reset exchange through
  `Exchange::console()`, `body()` and `respond()` (`crates/spindle-ingress/src/listener.rs:61`,
  `:94`, `:99`, already public), runs `console::reset`, builds `Seeding` again and steps it to its
  end, then answers. `Recorder::count_reset` (`crates/spindle-ingress/src/observe.rs:711`) becomes
  public so the CLI keeps counting resets.

## Not covered

- The readiness signal during a reset's seed run: `story:seed-readiness`, a draft, provides the
  signal; this story makes no readiness claim.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` (seed and serve loop) — cited
- **Files:** `crates/spindle-cli/src/seed.rs` (`Seeding`, :196-290) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:396-424` (the seed is built once at start and stepped in the serve loop; nothing builds it again after a reset) — inferred
- **Files:** `crates/spindle-ingress/src/console.rs:318` (`reset`) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:311` (`--seed` help: "does not submit it again") — cited
- **Symbols:** `console::reset`, `Seeding::new`, `Seeding::step`, `Seeding::pending` — cited
- **Files:** `crates/spindle-ingress/src/observe.rs:711` (`Recorder::count_reset` becomes public so the CLI keeps counting resets); `crates/spindle-ingress/src/listener.rs` and `router.rs` are read, not changed — cited
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/seed.rs` (acceptance tests next to the existing seed tests) — inferred
- **Documents:** `README.md:64`, `website/docs/concepts/environment-and-admission.md:234-236` — cited; `crates/spindle-docs/src/status.rs:432` ("a reset drops what the seed wrote") and its generated `website/data/status.json`, `website/docs/status.md`, plus `website/docs/reference/cli.md:134` generated from `cli.rs` — inferred; `CHANGELOG.md` — inferred
- **Not touched:** `crates/spindle-host` (`Host::reset_to` stays as it is when the seed is re-run from the CLI) — inferred; `crates/spindle-node` (it has no seed) — cited
- **Confidence:** medium — the reset waits for the seed in the CLI serve loop (§ Decided when scoped)
- **Would collide with:** any unit touching `spindle-cli`'s serve loop or seed module, `spindle-ingress`'s `console.rs` reset or `observe.rs` `Recorder`, the `--seed` help in `cli.rs`, or the status row "Seed at start"

## Shared files

- `crates/spindle-cli/src/cli.rs` (the `--seed` help at `:311`) and the generated
  `website/docs/reference/cli.md`: shared with `story:decisions-rules-on-input-predicates`;
  separate hunks, merged by the coordinator, the page generated again after both.
- `crates/spindle-docs/src/status.rs:432` and its generated `website/data/status.json` and
  `website/docs/status.md`: this story's alone; `story:test-step-within-budget` renames no test
  they name.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story of wave 25, merged by the coordinator.
