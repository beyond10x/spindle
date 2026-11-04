---
format: aep.planning-md/3
id: approval-record:six-drafts-acceptance
kind: approval-record
status: accepted
title: Operator accepts six draft stories
relations:
- decides: story:rally-runs-as-three-processes
- decides: story:rally-runs-in-three-microvms
- decides: story:spindle-conforms-to-its-specification
- decides: story:spindle-admission-rules-are-declared
- decides: story:spindle-placement-rules-are-declared
- decides: story:rally-rules-are-declared
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-04T21:33:53Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of six draft stories

## Words

The operator, 2026-11-04, answering "may I accept the 6 draft stories (rally as three processes, rally in three microVMs, spindle conforms to its spec, and the admission, placement and rally rules stories)?", with option A "you accept all 6; I record it, plan waves 3 to 6, and land the parked spec hardening through the rules stories" recommended and B "they stay drafts":

> A

## What it accepts

`story:rally-runs-as-three-processes`, `story:rally-runs-in-three-microvms`, `story:spindle-conforms-to-its-specification`, `story:spindle-admission-rules-are-declared`, `story:spindle-placement-rules-are-declared` and `story:rally-rules-are-declared`, each moved to active.
