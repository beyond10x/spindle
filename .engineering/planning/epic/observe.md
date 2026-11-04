---
format: aep.planning-md/3
id: epic:observe
kind: epic
status: active
title: Observe a running deployment
relations:
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
---
# Observe

## Outcome

While a deployment runs, `http://127.0.0.1:<port>/` shows every component, the bindings between them, and per instance what it executes, sends, writes and how much of its CPU and memory quota it uses. Design: `docs/design/observe.md`.
