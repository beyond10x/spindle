---
format: aep.planning-md/3
id: epic:deployment-targets
kind: epic
status: draft
title: Deployment targets
relations:
- serves: vision:spindle
revision: 1
---
# Deployment targets

## Outcome

One contract says where an instance of a component runs and within which limits, and every
deployment target implements it: a host process, a QEMU microVM, a Firecracker microVM, then
Docker, Docker Compose, Kubernetes and a browser. The specification never names a target; the
operator places instances on targets.

## The contract

- `spindle.placement` in `ess/` holds the nouns: a target (kind, the environment it provides, the
  most resources one instance may take there) and an instance (deployment, component, settings,
  target, resources requested), with plan → place → launch → exit.
- A `Launcher` port in a new crate `spindle-target` is what each target kind implements: prepare
  an artifact (binary, kernel and initramfs, image), launch an instance within its resources,
  connect it to the other instances' channels, observe it, stop it, report its exit.
- Resources are vCPUs, memory and disk; each launcher states which it enforces and refuses a
  request it cannot honour rather than ignoring it.

## Order (operator, 2026-11-04)

1. The contract.
2. QEMU `microvm`.
3. Firecracker.
4. Then Docker, Compose, Kubernetes, browser, each its own story.

## Not in this epic

Substrate as a launcher. It may become one binding of the same port later; it is not opened for
this.
