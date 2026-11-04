---
format: aep.planning-md/3
id: story:deployment-target-contract
kind: story
status: implemented
title: The deployment target contract
relations:
- serves: vision:spindle
- decomposes: epic:deployment-targets
scope:
- confidence: cited
  path: crates/spindle-target
- confidence: cited
  path: ess/domains/placement.yaml
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T09:17:18Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-04T09:17:18Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-04T10:52:46Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# The deployment target contract

## Outcome

`spindle.placement` (ESS, `ess/domains/placement.yaml`) specifies targets and instances. A new
crate `spindle-target` defines the `Launcher` port every target kind implements and the plan a
launcher receives: instance id, component, settings, artifact, requested resources, the addresses
of the peers its channels connect to. Placement refuses an instance whose request exceeds the
target's ceiling or whose environment does not admit the component. A `process` launcher (a child
process on the host) is the first implementation and the reference for the others.

## Acceptance

- `a_request_above_the_ceiling_is_refused_before_launch`.
- `an_instance_whose_environment_refuses_its_component_is_refused`.
- `the_process_launcher_starts_and_stops_an_instance_and_reports_its_exit`.
- `a_launcher_refuses_a_resource_it_cannot_enforce`.

## Scope

`ess/domains/placement.yaml`, `crates/spindle-target/` (new).
