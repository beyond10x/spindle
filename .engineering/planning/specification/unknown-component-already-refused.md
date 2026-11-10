---
format: aep.planning-md/3
id: specification:unknown-component-already-refused
kind: specification
status: draft
title: A placement naming an undeclared component is already refused at deploy
relations:
- specifies: story:unknown-component-refused-at-deploy
- informed_by: review-result:ess-review-2026-11-09
revision: 1
---
# A placement naming an undeclared component is already refused at deploy

## Finding

`story:unknown-component-refused-at-deploy` was filed from H2 of
`review-result:ess-review-2026-11-09`. H2 said `crates/spindle-control/src/control.rs:253-254`
(`derive(...).unwrap_or_default()`) admits an unknown component with empty requirements. The review
marked it `V`, coordinator-verified, but the coordinator had read only lines 253-254.

H2 does not hold on 98212306. `Control::start` checks every entry first, at `control.rs:229-234`,
and returns `ControlError::UnknownComponent { instance, component }` before line 253 runs.
`b10x-spindle deploy` goes through the same function (`crates/spindle-cli/src/deploy.rs:45,65`).
The check dates from 4c6e88ad (2026-11-05).

## Evidence

Wave 17 unit U3 added `crates/spindle-control/tests/spindle-control/unknown_component.rs`, with two
tests. Both pass on the base: `test result: ok. 2 passed; 0 failed` (U3's first run). The whole
`spindle-control` test binary passes: `13 passed; 0 failed`. The tests were merged into `wave/17` as
a regression guard (unit commit ae37ac88).

## Not built

Nothing. No source changed: the behaviour the story asks for already exists. A cleanup that drops
the second lookup in favour of `let Some(..) else` would change no behaviour and was not made.
