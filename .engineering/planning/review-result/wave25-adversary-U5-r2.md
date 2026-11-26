---
format: aep.planning-md/3
id: review-result:wave25-adversary-U5-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 25 U5
relations:
- reviews: story:reset-reapplies-seed
revision: 1
---
approve

```
unit: story:reset-reapplies-seed, head 340cebbbc
verdict: CONFIRMED
cases: executed 19→28, red 2
origin: introduced 0 / pre-existing 2 / undecided 0
```

The correction round's code held: SIGINT exits 130 and SIGTERM 143 between entries of a reset's
seed; a plain or world reset during the start's seed cuts it; the seed counter sums every run
across four resets, one with a world, and never falls; a refused world reset leaves the seed
running; an empty seed resets and ends again. These 7 cases are kept in
`crates/spindle-cli/tests/spindle-cli/seed/adversary_w25_reseed_r2.rs`.

The 2 red cases concern the serve loop's signal exit and are red on the base 16bba3d5f too; they
go with the backlog draft story:serve-loop-signal-exit-answers.

```findings
- file: crates/spindle-cli/src/serve.rs
  line: 426
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "a signal during a seed entry is not checked before listener.next, so a reset taken afterwards empties a durable state and is never answered"
- file: crates/spindle-cli/src/serve.rs
  line: 423
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "the signal exit returns before the listener writes an answer already handed to it, so the client reads EOF"
```
