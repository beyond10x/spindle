---
format: aep.planning-md/3
id: story:kernel-value-expressions
kind: story
status: implemented
title: Arithmetic value expressions in outcomes are computed
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/unsupported.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:04Z", actor: "human:timo", revision: 7, decided_on: {"recorded":{"test_result":1}}}
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

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited: `{expr: …}` resolves to ESS's `ResolvedPayloadValue::Expression` (ess-compiler `ir.rs:1184`, 0.90.0), which the kernel refuses in two places
- **Files:** `crates/spindle-kernel/src/supported.rs:348` (`value`, falls to the `other =>` refusal at :403) — cited
- **Files:** `crates/spindle-kernel/src/values.rs:137` (`Sources::value`, falls to the `ExecuteError::Unsupported` arm at :255) — cited
- **Symbols:** `ResolvedPayloadValue::Expression { expr, computes, optional }`, `ess_primitives::expr::Expr::evaluate` (`expr.rs:572`), `Reads`, `Reading::of_node`, `Evaluated::to_node` — cited, public in ess-primitives 0.90.0
- **Reference behaviour:** `ess-conformance/src/interpret/execute/determined.rs:714` `expression` (`pub(super)`; its `input.`/`subject.` reading has to be reproduced) — cited
- **Also likely:** a new test in `crates/spindle-kernel/tests/spindle-kernel/`, its `mod` line in `main.rs`, a new fixture under `crates/spindle-kernel/tests/fixtures/`, compared against the interpreter as `related_values.rs:209` does; `unsupported.rs` if it asserts the refusal — inferred
- **Documents:** module doc of `crates/spindle-kernel/src/values.rs:1-9`; `docs/design/spindle.md:53` ("no arithmetic", guards only) — inferred
- **Confidence:** medium — the defect site is read from the tree, not named by the story
- **Would collide with:** units adding a value-source arm to `supported::value` or `values::Sources::value`, and units adding a test module to `crates/spindle-kernel/tests/spindle-kernel/main.rs`
- **Safety fact:** the committed value equals ESS's when the kernel calls the same `Expr::evaluate` and spells the result with `Evaluated::to_node`, reading `input.` from the invocation and `subject.` from the row before the outcome (`determined.rs:714-748`) — unproven; a timestamp plus a duration and a `half_even` decimal are where a mismatch would show
- **Not established:** how the kernel's input and `Row` fields become ESS `Node`s for `Reading::of_node`; whether `computes`/`optional` go through `typed::validate`; the downstream refusal text for its 7 outcomes; whether `spindle-capability/src/derive.rs` needs an `Expression` arm.
