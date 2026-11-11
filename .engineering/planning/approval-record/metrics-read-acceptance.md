---
format: aep.planning-md/3
id: approval-record:metrics-read-acceptance
kind: approval-record
status: accepted
title: The metrics-read defect fix accepted on the downstream report
tags:
- delegated
relations:
- decides: story:metrics-read-without-scan
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T01:09:50Z", actor: "human:timo", revision: 2}
---
# The metrics-read defect fix accepted on the downstream report

## Who decided

The downstream session running Spindle 0.10.0 with `--observe` reported on 2026-11-11 that one
metrics read takes 42 s and holds other requests, and asked whether it is fixed and offered to
measure a build with a fix against the same state. This repository's coordinating session takes
orders from downstream sessions on the operator's standing word of 2026-11-09, and the defect is in
shipped behaviour.

## What it decides

- `story:metrics-read-without-scan` moves to active and is implemented now, outside the wave plan,
  as a patch release on 0.11.0.
- The acceptance text was sent to the reporting session; a correction from it revises the story.
