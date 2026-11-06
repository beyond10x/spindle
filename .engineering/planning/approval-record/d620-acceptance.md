---
format: aep.planning-md/3
id: approval-record:d620-acceptance
kind: approval-record
status: accepted
title: Operator accepts response-field value sources and success outcome bodies (D620)
relations:
- decides: story:response-field-value-source
- decides: story:http-outcome-bodies
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T15:02:53Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Operator accepts response-field value sources and success outcome bodies (D620)

## Words

`b10x-chef-orchestrator`, 2026-11-06, decision D620: "Accept story:response-field-value-source and
story:http-outcome-bodies (D620), after your plan critics pass them." It rests on the consumer
session `fake-backend`'s request under the operator's rule of 2026-11-06 for upstream stories that
make answers more correct ("it would be really great if you take these kind of decisions
yourself"), and on the operator's word to `b10x-chef-orchestrator` the same day: "So it's always
fine to accept this from them."

Condition: the plan critics pass the set. Round 1 (`review-result:d620-*-r1`): design approved;
acceptance, parallel safety and scope asked for revisions, all fixed. Round 2
(`review-result:d620-*-r2`): acceptance and scope approve; parallel safety had two warnings and
acceptance two, all fixed after the round, no blocker.

## What it accepts

- `story:response-field-value-source` and `story:http-outcome-bodies`, ranked after wave 11 by
  their `depends_on` edges (`one-test-binary-per-crate`, `outcome-scoped-refusal`,
  `structured-response-values`).
