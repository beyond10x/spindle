---
format: aep.planning-md/3
id: story:store-reset-and-world-specified
kind: story
status: implemented
title: A store reset and the world it seeds are specified
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-observe-api
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/system.yaml
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:51Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":1,"review_outcome":6,"verification":1}}}
---
# A store reset and the world it seeds are specified

## Outcome

A served run resets its store from a "world": rows, a decision table and a mint seed. An invalid
world is answered `400`. This is described at `crates/spindle-cli/src/cli.rs:115,141-146`,
`README.md:61` and `docs/design/observe.md:277`. `ess/` names it only as "the last reset's world"
in `AnswerSource` (`ess/domains/observe.yaml:273-280`). Reset and World have no typed home (H11,
`R`, `review-result:ess-review-2026-11-09`).

The specification gives them one, in the domain the code serves them from (`spindle.observe` or a
new domain). It covers what a world holds, what a reset does to every store, and what is refused.

## Acceptance

- `ess specify validate --path ess` exits 0.
- Every declaration cites its source in its `summary`. Every unread rule is an `UNMAPPED:` comment,
  listed in the report.
- The route, status codes and body shape agree with the code that serves the reset. The report
  names the handler `file:line`.
- If observe types are touched, `task check:the_generated_types_have_no_drift` and
  `task check:every_fixture_answer_validates_against_its_schema` pass after `task generate:observe`.

## Shared surfaces

A new domain file adds one line to `ess/system.yaml` and `ess/ess-inputs.yaml`, appended at the
end. Edits to `observe.yaml` stay in the `AnswerSource` and new declarations at the end of the
file.
