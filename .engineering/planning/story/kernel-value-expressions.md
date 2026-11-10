---
format: aep.planning-md/3
id: story:kernel-value-expressions
kind: story
status: active
title: Arithmetic value expressions in outcomes are computed
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 3}
---
# Arithmetic value expressions in outcomes are computed

## Outcome

An outcome value written as `{expr: "<expression>"}` over input and subject values (integer
addition, subtraction, multiplication and division, a duration added to a timestamp, `coalesce`,
`round`) is computed by the kernel when the outcome is selected, as ESS 0.90.0's interpreter
computes it. A downstream specification has 7 outcomes that answer 501 for this today (its count;
the refusal text was not read here).

## Acceptance

On a neutral fixture with outcomes setting `attempt: {expr: "subject.attempt + 1"}`,
`expires_at: {expr: "input.requested_at + 168h"}` and
`share: {expr: "coalesce(round(input.part * 100 / input.whole, 1, half_even), 0)"}`, `admit` reports
no unsupported construct, and each committed row equals ESS 0.90.0's interpreter for the same steps.
