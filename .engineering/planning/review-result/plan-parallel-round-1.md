---
format: aep.planning-md/3
id: review-result:plan-parallel-round-1
kind: review-result
status: active
title: Plan critic (parallel), round 1
relations:
- reviews: story:spindle-admission-rules-are-declared
- reviews: story:spindle-placement-rules-are-declared
- reviews: story:rally-rules-are-declared
- reviews: story:telemetry-transport
- reviews: story:control-process-serves-the-api
- reviews: story:replay-ess-scenarios
- reviews: story:run-over-http-ingress
- reviews: story:rally-runs-as-three-processes
- reviews: story:rally-runs-in-three-microvms
- reviews: story:spindle-conforms-to-its-specification
- reviews: story:observe-ui
revision: 1
---
needs-revision
story:telemetry-transport — shares `crates/spindle-cli` with story:replay-ess-scenarios, which edits the `run` flags in `src/cli.rs` and `src/main.rs`, and the sender's start is left open between CLI and `spindle-host`, which replay also reads (surface inferred). Both also name `status.rs`, README.md, AGENTS.md and `Cargo.lock` as shared, and neither body names the other; the body must name an ordering edge recording the shared file, or split the surface by deciding the sender starts outside `main.rs` — .engineering/planning/story/telemetry-transport.md:74-77, .engineering/planning/story/replay-ess-scenarios.md:51-57
story:replay-ess-scenarios — its `--scenario` flag on `run` lands in `src/cli.rs` and `src/main.rs` (cited), the same files story:rally-runs-as-three-processes (per-component entry point, possibly a `--component` flag on `run`; also `spindle-host`) and story:run-over-http-ingress (`run` starts the listener) edit (cited). None of the three bodies names the others, and `aep plan artifact waves` puts them in different waves with no recorded reason; the remedy is an ordering edge naming `crates/spindle-cli/src/main.rs` as the reason, or splitting the `run` entry point — .engineering/planning/story/replay-ess-scenarios.md:51, .engineering/planning/story/rally-runs-as-three-processes.md:55, .engineering/planning/story/run-over-http-ingress.md:48
story:rally-runs-in-three-microvms — it edits the guest init in `crates/spindle-qemu` and `crates/spindle-firecracker` (`src/bin/spindle-guest-init.rs`), which story:telemetry-transport also edits (the vsock device and the guest init), and no `depends_on` or body mention records it (cited, both bodies); the remedy is an ordering edge naming the guest-init files, or splitting the telemetry wiring from the per-component launch — .engineering/planning/story/rally-runs-in-three-microvms.md:52, .engineering/planning/story/telemetry-transport.md:71-73
story:spindle-conforms-to-its-specification — `the_suite_counts_are_held` and `the_synthesis_refusals_are_listed` read the synthesis output of `ess/`, which story:spindle-admission-rules-are-declared and story:spindle-placement-rules-are-declared change. The placement story removes the `ESS-SYNTH-001` refusal and both add declarations and scenarios, and the only `depends_on` edge goes to story:replay-ess-scenarios. The missing edges run to the two stories that rewrite `ess/domains/deployment.yaml` and `ess/domains/placement.yaml`, or the two must be merged — .engineering/planning/story/spindle-conforms-to-its-specification.md:38-40, .engineering/planning/story/spindle-placement-rules-are-declared.md:36
story:spindle-placement-rules-are-declared — its acceptance `an_instance_of_a_refused_deployment_cannot_be_placed` and the refusal cause `admission` read `Deployment` state and `DeploymentRefused`. story:spindle-admission-rules-are-declared reshapes those (refused carries capabilities; one or three deployments for three environments), and `placement.yaml` references `spindle.deployment.*` types, so one `ess specify validate --path ess` runs over both. No edge or body mention orders them (surface inferred from `ess/domains/placement.yaml:56,84,97,215`) — .engineering/planning/story/spindle-placement-rules-are-declared.md:36-37, .engineering/planning/story/spindle-admission-rules-are-declared.md:36-37
story:spindle-admission-rules-are-declared — its finding 9 design statement targets `spindle.md:383-384`, which is the § 13 rally text story:rally-rules-are-declared edits (its findings 15, 19 and 21 cite `spindle.md:374-380`). Both list `docs/design/spindle.md` as an inferred surface and neither names the other; the file is shared by two items worked at once (inferred) — .engineering/planning/story/spindle-admission-rules-are-declared.md:55, .engineering/planning/story/rally-rules-are-declared.md:73

**What you read:** 10 artifacts via `aep plan artifact show` on each, plus `aep plan artifact waves`, `relations`, `graph` and `crates/spindle-cli/src/cli.rs` and `main.rs`. Surfaces: 5 cited (`telemetry-transport` partly, `control-process-serves-the-api` partly, `rally-runs-as-three-processes`, `rally-runs-in-three-microvms`, `run-over-http-ingress`), 5 inferred (the three rules stories on `docs/design/spindle.md`, `replay-ess-scenarios` on host and `Cargo.toml`, `spindle-conforms-to-its-specification` on `crates/spindle-conformance`), 0 unplaceable. Acceptance reads traced to a producer in the set: 3, of which 1 is recorded by an edge (control-process to telemetry-transport, run-over-http-ingress and rally-runs-as-three-processes; conforms to replay-ess-scenarios on the executor).

**Could not establish:**
- Where telemetry-transport's sender starts (CLI or host) is left to the implementor, so its `spindle-host` collision with replay-ess-scenarios is inferred.
- Whether admission's `Deployment` reshaping touches the types `placement.yaml` references is inferred from the story bodies, not from a diff.
- Out of my lane: the ask that `Cargo.toml`, `status.rs` and README.md edits are "shared, not typed" is a design call about untyped surfaces. Whether `ess-conformance` belongs in two stories is design and scope.

```findings
- file: .engineering/planning/story/telemetry-transport.md
  line: 74
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "shares crates/spindle-cli (the run entry point in src/cli.rs and src/main.rs) with story:replay-ess-scenarios, leaves the sender's start between CLI and spindle-host (inferred surface), and both name status.rs, README.md, AGENTS.md and Cargo.lock as shared; neither body names the other and no depends_on edge records it, so name an ordering edge or split the surface"
- file: .engineering/planning/story/replay-ess-scenarios.md
  line: 51
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the --scenario flag on run lands in src/cli.rs and src/main.rs, the same files story:rally-runs-as-three-processes and story:run-over-http-ingress edit (cited); none of the three bodies names the others and no edge records it"
- file: .engineering/planning/story/rally-runs-in-three-microvms.md
  line: 52
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "edits the guest init in crates/spindle-qemu and crates/spindle-firecracker that story:telemetry-transport also edits (vsock device and guest init, cited in both bodies), with no depends_on edge or body mention"
- file: .engineering/planning/story/spindle-conforms-to-its-specification.md
  line: 38
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads the synthesis counts and refusals of ess/, which story:spindle-admission-rules-are-declared and story:spindle-placement-rules-are-declared change, and no depends_on edge runs to either; add the edges or merge the stories"
- file: .engineering/planning/story/spindle-placement-rules-are-declared.md
  line: 36
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads Deployment state and DeploymentRefused, which story:spindle-admission-rules-are-declared reshapes, and placement.yaml references spindle.deployment.* types (inferred from the story bodies); no edge or body mention orders the two"
- file: .engineering/planning/story/spindle-admission-rules-are-declared.md
  line: 55
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its finding 9 design statement targets docs/design/spindle.md:383-384, the section 13 text story:rally-rules-are-declared also edits; both list the file as an inferred surface and neither names the other (inferred)"
```
