---
format: aep.planning-md/3
id: approval-record:runtime-observability-acceptance
kind: approval-record
status: accepted
title: Operator accepts runtime observability
relations:
- decides: epic:runtime-observability
- decides: story:observe-ingress-model
- decides: story:ingress-request-log
- decides: story:ingress-prometheus-metrics
- decides: story:ingress-observe-panel
- decides: story:external-answers-observed
- decides: story:live-topology
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T14:45:46Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Operator accepts runtime observability

## Words

The requirement, the operator, 2026-11-06, relayed by the consumer session `fake-backend` (marked
by it as condensed, not verbatim) and passed on with its date by `b10x-chef-orchestrator`:

> Spindle offers an ingress including a router that understands incoming requests and outgoing
> responses, so we should be able to have some mini Prometheus inside, or at least an endpoint
> returning metrics or logs. We want to understand what's going on inside that runtime: runtime
> metrics or runtime tracing. If enabled, it could come with a little UI that can be embedded, or
> the raw values accessed via an API on its own routing.

The live topology view was added to the epic the same day on the same route (decision D616).

The acceptance rests on, as quoted by `b10x-chef-orchestrator`: the operator's rule relayed by
`fake-backend` the same day, "it would be really great if you take these kind of decisions
yourself", and the operator's word to `b10x-chef-orchestrator` on 2026-11-06 that relays of story
acceptances from that session count as his: "So it's always fine to accept this from them."

Condition set by `b10x-chef-orchestrator`: accepted once the plan critics pass the set. Round 2
(`review-result:observability-*-r2`): acceptance, design and scope approve; parallel safety had
one finding on the epic's story table, fixed after the round.

## What it accepts

- `epic:runtime-observability` and its stories `observe-ingress-model`, `ingress-request-log`,
  `ingress-prometheus-metrics`, `ingress-observe-panel`, `external-answers-observed`,
  `live-topology`, ranked after wave 11.
