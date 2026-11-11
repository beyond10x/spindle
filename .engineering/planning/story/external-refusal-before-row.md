---
format: aep.planning-md/3
id: story:external-refusal-before-row
kind: story
status: active
title: An external refusal decided before the row is run
relations:
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T17:56:30Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T17:56:30Z", actor: "human:timo", revision: 3}
---
# An external refusal decided before the row is run

## Outcome

A command whose external refusal is declared `before_row: true` asks its external cause before the
row is read, and answers the refusal as ESS's interpreter does, instead of being refused as
unsupported.

## Why

Reported by a downstream consumer on 2026-11-10: in its specification two commands use it, and it
stops 8 of 28 scenarios.

## Acceptance

- On a neutral fixture with a `before_row: true` external refusal, a request whose cause refuses
  answers the declared refusal and reads no row; one whose cause accepts runs as before; both match
  ESS 0.90.0's interpreter (or the pinned release's) under `conform`.
