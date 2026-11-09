---
format: aep.planning-md/3
id: story:unknown-component-refused-at-deploy
kind: story
status: active
title: A placement naming an unknown component is refused at deploy
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-control/src/control.rs
- confidence: inferred
  path: crates/spindle-control/tests/spindle-control/main.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
---
# A placement naming an unknown component is refused at deploy

## Outcome

`crates/spindle-control/src/control.rs:253-254` computes a component's requirements as
`spindle_capability::derive(&ir, Some(&entry.component)).unwrap_or_default()`. `derive`
(`crates/spindle-capability/src/derive.rs:78-81`) returns `None` when the component is not in the
specification, so a placement entry with a misspelled component gets empty requirements and is
admitted (H2, coordinator-verified, `review-result:ess-review-2026-11-09`). The specification's
`admitted` outcome (`ess/domains/deployment.yaml:212-237`) admits only a decision covering what the
component requires.

`deploy` refuses such a placement before anything is launched, naming the entry and the component.
The refusal is checked when the placement is loaded, and every other entry is unaffected.

## Acceptance

- A test in the `spindle-control` test binary deploys a placement whose entry names a component
  the specification does not declare. It is refused, the refusal names the component, and no
  instance is launched or registered. The test is red on the base.
- A placement whose components all exist deploys as before. Existing `spindle-control` tests pass.
- `cargo clippy -p spindle-control --all-targets -- -D warnings` exits 0.
