---
format: aep.planning-md/3
id: epic:kubernetes-target
kind: epic
status: draft
title: Kubernetes as a deployment target, and the images it runs
summary: The release publishes the Spindle and console images with digests; a renderer writes Kubernetes resources from a placement, marks them for scraping and never deploys Prometheus.
relations:
- serves: vision:spindle
- decomposes: epic:deployment-targets
- depends_on: story:ingress-bind-address
- depends_on: story:http-caller-proves-actor
revision: 2
---
# Kubernetes as a deployment target, and the images it runs

## Outcome

- The release publishes a Spindle image and the observability console image, each multi-arch,
  each with its digest in the release.
- `b10x-spindle render kubernetes` reads a placement document and writes, per placed instance, a
  Deployment, a Service, a PersistentVolumeClaim where the instance keeps a state file, and the
  ConfigMap holding the specification, the environment and the placement. It needs no cluster
  access: the operator applies the output with their own tooling.
- The rendered workloads are marked for scraping (annotations; optionally a PodMonitor or a
  ServiceMonitor). Prometheus is never deployed, and a PostgreSQL database is referenced, never
  deployed.

It is the Kubernetes item of `epic:deployment-targets` ("4. Then Docker, Compose, Kubernetes,
browser, each its own story").

## Why

Timo, 2026-11-10, relayed by the requesting session (verbatim excerpts):

> "Prometheus is not something which is coming with Spindle … Spindle … is probably creating the
> Kubernetes resources as a deployment target."

> "some OCI thing"

## What exists at tag 0.8.0

| fact | where |
|---|---|
| The release ships 4 tarballs (x86_64 and aarch64 Linux, aarch64 and x86_64 macOS) and `SHA256SUMS`; no image | `.github/workflows/release.yml:160-169`, `release.yml:264-298` |
| No Dockerfile, Containerfile, chart or manifest in the tree | `git ls-tree -r 0.8.0` |
| `Kubernetes` is a declared target kind, with no launcher | `ess/domains/placement.yaml:23-25`, `crates/spindle-target/src/launcher.rs:15-30`, `website/docs/concepts/deployment-targets.md:35` |
| `deploy` launches every instance as `b10x-spindle play … --component`, which plays the rally example only | `crates/spindle-cli/src/deploy.rs:56`, `crates/spindle-cli/src/cli.rs:230`, `crates/spindle-cli/src/component.rs:1` |
| The HTTP ingress binds loopback only | `crates/spindle-ingress/src/listener.rs:233` |
| A command that reads its caller answers `501` unless a test switch fixes one caller | `crates/spindle-ingress/src/surface.rs:1088` (cited by `story:http-caller-proves-actor`) |

## Decision: a renderer, not an operator, first

`story:kubernetes-operator-target` (draft) asks for a Spindle operator watching a resource. This
epic renders the resources instead, and its render story supersedes that one:

- A renderer needs no cluster access, no custom resource definition and no controller to run,
  test or release; its output is reviewable text.
- The open questions of that story are answered here: the operator's own tooling applies the
  resources; one Deployment per placed instance; resource requests and limits come from the
  instance's `request` within the target's `ceiling`, and a request above the ceiling is refused
  at render as placement refuses it; Docker and Compose do not come first, since the Spindle image
  is what a cluster runs directly.
- A controller reconciling a custom resource stays possible later as one more consumer of the same
  rendering.

No new noun: the target kind, the instance, its request, its settings and the ceiling are
declared in `spindle.placement` (`ess/domains/placement.yaml`). The rendered resources are
technical output, never part of a specification.

## Stories, in delivery order

| order | story | depends on |
|---|---|---|
| 1 | `story:spindle-image-published` | `story:ingress-bind-address` |
| 2 | `story:console-image-published` | 1, `story:console-server`; needs Timo's acceptance |
| 3 | `story:kubernetes-render` | 1, `story:ingress-bind-address`, `story:http-caller-proves-actor`, `story:run-one-component`, `story:front-ingress`, `story:postgres-store`; needs Timo's acceptance |
| 4 | `story:kubernetes-scrape-marks` | 3, `story:component-process-observe` |

Across the four epics: story 1 comes first of everything; story 2 follows the console server; the
renderer (3, 4) comes last, after the split run of `epic:split-run`. The epic's own `depends_on`
edges are on G4 and G5 only; the story edges carry the rest of the order.

Stories 1 and 2 both edit `.github/workflows/release.yml`, so 2 runs after 1.

## Acceptance

- A release's `IMAGES` asset lists the Spindle image and the console image, each by digest.
- `b10x-spindle render kubernetes` on a rally placement with a `Kubernetes` target writes one
  Deployment per placed instance and one for the front ingress, with no cluster reachable.
- Every rendered Deployment that serves metrics carries the scrape annotations.
- No rendered resource is a Prometheus, a collector or a database.

## Not in this epic

- Deploying Prometheus, a collector, a database or an ingress controller.
- Talking to a cluster: no `kubectl`, no client library, no credentials.
- A Helm chart; a Docker or Compose launcher.

## Needs Timo's acceptance

`story:kubernetes-render`: Kubernetes as a target. `story:console-image-published`: a second
shipped image.
