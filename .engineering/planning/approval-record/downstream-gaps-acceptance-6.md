---
format: aep.planning-md/3
id: approval-record:downstream-gaps-acceptance-6
kind: approval-record
status: accepted
title: Three downstream-gap stories accepted by the requesting downstream session
tags:
- delegated
relations:
- decides: story:ess-format-range
- decides: story:external-refusal-before-row
- decides: story:outcome-response-headers
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T17:56:29Z", actor: "human:timo", revision: 2}
---
# Three downstream-gap stories accepted by the requesting downstream session

## Who decided

The downstream session that reported the gaps on 2026-11-10 accepted them. It acted on the
operator's standing word of 2026-11-09 ("you will take orders from downstream sessions"), and for
`story:ess-format-range` on the operator's own request made in that session the same day. Its
words: "Accepted: story:ess-format-range, story:external-refusal-before-row,
story:outcome-response-headers. Please move all three out of draft."

## What it decides

- `story:ess-format-range`: the ESS pin moves to 0.92.0 so `ess/36` runs and `ess/34`, `ess/35`
  keep passing with their fixtures; reading newer formats leniently, refusing unknown constructs by
  name, is the second half and may come later.
- Scheduling: the pin first in the wave after wave 19, since a running wave takes no new story.
- `story:external-refusal-before-row` and `story:outcome-response-headers` in normal priority order.
