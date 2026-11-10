---
format: aep.planning-md/3
id: story:periodic-timing-specified
kind: story
status: implemented
title: When a periodic binding fires is specified
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-observe-api
- confidence: cited
  path: ess/domains/observe.yaml
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:51Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1,"review_outcome":3,"verification":1}}}
---
# When a periodic binding fires is specified

## Outcome

A periodic binding fires on a schedule with the words anchor, first, cadence, overlap, missed and
lifetime. These are implemented in `crates/spindle-timer/src/lib.rs:26-31` and described in
`website/docs/concepts/components-and-bindings.md:22-34`. `spindle.observe.Binding`
(`ess/domains/observe.yaml:50-74`) says only that a binding is periodic. When a tick is due and
which ticks are dropped are not typed (H12, `R`, `review-result:ess-review-2026-11-09`).

First establish where the words come from. If ESS defines them (a periodic binding's
`schedule`/`on_failure` keys), Spindle's specification references ESS's terms and does not
redefine them. In that case the outcome is that the observe answers publish them. If Spindle
defines them, the specification declares them.

## Acceptance

- `ess specify validate --path ess` exits 0.
- The report says whether each of the six words is ESS's or Spindle's, citing the ESS reference or
  the code.
- What the specification adds cites its source in its `summary`, and every unread rule is an
  `UNMAPPED:` comment.
- If observe types change, `task generate:observe` is run, and both observe contract checks pass.
- If the conclusion is that nothing belongs in Spindle's specification, a `specification`
  artifact records why, with citations, and no file changes.

## Shared surfaces

Observe changes stay within `Binding` and `BindingKind` (`observe.yaml:50-74`).
