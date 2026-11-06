---
format: aep.planning-md/3
id: approval-record:wave10-acceptance
kind: approval-record
status: accepted
title: Operator accepts the two consumer-blocking drafts for wave 10
relations:
- decides: story:kernel-delete-outcomes
- decides: story:refusal-payload-complete
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T09:08:51Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of the two consumer-blocking drafts for wave 10

## Words

The operator, 2026-11-06, answering whether to accept `story:kernel-delete-outcomes` and
`story:refusal-payload-complete` now so wave 10 takes them first (option A, recommended), or keep
them as drafts (B):

> A

## What it accepts

- `story:kernel-delete-outcomes` — the kernel runs outcomes that delete their subject.
- `story:refusal-payload-complete` — a refusal answers every error field its schema requires.

Both move to active; wave 10 takes them first. `story:kernel-row-set-guards` and
`story:browser-store` move to wave 11.
