---
format: aep.planning-md/3
id: story:environment-implementations-specified
kind: story
status: implemented
title: The environment's implementations and their matching rule are specified
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: cited
  path: ess/domains/deployment.yaml
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/system.yaml
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:50Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"review_outcome":3,"verification":1}}}
---
# The environment's implementations and their matching rule are specified

## Outcome

An environment declares, per capability, which implementation backs it, and a capability may be
matched by prefix (`prefix.*`). This is described in `README.md:87-102` and implemented in
`crates/spindle-capability/src/environment.rs:26,83`. `spindle.deployment.Environment`
(`ess/domains/deployment.yaml:40-59`) holds only `provides` and one state. `RegisterEnvironment`
(`:140-162`) records it. The implementation names and the prefix rule have no typed home (H10, `R`,
`review-result:ess-review-2026-11-09`).

The specification gives them one. It may extend `Environment` and `RegisterEnvironment` in
`deployment.yaml`, or add a domain file. It obeys `AGENTS.md`: "No technical words in a
specification", so an implementation is named by its identifier and never by the technology
behind it.

## Acceptance

- `ess specify validate --path ess` exits 0, and the full `--format json` run reports no new
  `ESS-ENTITY-019` warning.
- Each new declaration cites its source (`file:line` in the README, docs or code) in its
  `summary`. A relation or rule no source states is an `UNMAPPED:` comment, listed in the report.
- The prefix match is stated as the specification states rules (a predicate or a summary the docs
  agree with). The report says which, and why.
- If the work concludes the environment document should not be modelled in Spindle's own
  specification, a `specification` artifact records the reason with citations instead, and no
  file changes.

## Shared surfaces

Edits only `Environment` (`deployment.yaml:40-59`) and `RegisterEnvironment` (`:140-162`) in
`deployment.yaml`. `story:admission-decision-checked-by-model` owns the rest of that file. A new
domain file adds one line to `ess/system.yaml` `domains:` and `ess/ess-inputs.yaml`
`specification:`, appended at the end.
