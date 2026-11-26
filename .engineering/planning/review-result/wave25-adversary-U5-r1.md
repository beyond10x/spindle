---
format: aep.planning-md/3
id: review-result:wave25-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U5
relations:
- reviews: story:reset-reapplies-seed
revision: 1
---
needs-revision

```
unit: story:reset-reapplies-seed, head 17d8ee8c3
verdict: NEEDS-CHANGE
cases: executed 17→20, red 3
origin: introduced 2 / pre-existing 0 / undecided 1
```

Cases in `crates/spindle-cli/tests/spindle-cli/seed/adversary_w25_reseed.rs`, all red on 17d8ee8c3:

- `sigterm_during_a_resets_seed_stops_the_run_between_entries`: SIGTERM during a reset's seed did
  not end the run; the desk was asked for the next entry.
- `a_reset_with_a_world_during_the_start_seed_loads_that_world_alone`: the view held
  `["auditor", "guest"]` where the world alone holds `["guest"]`.
- `no_seed_count_falls_across_a_reset`: `spindle_seed_commands_total` went from `[3, 3, 0, 0]` to
  `[1, 1, 0, 0]` during the reset's seed.

Attacked and not broken: two resets in a row; the request-log class of the reset's answer; a
malformed or failing reset does not seed; a whitespace body counts as no world, `{}` as an empty
world; a refused seed entry is counted in its run; `spindle_resets_total` counts the reset once.

```findings
- file: crates/spindle-cli/src/seed.rs
  line: 254
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "Seeding::finish runs the whole reset seed without checking for a signal"
- file: README.md
  line: 99
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a reset sets spindle_seed_commands_total back to zero, a counter that falls"
- file: crates/spindle-cli/src/serve.rs
  line: 425
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: undecided
  message: "a reset with a world during the start's seed gets the remaining start entries written into its world"
```
