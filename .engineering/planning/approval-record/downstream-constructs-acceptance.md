---
format: aep.planning-md/3
id: approval-record:downstream-constructs-acceptance
kind: approval-record
status: accepted
title: Operator accepts the downstream kernel construct stories
relations:
- decides: story:kernel-set-outcomes
- decides: story:kernel-related-row-guards
- decides: story:kernel-related-row-values
- decides: story:kernel-row-set-guards
- decides: story:kernel-command-responses
- decides: story:kernel-secondary-effects
- decides: story:kernel-caller-attributes
- decides: story:generated-bytes-and-list-values
- decides: story:binding-mappings-across-conversions
- decides: story:binding-refusal-policies
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-05T12:00:36Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of the downstream kernel construct stories

## Words

The operator, 2026-11-05, answering "accept the 10 construct stories (commit b608ffb)?", with option A "accept all 10; they run from wave 7" recommended, B "accept only the conversion and per-refusal policy stories" and C "all stay drafts":

> A

## What it accepts

- `story:kernel-set-outcomes`, `story:kernel-related-row-guards`, `story:kernel-related-row-values`, `story:kernel-row-set-guards`, `story:kernel-command-responses`, `story:kernel-secondary-effects`, `story:kernel-caller-attributes`, `story:generated-bytes-and-list-values`, `story:binding-mappings-across-conversions` and `story:binding-refusal-policies`, each moved to active.
- They run from wave 7, under the standing acceptance of the next 10 waves (`approval-record:kernel-constructs-acceptance`).
