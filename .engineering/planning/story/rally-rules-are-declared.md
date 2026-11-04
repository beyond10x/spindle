---
format: aep.planning-md/3
id: story:rally-rules-are-declared
kind: story
status: draft
title: The rally specification declares its rules
relations:
- serves: vision:spindle
- decomposes: story:spec-design-review-findings
- depends_on: story:spindle-admission-rules-are-declared
scope:
- confidence: inferred
  path: docs/design/spindle.md
- confidence: cited
  path: examples/rally/ess/components.yaml
- confidence: cited
  path: examples/rally/ess/domains/east.yaml
- confidence: cited
  path: examples/rally/ess/domains/table.yaml
- confidence: cited
  path: examples/rally/ess/domains/west.yaml
revision: 4
---
## Story

As the maintainer of the rally example, I want `examples/rally/ess/` to declare the rules
`docs/design/spindle.md` § 13 states and the callers rely on, so that a ball that leaves the grid
or a winner without a won match is refused by the model, not only by the caller.

Split out of `story:spec-design-review-findings` by a plan review on 2026-11-04. It owns the nine
findings below: three warnings, six notes. At the review, synthesis of rally refused 30 scenarios
(`ESS-SYNTH-003` ×18, `ESS-SYNTH-010` ×12).

## Acceptance

A finding is closed when its check holds on `main`. Each check is satisfied by one of: a
declaration that `ess specify validate --path examples/rally/ess` accepts and a synthesised
scenario of `ess conform synthesize --path examples/rally/ess` exercises; a design statement in
`docs/design/spindle.md` § 13 for behaviour that is only in the specification; or a link in this
body to an issue on `https://github.com/beyond10x/ess/issues` recording an ESS limit. Line numbers
are those of the review's base and move as fixes land.

- `each_tick_moves_the_ball_or_the_design_says_when_it_does_not` — either every `Tick` outcome
  moves the ball one cell, or § 13 states that a serve, a paddle return and a wall bounce change
  only the heading (finding 15, warning).
- `value_changes_match_the_design` — § 13 admits the literal reset on a point and the copy of
  input values by the players, or the specification uses increments only (finding 16, note).
- `the_grid_bounds_are_invariants` — `ball_x`, `ball_y` and the paddle fields carry invariants
  for the grid bounds `crates/spindle-host/src/rally.rs` relies on; a synthesised scenario refuses
  a value outside (finding 17, warning).
- `a_winner_implies_won` — an invariant ties a non-empty `winner` to state `Won` (finding 18,
  warning).
- `paddle_ownership_is_decided` — the design says whether the table or the player owns a paddle,
  and the specification keeps one owner and at most a declared copy (finding 19, note, verdict
  NEEDS-CHANGE: a design decision first).
- `paddle_moved_is_consumed_or_dropped` — a binding consumes `PaddleMoved`, or § 13 lists it as
  published for observers, or the table stops publishing it (finding 20, note).
- `first_sight_is_stated` — § 13 states the player's first-sight rule (finding 21, note).
- `binding_delivery_is_stated` — § 13 states why every rally binding is `at_most_once` with
  `on_failure: drop`, and with it the four `ESS-SYNTH-010` delivery refusals that follow (finding
  22, note).
- `target_too_low_is_stated` — § 13 states that a match to fewer than one point is refused
  (finding 23, note).

The refusal counts above (30 at the review) are the review's record, not a promise of this story:
no check here asks for the other `ESS-SYNTH-003` or `ESS-SYNTH-010` refusals to go. Revised after
the round-1 scope critic, 2026-11-04.

## Scope

- `examples/rally/ess/domains/table.yaml` — cited (findings 15, 16, 17, 18, 23).
- `examples/rally/ess/domains/west.yaml` — cited (findings 19, 21).
- `examples/rally/ess/domains/east.yaml` — cited (finding 21 names `east.yaml:36`).
- `examples/rally/ess/components.yaml` — cited (findings 20, 22).
- `docs/design/spindle.md` — inferred: the § 13 statements.

## Order

`docs/design/spindle.md` § 13 is also edited by `story:spindle-admission-rules-are-declared`
(finding 9, whether rally in three microVMs is one deployment or three, `spindle.md:383-384`).
This story runs after it (`depends_on`), so the § 13 text it extends already says how rally is
deployed.

## Findings note

Finding 23 keeps `origin: introduced`, as the design review recorded it; the first split copied
it as `pre-existing` by mistake (round-1 design critic).

## Findings

```findings
- file: examples/rally/ess/domains/table.yaml
  line: 219
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[contradicts] spindle.md:374 says each Tick moves the ball one cell; served, west-returns, east-returns, top-wall and bottom-wall change only the heading and leave the ball in place"
- file: examples/rally/ess/domains/table.yaml
  line: 176
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[contradicts] spindle.md:380 says every value change is a guard plus increment; scoring resets ball_x and ball_y to literals and the players copy input values"
- file: examples/rally/ess/domains/table.yaml
  line: 26
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] the grid bounds the caller relies on (rally.rs:8-11, OutsideGrid at 95-99) are only in the domain summary; ball_x, ball_y and the paddle fields carry no invariant"
- file: examples/rally/ess/domains/table.yaml
  line: 49
  category: spec-gap
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "[missing] no invariant ties winner to state Won, yet the caller ends play on a non-empty winner (rally.rs:40-44, main.rs:282)"
- file: examples/rally/ess/domains/west.yaml
  line: 13
  category: spec-gap
  severity: note
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: "[unclear] spindle.md:374 gives the table both paddles and spindle.md:375 gives each player its paddle; the spec stores the paddle on the table and a copy on the player"
- file: examples/rally/ess/components.yaml
  line: 17
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] rally-table publishes PaddleMoved, which spindle.md:378 does not list and no binding consumes"
- file: examples/rally/ess/domains/west.yaml
  line: 36
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] first-sight seats the player and asks for nothing (same in east.yaml:36); spindle.md:375 has no such rule"
- file: examples/rally/ess/components.yaml
  line: 72
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: "[spec-only] all five bindings declare delivery at_most_once and on_failure drop with no design statement; the four ESS-SYNTH-010 delivery refusals follow from it"
- file: examples/rally/ess/domains/table.yaml
  line: 87
  category: spec-gap
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "[spec-only] target-too-low (target_score < 1) has no design statement; only the play caller and its test rely on it"
```
