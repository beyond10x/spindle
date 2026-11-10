---
format: aep.planning-md/3
id: review-result:ess-review-2026-11-09
kind: review-result
status: active
title: 'ESS review of ess/: five reviewers, one lens each'
relations:
- reviews: epic:spec-states-runtime
prose_only: merged report of five sub-agent reviews; each finding is a file:line line, not a findings block
revision: 1
---
# Spindle ESS review, 2026-11-09

Spec `ess/` at 98212306, `ess/34`, `ess 0.90.0`. `ess specify validate`: `spindle v1 — 5 file(s), valid`.
Synthesis: 56 scenarios, 4 refusals (ESS-SYNTH-003), 38 notes. Five Haiku reviewers, one lens each.
V = I re-read both sides. R = reviewer said verified. I = inferred.

## Spec contradicts the code

- V high placement.yaml:37-39 RefusalCause has 3 variants; spindle-target/src/placement.rs:63 also has EnvironmentMismatch.
- V high control.rs:253-254 `derive(...).unwrap_or_default()`: an unknown component gets empty requirements and is admitted.
- V high observe.yaml:584-588 declares GET /__spindle/topology; observe.rs:64,673 answers 404 "until its story lands".
- R med placement.yaml:303-305 not-admitted text names "another environment"; code yields EnvironmentMismatch instead.
- R med placement.yaml:313 InstanceRefused has one `cause`; Placement::Refused returns every reason (placement.rs:125-128).
- R med placement.yaml:221-266 PlanInstance: control has no planning step (grep).
- R med placement.yaml:112-128 lifecycle not enforced: hub.rs:118-130 accepts any transition; control sets Exited directly (control.rs:646).
- R med placement.yaml:341-374 LaunchInstance has no launch-failure outcome; control maps every LaunchError to Exited.
- R med placement.yaml:383-386 ReportExit code/signal never filled; reap discards the Exit (control.rs:708).
- R med placement.yaml:25 seven TargetKind variants; control launches only Process (control.rs:602-608).
- R med spindle-control does not depend on spindle-kernel: admission/placement implemented twice.
- R med observe.yaml:178-182 says histogram counts "cumulative"; telemetry stores per-bucket (counters.rs:238-243, fixture rally-table-0/00.json).
- R med observe.yaml:358-361 status 100..599; recorder takes raw status, 600..999 passes and is classed served (listener.rs:319). Reachability I.
- R low observe.yaml:7-8 omits GET /__spindle/ui, which observe.rs:672 serves.
- R low reserved routes: 405 vs 404 status contract undeclared (observe.rs:670-673).
- R low MetricFamilies lists no families; the 14 live in metrics.rs:38-135 and a fixture copy.
- R low Payload `utf8_bytes <= 1024` not in generated schema or Rust newtype (producer truncates, frame.rs:146).

## Modelling

- V high deployment.yaml:173-174 OfferBundle takes `requires` as input; the summary (189-191) and AGENTS.md say it is derived.
- V high deployment.yaml:212-224 `admitted` never reads Environment.provides (52); a binding the environment does not offer is admitted.
- R high deployment.yaml:202-203 `missing` is caller input, stored as Deployment.missing (76-77).
- R high deployment.yaml:238-259 `refused` accepts decisions AdmissionDecisionInvalid defines as invalid.
- R high placement.yaml:276-287 `placed` has no predicate; retired/ceiling checks are external though both are model values (61-62, 95-96).
- R high placement.yaml:349-369 launch admission gate is external; `launched` has no predicate.
- R med deployment.yaml:213-224 exactly-once binding relies on outcome order; add `distinct:` to `admitted`.
- R med placement.yaml:126-128 `exit` from Placed contradicts the already-ended summary.
- R med observe.yaml: 7 answer structs referenced by nothing; no entities, commands or views in the domain.
- R med observe.yaml:76-95 DeployedInstance restates placement.Instance without a relation or view.
- R low placement.yaml:27-35 Resources accept negative vcpus/memory.
- R low placement.yaml:383-386 code/signal: no exactly-one rule.
- R low observe.yaml:16,52 Deployment and Binding reuse deployment-domain short names.
- R low placement.yaml:7-8 summary says "operator"; actors are Placer and Launcher.

## Not specified at all

- R high environment implementations and `prefix.*` matching (README.md:87-102, environment.rs).
- R high store reset and the world body (cli.rs:115,141-146).
- R high timer words: anchor, cadence, overlap, missed (spindle-timer/src/lib.rs:26-31).
- R high event-binding delivery: at_least_once, 5-attempt retry (spindle-channel/src/lib.rs:13-14).
- R high caller identity behind an ingress grant (surface.rs:113).
- R med decisions file shape and --answer-default.
- R med deploy control-process routes, token and Host guard.
- R med ingress and channel are not components; spindle-control is the only one (components.yaml).
- R low examples/rally/placement.toml uses `[[instance]]`; deploy reads `[[instances]]` with a format key (control/src/placement.rs:15,29).

## Testability

- R high all 4 refusals: DecideAdmission guard in states Admitted/Refused/Running/Stopped; wrong-state outcome (deployment.yaml:261-267) has no `when`, validity is prose.
- R high DeploymentState view (deployment.yaml:360-371) omits bundle, environment_id, requires; adding them clears 8 notes. Fix of the 4 refusals by this I.
- R med known failure LaunchInstance/not-admitted (placement.yaml:349-352): deployment state is `external:` prose, not a guard.
- R med conformance.toml:45 holds 15 skips by count only; a different scenario can take a slot.
- R med observe.yaml types: no scenario sends or reads them; only fixture-vs-schema checks (Taskfile.yml:152-167).
- R low any spec fix re-pins conformance-known-failures.json spec_digest.
