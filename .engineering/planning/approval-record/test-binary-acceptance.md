---
format: aep.planning-md/3
id: approval-record:test-binary-acceptance
kind: approval-record
status: accepted
title: Operator accepts one integration-test binary per crate for wave 10
relations:
- decides: story:one-test-binary-per-crate
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T09:14:32Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of one integration-test binary per crate

## Words

The operator, 2026-11-06, answering whether to take `story:one-test-binary-per-crate` into wave 10
(option A, recommended) or keep it as a draft (B):

> A -> accept

## What it accepts

- `story:one-test-binary-per-crate` moves to active; wave 10 takes it beside
  `story:kernel-delete-outcomes` and `story:refusal-payload-complete`, merged last in the wave
  because it moves every crate's test files.
