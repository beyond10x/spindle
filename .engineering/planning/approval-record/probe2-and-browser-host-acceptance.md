---
format: aep.planning-md/3
id: approval-record:probe2-and-browser-host-acceptance
kind: approval-record
status: accepted
title: Operator accepts the second probe stories and the browser host stories
relations:
- decides: story:event-bindings-retry-on-failure
- decides: story:generated-boolean-values
- decides: story:console-api
- decides: story:wasm-host-builds
- decides: story:browser-store
- decides: story:http-router-is-transport-free
- decides: story:service-worker-serves-a-specification
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-05T09:39:08Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of the second probe stories and the browser host stories

## Words

The operator, 2026-11-05, answering "accept the 7 draft stories (retry-on-failure, generated Boolean, console API, the 4 browser-host stories)?", with option A "accept all 7" recommended, B "accept only retry, generated Boolean and console API" and C "all 7 stay drafts":

> A

## What it accepts

- `story:event-bindings-retry-on-failure`, `story:generated-boolean-values`, `story:console-api`, `story:wasm-host-builds`, `story:browser-store`, `story:http-router-is-transport-free` and `story:service-worker-serves-a-specification`, each moved to active.
- They join the planned waves under the standing acceptance of the next 10 waves (`approval-record:kernel-constructs-acceptance`).
