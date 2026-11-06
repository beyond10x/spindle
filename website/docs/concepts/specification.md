---
title: The specification
sidebar_position: 1
description: What Spindle reads, how it compiles it, and what the kernel executes from it.
lede: A specification is a directory of ESS documents, compiled by ESS's own compiler into the program Spindle runs.
source: crates/spindle-spec/src/lib.rs, crates/spindle-spec/tests/load.rs, crates/spindle-kernel/src/lib.rs, crates/spindle-kernel/src/supported.rs, docs/design/spindle.md §3a
---

A specification is a directory of [ESS](https://beyond10x.github.io/ess/)
([GitHub](https://github.com/beyond10x/ess)) source documents with a `system.yaml` at its root.
It states **semantics** only: entities and their lifecycles, commands and their outcomes, guards,
events, views, components and the bindings between them. It never names a database, a protocol or
a queue.

## Loading

`spindle-spec` reads the files the directory's `ess-inputs.yaml` lists, in that order, or, without
one, every `.yaml` and `.yml` file under the directory in path order. A manifest `ess` refuses is
refused here too: a format other than `ess-inputs/1` or `ess-inputs/2`, an entry that is absolute,
climbs out with `..` or repeats another, and an entry that passes through a symlink. It parses them together (a type declared in one file may be used in another), assembles them and compiles them
with `ess-compiler`. The result is the resolved IR: every reference checked, every guard a typed
ESS predicate. It is the same IR `ess specify compile` produces, because it is the same code.

Spindle never re-implements ESS parsing or validation. A document `ess specify validate` refuses is
refused by `b10x-spindle check` too, with the compiler's own diagnostics. The ESS crates are pinned
to a released tag in the workspace `Cargo.toml`.

## Executing

`spindle-kernel` executes one command against the compiled model:

1. It reads the instance the command addresses from a store.
2. It selects the outcome in the order of ESS's precedence plan: input refusals, whether the
   instance exists, what it holds, causes outside the model (`external:`), then the default.
3. It applies the outcome's effect (`creates:`, `moves:`, `updates:`), checks every written value
   against its declared type and the instance against its entity's invariants, and commits.
4. It returns the events the outcome emits, or the error it declares.

A guard that evaluates to *unknown* refuses the command; it never takes a branch by default.

A `creates:` outcome whose identity is neither an input nor a payload source (`generated: true`,
or no source) leaves the identity to the runtime, which mints it: only a `Uuid` identity, or a
newtype of one, as a UUIDv4. This needs the `identity.mint` capability
([capabilities](/docs/concepts/capabilities)).

The kernel runs the constructs Spindle's own specification and the rally example use. Any other
construct is refused by name as unsupported, before the command reads or writes anything.

## Views

A view is answered from the store of the component that owns its source: one row by identity, or a
list over every instance of the entity, with `filter: state == <State>` and
`order_by: [<field> asc|desc]`. Parameters, computed fields, aggregation, fields read through a
reference, a default row for an absent identity, paging and any other filter are refused as
unsupported.

## Scenarios

`b10x-spindle run <spec> --environment <env> --scenario <file>` replays an authored
`ess-scenario` document. ESS's own reader reads the document and checks it against the model, and
ESS's conformance runner runs its steps against every component of the system in one process, on
empty in-memory stores whatever `store.durable` is bound to, and compares what happened with what
each step expects: the outcome, the error, the events, an identity captured as `$instance` for a
later step, and each view assertion. Spindle prints what the document arranges, each timeline act
and each assertion that holds. At the first that does not, it prints what was expected and what
was observed and exits 1. That step is found by replaying ever shorter prefixes of the document
from an empty system, halving each time, and the replay it prints ran no step after it. A
document ESS refuses, such as one with an unknown `type:`, or one of more than 1000 steps, exits 1
before any step runs.

An entity the document arranges with `setup:` (an identity, its fields and a lifecycle state) is
checked by ESS and written as it rests into the store of the component that owns it, as ESS's
interpreter establishes it: no command runs, no event is published, and the acts after it read it.

A replay forces no external cause, sends no command as an actor and reads no view with
parameters; a step that needs one is reported as unsupported, which also exits 1.

:::caution[Planned]
Running the conformance suite ESS synthesises from Spindle's own specification is planned; see
[status](/docs/status).
:::
