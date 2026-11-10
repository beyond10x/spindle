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
revision: 3
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
