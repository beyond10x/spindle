---
format: aep.planning-md/3
id: approval-record:outcome-scoped-refusal-acceptance
kind: approval-record
status: accepted
title: Operator accepts outcome-scoped refusal
relations:
- decides: story:outcome-scoped-refusal
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T13:06:15Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Operator accepts outcome-scoped refusal

## Words

The operator, 2026-11-06, relayed by the consumer session `fake-backend` and passed on with its date
by `b10x-chef-orchestrator`:

> Yeah, stop refusing calls you can answer correctly. That sounds about right

The operator told `b10x-chef-orchestrator` on 2026-11-06 that relays of story acceptances from that
session count as the operator's word: "So it's always fine to accept this from them".

## What it accepts

- `story:outcome-scoped-refusal`: a route stays served; 501 only when the unsupported outcome is chosen.
