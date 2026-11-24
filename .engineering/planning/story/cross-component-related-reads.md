---
format: aep.planning-md/3
id: story:cross-component-related-reads
kind: story
status: draft
title: A guard or an effect over another component's entity is refused or read from its owner
summary: Guards and affects entries over an entity another component owns read the running component's store; refuse them by name or read the owner's.
relations:
- serves: vision:spindle
revision: 1
---
# A guard or an effect over another component's entity is refused or read from its owner

## Outcome

A command whose guard or secondary effect reads rows of an entity another component owns is
either answered from the owner's store, as ESS's interpreter answers it, or refused by name before
anything is read, as a value over such an entity is refused since this story was filed.

## Why

The kernel reads and writes only the store of the component that runs the command. ESS's
interpreter holds every entity in one store. Values are covered: `spindle_kernel::takes`
(`crates/spindle-kernel/src/supported.rs`) refuses an outcome whose `{related: {entity, where,
field}}` or `{related: {via: input.<field>, field}}` reads an entity the running component does
not own, with an `unsupported: …` reason (`crates/spindle-host/tests/spindle-host/cross_component_reads.rs`).
Three sites with the same limit are not covered, because selection reads them and the
selection-time check (`spindle_kernel::selects`) takes no IR to find an owner:

- a `when_related:` guard over a row the input or a stored field names (`Run::select`,
  `crates/spindle-kernel/src/execute.rs`): the row is loaded from the running component's store,
  so it is missing and the `exists: false` branch answers where the interpreter finds the row;
- a row-set `when_related:` guard (`Run::row_set`): the entity is scanned in the running
  component's store and finds no row;
- an `affects:` entry over an entity another component owns (`execute/affects.rs`): it selects and
  writes rows in the running component's store.

Found by a review of `0.19.0..0.19.1`, beside the value refusal.

## Acceptance

- `a_guard_over_another_components_entity_is_refused_by_name`: on a two-component fixture, a
  command with a `when_related:` guard over the other component's entity is named not served by
  `admit` and `run`, and a request for it answers `unsupported` naming the entity, its owner and
  the running component.
- `a_row_set_guard_over_another_components_entity_is_refused_by_name`: the same for a row-set
  guard.
- `an_affects_entry_over_another_components_entity_is_refused_by_name`: the same for an
  `affects:` entry, refused for the outcome alone.
- `a_guard_over_the_running_components_entity_is_served`: a guard over the running component's
  own entity answers as before.

## Not covered

- Reading another component's store in one process: a choice for after this refusal, recorded in
  the design (§11) if taken.
