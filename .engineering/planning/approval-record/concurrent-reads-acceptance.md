---
format: aep.planning-md/3
id: approval-record:concurrent-reads-acceptance
kind: approval-record
status: accepted
title: Concurrent reads accepted on the operator's defect call, default on silence
tags:
- default-on-silence
relations:
- decides: story:concurrent-reads
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T04:46:07Z", actor: "human:timo", revision: 2}
---
# Concurrent reads accepted on the operator's defect call, by default on silence

## Who decided

The operator called serving one request at a time a defect on 2026-11-11 ("Seems like you have a
defect."), relayed by the downstream session that measured it. The coordinating session presented
when to do it (D1: A now, after the metrics fix; B after the PostgreSQL release; C not at all),
recommended A and stated A as the default on silence. No answer came before the metrics fix
shipped as 0.12.1, so A applies.

## What it decides

- `story:concurrent-reads` moves to active and is implemented now, before the PostgreSQL release.
- A word from the operator choosing B or C moves it back.
