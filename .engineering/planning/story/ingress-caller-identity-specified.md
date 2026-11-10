---
format: aep.planning-md/3
id: story:ingress-caller-identity-specified
kind: story
status: implemented
title: How an ingress request proves its caller is specified
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: cited
  path: ess/components.yaml
- confidence: inferred
  path: ess/domains/deployment.yaml
- confidence: inferred
  path: ess/domains/placement.yaml
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:53Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# How an ingress request proves its caller is specified

## Outcome

ESS grants commands to actors (`ess/domains/deployment.yaml:104-118`,
`ess/domains/placement.yaml:130-145`). Spindle's ingress decides which actor an HTTP request is,
and with which attributes. That logic is at `crates/spindle-ingress/src/surface.rs:113` and
`crates/spindle-cli/src/cli.rs:190-194`, and is described at `docs/design/spindle.md:402`.
`--fixed-actor` is a test switch (`website/docs/status.md:53`). No typed declaration ties a request
to an actor (H14, `R`, `review-result:ess-review-2026-11-09`). Synthesis notes: "the specification
declares actors and serves no component, so enforcing a grant is the caller's … no
`<command>/grant/denied` scenario is owed".

The specification states how a caller is established and what a request with no established caller
is answered, in the form ESS 0.90 offers (a component `serves:`/ingress declaration, actor
attributes, a refusal). It names no protocol or credential technology (`AGENTS.md`).

## Acceptance

- `ess specify validate --path ess` exits 0.
- `ess verify conform synthesize --path ess` no longer prints the "serves no component" grant note,
  or the report cites the ESS construct that is missing.
- The answers for an unestablished caller (status, error) agree with the code. The report names the
  `file:line` that answers them.
- Every unread rule is an `UNMAPPED:` comment, listed in the report.

## Shared surfaces

`ess/components.yaml`. Actor blocks in `deployment.yaml:104-118` and `placement.yaml:130-145`
change only in their `attributes`/summaries.
