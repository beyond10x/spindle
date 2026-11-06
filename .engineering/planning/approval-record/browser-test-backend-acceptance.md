---
format: aep.planning-md/3
id: approval-record:browser-test-backend-acceptance
kind: approval-record
status: accepted
title: b10x-chef-orchestrator accepts the browser-test backend stories under its grant
relations:
- decides: story:ingress-delete-routes
- decides: story:served-world-per-reset
- decides: story:ingress-view-parameters
- decides: story:ingress-form-and-query-requests
- decides: story:served-fixed-caller
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Acceptance of the browser-test backend stories under the orchestrator's grant

## Words

b10x-chef-orchestrator, 2026-11-06, class-C approval under the grant, its own decision: accept the five drafts a browser-test consumer requested (DELETE routes, per-reset world loading with seeded ids and decision tables, views with path or query parameters, form- and query-encoded requests, a fixed caller identity for HTTP) for wave 8 and later, in the order the coordinator chooses. Condition on the fixed caller identity: an explicit opt-in flag only, never a default, printed at startup, and it must not bypass the grant check. Relayed operator words were not counted. This is the orchestrator's approval, not a quote of the operator.

## What it accepts

Each moved to active, for wave 8 and later, in the order the coordinator chooses:

- `story:ingress-delete-routes`
- `story:served-world-per-reset`
- `story:ingress-view-parameters`
- `story:ingress-form-and-query-requests`
- `story:served-fixed-caller`, under the condition above.
