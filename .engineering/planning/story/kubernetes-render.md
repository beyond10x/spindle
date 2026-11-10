---
format: aep.planning-md/3
id: story:kubernetes-render
kind: story
status: draft
title: b10x-spindle renders a placement as Kubernetes resources
tags:
- needs-timo-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:kubernetes-target
- supersedes: story:kubernetes-operator-target
- depends_on: story:spindle-image-published
- depends_on: story:ingress-bind-address
- depends_on: story:http-caller-proves-actor
- depends_on: story:run-one-component
- depends_on: story:front-ingress
- depends_on: story:postgres-store
scope:
- confidence: cited
  path: Cargo.toml
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-kubernetes
- confidence: inferred
  path: examples/placements/rally-kubernetes.toml
- confidence: cited
  path: website/docs/concepts/deployment-targets.md
revision: 3
---
# b10x-spindle renders a placement as Kubernetes resources

**Needs Timo's acceptance**: Kubernetes as a deployment target. Supersedes
`story:kubernetes-operator-target`; `epic:kubernetes-target` answers its four open questions.

## Outcome

`b10x-spindle render kubernetes <spec> --environment <file> --placement <file> --image <ref>
--out <dir>` writes, with no cluster access, one YAML file per resource:

- per instance placed on a `Kubernetes` target: a Deployment of one replica running
  `b10x-spindle run --component <component>` from the Spindle image, its settings as environment
  variables, requests and limits from the instance's `request` (a request above the target's
  `ceiling` is refused, as placement refuses it); a Service for its channel port and, where its
  component is reached over the network, its ingress port bound per `story:ingress-bind-address`;
- a PersistentVolumeClaim per instance whose `store.durable` is bound to `sqlite`; where it is
  bound to `postgres`, a reference to a Secret the operator names (`--database-secret`), and no
  database resource;
- one front ingress Deployment and Service (`story:front-ingress`), configured with how a caller
  is proven (`story:http-caller-proves-actor`);
- one ConfigMap with the specification, the environment and the placement.

An instance on another target kind is named on stderr and not rendered. Two renders of the same
inputs are byte-identical.

## Acceptance

- `rally_renders_one_workload_per_instance`: rendering `examples/placements/rally-processes.toml`
  with its target kind set to `Kubernetes` writes three component Deployments, the front ingress,
  their Services, one ConfigMap and one claim per SQLite-bound instance.
- `a_render_is_deterministic`: two renders write identical bytes.
- `a_request_above_the_ceiling_is_refused`: render exits 1 naming the instance and the resource.
- `postgres_is_referenced_not_deployed`: with `store.durable` bound to `postgres`, no rendered
  resource is a database, and each component's Deployment reads its connection from the named
  Secret.
- `nothing_reaches_a_cluster`: the render runs with no network and no cluster credentials.
- An ignored test applies the output to a local cluster with server-side dry run, run by
  `task check:kubernetes` where a cluster is reachable.

## Scope

- `crates/spindle-kubernetes/` (new: placement and plan to resource documents, no I/O beyond
  writing files).
- `crates/spindle-cli/src/cli.rs` (`Render`), `crates/spindle-cli/src/main.rs`.
- `crates/spindle-target/src/launcher.rs` (`TargetKind::Kubernetes`, read only),
  `ess/domains/placement.yaml` (read only).
- `examples/placements/` (a Kubernetes placement), `website/docs/concepts/deployment-targets.md`
  (line 35: Kubernetes no longer "no launcher").
