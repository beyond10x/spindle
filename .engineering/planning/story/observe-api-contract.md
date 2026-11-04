---
format: aep.planning-md/3
id: story:observe-api-contract
kind: story
status: active
title: The observe API contract
relations:
- serves: vision:spindle
- decomposes: epic:observe
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 4}
---
# The observe API contract

## Outcome

Spindle's own specification gains the domain `spindle.observe` for the nouns the control API returns: the deployment answer (components, bindings, instances with target, request and state), the counter catalogue, and the sample (instance id, sequence, cumulative counters, histogram buckets, recent answers, resource readings against the quota). `ess` projects their JSON Schema, Rust types and TypeScript types; the generated files are committed and `task check` fails on drift. A fixture set of answers, valid against the schema, lets the UI be built and tested before the control process exists. Design `docs/design/observe.md` § 2, § 3, § 5.

## Acceptance

- `the_observe_domain_validates`
- `the_generated_types_have_no_drift`
- `every_fixture_answer_validates_against_its_schema`
