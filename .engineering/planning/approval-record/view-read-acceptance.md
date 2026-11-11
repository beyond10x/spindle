---
format: aep.planning-md/3
id: approval-record:view-read-acceptance
kind: approval-record
status: accepted
title: The view-read cost fix accepted on the downstream report
tags:
- delegated
relations:
- decides: story:view-read-without-log-scan
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T07:41:24Z", actor: "human:timo", revision: 2}
---
# The view-read cost fix accepted on the downstream report

## Who decided

The downstream session running Spindle 0.12.1 reported on 2026-11-11 that two filtered view reads
over entities of tens of rows take about 480 ms each and asked what a view read costs and whether
0.13.0 changes it. This repository's coordinating session takes orders from downstream sessions on
the operator's standing word of 2026-11-09, and the cost is in shipped behaviour.

## What it decides

- `story:view-read-without-log-scan` moves to active and is implemented now, as a release after
  0.13.0.
