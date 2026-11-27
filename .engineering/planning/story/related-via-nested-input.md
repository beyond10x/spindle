---
format: aep.planning-md/3
id: story:related-via-nested-input
kind: story
status: implemented
title: A related row named through a nested input member is read
relations:
- decomposes: epic:downstream-kernel-constructs
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-spec/tests/spindle-spec/load.rs
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:52:00Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-27T09:52:00Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "active", to: "implemented", at: "2026-11-27T17:04:23Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"test_result":1,"review_outcome":9}}}
---
# A related row named through a nested input member is read

## Outcome

A `when_related:` guard whose `via:` names a member of a struct input (`via: input.wrap.shelf`) is
answered as ESS answers it. At the pinned ESS 0.99.1, ESS rejects such a guard when it checks the
specification (`ess-domain` `command/related_guard.rs:1361` → `entity_or_refusal` `:1672`,
`UndeclaredReference`), so no IR with it reaches the kernel and the kernel's flat read at
`crates/spindle-kernel/src/execute.rs:985` gives ESS's answer. A test pins that rejection; the day
ESS accepts the guard, the test fails and the kernel read is changed then.

## Why

Found in wave 18 by the implementor of `story:kernel-existing-instance`, after its adversary pass
found the same defect in `Run::existing` (`review-result:w18-u2-adversary-1`). Pre-existing:
`Run::reference` (`crates/spindle-kernel/src/execute.rs:723` at fe90c61e) reads
`ResolvedRelatedVia::Input { field }` with a flat `self.input.get(field)`. ESS reads input paths
with `Context::get`, `ess_compiler::ir::read_input`, which follows dotted paths (`ir.rs:1463`). A
related row named through a nested member therefore reads as absent in the kernel. Not measured
with a test yet.

## Acceptance

- `nested_related_via_is_rejected_at_load` (`crates/spindle-spec/tests/spindle-spec/load.rs`): a
  specification written into a temporary directory with `when_related: {via: input.<struct>.<member>}`
  is refused by `spindle_spec::load` with ESS's code `[undeclared_reference]` in its text, naming the guard.

## Scope

Derived 2026-11-27 by `story-scoper`, narrowed by decision D1 B. Every line is **cited** (read from
the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-spec` tests — inferred
- **Files:** `crates/spindle-spec/tests/spindle-spec/load.rs` (new case
  `nested_related_via_is_rejected_at_load`) — inferred
- **Not touched:** `crates/spindle-kernel/src/execute.rs:985` (`Run::reference`, the flat read of
  `ResolvedRelatedVia::Input`), which gives ESS's answer while ESS rejects the guard — cited
- **Parity premise:** ESS 0.99.1 rejects `when_related: {via: input.<struct>.<member>}` when it
  checks the specification: `ess-domain` `command/related_guard.rs:1361` → `entity_or_refusal`
  `:1672` (`UndeclaredReference`); the compiler's `related_read` (`ess-compiler` `resolve.rs:3640-3652`)
  and the interpreter's `related::held_row` (`interpret/execute/related.rs:210-224`) read only a
  top-level field. The same files at 0.99.2 — cited
- **Stale in the draft:** ESS 0.90.0 is now 0.99.1; `execute.rs:723` is `:985`; `values.rs:413` is
  `:692` and needs no change; `Run::existing` already reads through `values::read_input`
  (`execute.rs:1330`) — cited
- **Confidence:** medium — the rejection is read from ESS's source; the test proves it
- **Would collide with:** the module list of the `spindle-spec` test binary, if a module is added — inferred

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 B: no kernel change. A test pins that ESS rejects `via: input.<struct>.<member>` when the
  specification is checked; it fails once ESS accepts such a guard, which reopens this work.
