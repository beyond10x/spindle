---
format: aep.planning-md/3
id: story:kernel-existing-instance
kind: story
status: active
title: 'An existing_instance: outcome is executed'
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: crates/spindle-kernel/src/caller.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/existing-instance
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/existing_instance.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:35Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:35Z", actor: "human:timo", revision: 3}
---
# An `existing_instance:` outcome is executed

## Outcome

A command outcome declared `existing_instance: true` (the instance the input names already exists,
so the outcome answers its declared error) is selected by the kernel as ESS selects it. Today the
kernel refuses the command as unsupported (`crates/spindle-kernel/src/supported.rs:222`:
`ResolvedCondition::ExistingInstance => Some("`existing_instance:`")`), so a creating command that
declares the duplicate case is not served at all. A downstream specification has 13 such commands.

## Acceptance

On a neutral fixture with `AddItem` (outcome `added` creates `Item` with `instance: id`, outcome
`duplicate` with `existing_instance: true` and error `ItemExists`), `admit` lists no unsupported
construct for `AddItem`, a first `AddItem` creates the row, a second with the same id answers
`ItemExists` and writes nothing, and both answers equal ESS 0.90.0's interpreter.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:222` — cited, the `ResolvedCondition::ExistingInstance => Some("existing_instance:")` refusal arm in `condition`
- **Files:** `crates/spindle-kernel/src/execute.rs` — inferred, `Run::select` (around :481) has the `Phase::RelatedRow` (:496/:501) and `Phase::Existence` (:502) arms where ESS 0.90.0 places the `existing_instance:` branch (`ess-domain` `precedence::place`); it needs an `existing` step mirroring `ess-conformance` `interpret/execute/existence.rs::existing`. `Run::truth` (around :645) probably needs an `ExistingInstance` marker arm
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/existing_instance.rs` (new), `crates/spindle-kernel/tests/spindle-kernel/main.rs` (module line), `crates/spindle-kernel/tests/fixtures/existing-instance/` (new) — inferred, following `creation_takes_unknown_row.rs`
- **Symbols:** `ResolvedCondition::ExistingInstance`, `supported::condition` — cited; `Run::select`, `Run::truth`, `PrecedencePlan`, `Phase::Existence`, `Phase::RelatedRow` — inferred
- **Also likely:** `crates/spindle-kernel/src/caller.rs:148` — inferred, already lists `ExistingInstance` as no-guard; probably unchanged
- **Documents:** none — inferred
- **Confidence:** high — the story cites the refusal site, and `execute.rs::select` is the only executor that would see the condition after admission
- **Would collide with:** units touching `spindle-kernel`'s precedence selection in `src/execute.rs` (`Run::select`, the Existence/RelatedRow phases, `creation_takes_absent`, `unknown_instance`) or the condition table in `src/supported.rs`
- **Safety fact:** `execute.rs:236` reaches `supported::selects(spec)` before selection, so no `ExistingInstance` branch reaches `select` today; lifting the refusal at `supported.rs:222` changes only commands that declare `existing_instance:`, none of which were served — unproven
- **Not established:** how `existing` fits beside `select`'s `stored`/`missing_late` handling; whether ESS's `NotInterpreted` for several creation addresses must be mirrored; what a repeated creation of an existing row does today.
