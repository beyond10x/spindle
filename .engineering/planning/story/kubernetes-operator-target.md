---
format: aep.planning-md/3
id: story:kubernetes-operator-target
kind: story
status: archived
title: A Kubernetes target run by a Spindle operator
relations:
- informed_by: story:deployment-target-contract
revision: 2
transitions:
- {from: "draft", to: "archived", at: "2026-11-10T00:50:29Z", actor: "human:timo", revision: 2}
---
## Outcome

An instance placed on a `Kubernetes` target runs in a cluster. A Spindle operator watches a resource
holding the launch plan and starts, stops and reports the instance, the same launcher contract the
Process, QEMU and Firecracker launchers implement.

## Why

Raised by the operator on 2026-11-10: the deployment target was meant to be operator based, and
Spindle may offer several targets. It already does in the model:
- `TargetKind` declares 7 kinds (`crates/spindle-target/src/launcher.rs`: Process, QemuMicrovm,
  Firecracker, Docker, Compose, Kubernetes, Browser);
- launchers exist for 3 of them (`website/docs/concepts/deployment-targets.md`, "no launcher exists"
  for Docker, Compose, Kubernetes and Browser).

The precedent is loom's "generic binary + declared spec + controller", which ships for agents on
Kubernetes as `b10x-loom-operator` (`docs/design/spindle.md:60`).

## Open questions

- Is "operator" a Kubernetes controller reconciling a custom resource, or a separate control
  process for every target kind?
- Is the custom resource one instance or one deployment?
- How are resource ceilings enforced: pod limits, or refused like the Process launcher?
- Does Docker or Compose come first, as the smaller step to a container target?
