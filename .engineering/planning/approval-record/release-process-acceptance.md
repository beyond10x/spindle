---
format: aep.planning-md/3
id: approval-record:release-process-acceptance
kind: approval-record
status: accepted
title: Operator asks for a spindle release process
relations:
- decides: story:release-process
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T10:36:24Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Operator asks for a spindle release process

## Words

The operator, 2026-11-06, relayed verbatim with its date by `b10x-chef-orchestrator`:

> So for spindle, I mean, I don't know, if it doesn't has a release process, give it a release process, please. We do want to release it.

## What it accepts

- `story:release-process`, delivered with wave 9; the first release is cut from `main` after the wave 9 merge. Registry publishing stays out (`publish = false`).
