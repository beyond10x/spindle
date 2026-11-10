---
format: aep.planning-md/3
id: story:related-via-nested-input
kind: story
status: draft
title: A related row named through a nested input member is read
relations:
- decomposes: epic:downstream-kernel-constructs
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
revision: 2
---
# A related row named through a nested input member is read

## Outcome

A `when_related:` guard whose `via:` names a member of a struct input (`via: input.wrap.shelf`)
reads the related row the member names, as ESS 0.90.0's interpreter reads it.

## Why

Found in wave 18 by the implementor of `story:kernel-existing-instance`, after its adversary pass
found the same defect in `Run::existing` (`review-result:w18-u2-adversary-1`). Pre-existing:
`Run::reference` (`crates/spindle-kernel/src/execute.rs:723` at fe90c61e) reads
`ResolvedRelatedVia::Input { field }` with a flat `self.input.get(field)`. ESS reads input paths
with `Context::get`, `ess_compiler::ir::read_input`, which follows dotted paths (`ir.rs:1463`). A
related row named through a nested member therefore reads as absent in the kernel. Not measured
with a test yet.

## Acceptance

- A kernel test on a neutral fixture with `when_related:` over `via: input.<struct>.<member>`
  answers each outcome as ESS 0.90.0's interpreter does, for a held and a missing related row.
- The test fails on the code before the change.

## Scope

- `crates/spindle-kernel/src/execute.rs` (`Run::reference`, :723) — cited
- `crates/spindle-kernel/src/values.rs` (`read_input`, :413, the dotted reader) — cited
- `crates/spindle-kernel/tests/spindle-kernel/` and a fixture — inferred
