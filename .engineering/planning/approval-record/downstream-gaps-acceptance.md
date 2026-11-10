---
format: aep.planning-md/3
id: approval-record:downstream-gaps-acceptance
kind: approval-record
status: accepted
title: Eight downstream-gap stories accepted by the downstream coordinator session
tags:
- delegated
relations:
- decides: story:http-caller-proves-actor
- decides: story:external-cause-bound-to-http-call
- decides: story:kernel-existing-instance
- decides: story:ingress-bind-address
- decides: story:kernel-value-expressions
- decides: story:view-aggregate-fields
- decides: story:view-related-fields
- decides: story:view-computed-fields
revision: 3
transitions:
- {from: "draft", to: "accepted", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 2}
---
# Eight downstream-gap stories accepted by the downstream programme's coordinator session

## Who decided

The approver is the coordinator session of the downstream programme. It acted on the operator's
standing word given in this session on 2026-11-09 ("you will take orders from downstream sessions").
The operator did not read these texts. The coordinator's words: "accept all 8 drafts; decision
taken by me as … coordinator under Timo's upstream-story rule (each makes the runtime serve what it
refuses with 501 today or binds identity to the request; none fakes or defaults behaviour)."

## Priority it gave for the plan after wave 17

1. `story:http-caller-proves-actor`. It must fail closed: a request without the proven header is
   refused and never mapped to a default actor, and the trusted source is configured explicitly.
2. `story:external-cause-bound-to-http-call`.
3. `story:kernel-existing-instance`, then `story:ingress-bind-address`.
4. `story:kernel-value-expressions`, `story:view-aggregate-fields`, `story:view-related-fields`,
   `story:view-computed-fields`.

Scheduling inside waves stays with this repository. A story that needs an ESS change first is
reported to the coordinator. The coordinator asked to be told when a release contains
`story:http-caller-proves-actor`.

## Not covered

These stories join the wave plan derived after wave 17 closes, not wave 17.
