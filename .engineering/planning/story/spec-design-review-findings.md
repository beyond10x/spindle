---
format: aep.planning-md/3
id: story:spec-design-review-findings
kind: story
status: draft
title: Close the gaps the spec design review found
relations:
- serves: vision:spindle
revision: 3
---
## Story

As the maintainer of spindle's specifications, I want the gaps a design review found between
docs/design/spindle.md and the ESS sources of spindle and rally closed, so that what the design
states is declared and checked.

## Source

An ess:hardening design review (technique 8) of `ess/` and `examples/rally/ess/` against
`docs/design/spindle.md`, 2026-11-04: 23 findings, 10 warning, 13 note, 0 blocker. Validation at
the time: spindle 51 scenarios, 1 refusal; rally 28 scenarios, 30 refusals (ESS-SYNTH-003 x18,
ESS-SYNTH-010 x12).

## Split

A plan review on 2026-11-04 split the findings by area into three stories that decompose this
one. Each finding lives in exactly one of them, in its `findings` block, with a named check:

| story | area | findings | warning / note |
|---|---|---|---|
| `story:spindle-admission-rules-are-declared` | `ess/domains/deployment.yaml` | 6 | 4 / 2 |
| `story:spindle-placement-rules-are-declared` | `ess/domains/placement.yaml`, `ess/system.yaml` | 8 | 3 / 5 |
| `story:rally-rules-are-declared` | `examples/rally/ess/` | 9 | 3 / 6 |

One finding already holds on `main` at `b9b0cd9`: `ess/system.yaml` declares `spindle.observe`
(owned by the placement story).

## Acceptance

- `every_child_story_is_implemented` — the three stories above are implemented; this umbrella
  carries no check of its own.
