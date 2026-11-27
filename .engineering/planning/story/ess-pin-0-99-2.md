---
format: aep.planning-md/3
id: story:ess-pin-0-99-2
kind: story
status: draft
title: The ESS pin moves to the release a consumer's specification requires
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

Spindle's ESS pin moves from 0.99.1 to the ESS release a consumer's specification requires
(0.99.2 or later), with the parity suite and the conformance gate green on the new pin.

## Why

Reported by a consumer on 2026-11-27: its specification requires ESS 0.99.2 while Spindle pins
0.99.1 (`Cargo.toml`). `check` and `admit` pass on 0.22.0; nothing beyond them was run, so whether
0.99.2's changes reach Spindle's runtime is not established.

## Acceptance

- The pin in `Cargo.toml` names the new release, `check:the_ess_pin_is_one_release` passes, and the
  workspace's tests and `check:conformance` are green on it.
- Every behaviour ESS changed between the two releases that Spindle runs is named in the changelog,
  each with a test or a stated refusal.
