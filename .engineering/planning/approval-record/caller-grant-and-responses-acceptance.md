---
format: aep.planning-md/3
id: approval-record:caller-grant-and-responses-acceptance
kind: approval-record
status: accepted
title: Orchestrator accepts fixed-caller-per-grant and structured-response-values
relations:
- decides: story:fixed-caller-per-grant
- decides: story:structured-response-values
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T10:10:59Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Orchestrator acceptance of fixed-caller-per-grant and structured-response-values

## Words

`b10x-chef-orchestrator`, 2026-11-06, under its class-C grant (wave scope), not relaying the
operator:

> Accept story:fixed-caller-per-grant and story:structured-response-values (draft -> proposed ->
> active); record this in your store as the acceptance. This is my decision under the class-C grant
> in ~/beyond10x/CLAUDE.md (wave scope), based on the story bodies in spindle-wave9:
> fixed-caller-per-grant keeps the fixed caller an explicit opt-in test-backend flag, refuses an
> operation no fixed actor may run with 403, and sets no default, which matches my earlier rule that
> it never bypasses grants; structured-response-values follows ESS 0.87.0's interpreter and still
> refuses by name the types it does not choose. [...] Put them into your next wave.

## What it accepts

- `story:fixed-caller-per-grant` — repeatable `--fixed-actor`, each operation sent as the first
  fixed actor granted it; no default; an operation no fixed actor may run answers 403.
- `story:structured-response-values` — record, union, map, list and constrained-newtype response
  values as ESS 0.87.0's interpreter chooses them; types it does not choose stay refused by name.

Both move to active for wave 10.
