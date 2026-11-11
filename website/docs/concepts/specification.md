---
title: The specification
sidebar_position: 1
description: What Spindle reads, how it compiles it, and what the kernel executes from it.
lede: A specification is a directory of ESS documents, compiled by ESS's own compiler into the program Spindle runs.
source: crates/spindle-spec/src/lib.rs, crates/spindle-spec/tests/spindle-spec/load.rs, crates/spindle-kernel/src/lib.rs, crates/spindle-kernel/src/supported.rs, crates/spindle-cli/src/conform.rs, crates/spindle-cli/tests/spindle-cli/conform.rs, docs/design/spindle.md §3a
---

The directory of [ESS](https://beyond10x.github.io/ess/)
([GitHub](https://github.com/beyond10x/ess)) source documents has a `system.yaml` at its root.
It states **semantics** only: entities and their lifecycles, commands and their outcomes, guards,
events, views, components and the bindings between them. It does not name a database, a protocol
or a queue.

## Loading

`spindle-spec` reads the files the directory's `ess-inputs.yaml` lists, in that order. Without
one, it reads every `.yaml` and `.yml` file under the directory in path order. A manifest `ess`
refuses is refused here too:

- a format other than `ess-inputs/1` or `ess-inputs/2`
- an entry that is absolute, climbs out with `..` or repeats another
- an entry that passes through a symlink

It parses the files together, so a type declared in one file may be used in another. It
assembles them and compiles them with `ess-compiler`. The result is the resolved IR: every
reference checked, every guard a typed ESS predicate. It is the same IR `ess specify compile`
produces.

Spindle does not re-implement ESS parsing or validation. A document `ess specify validate` refuses
is refused by `b10x-spindle check` too, with the compiler's own diagnostics. The ESS crates are
pinned to a released tag in the workspace `Cargo.toml`.

## Executing

`spindle-kernel` executes one command against the compiled model:

1. It reads the instance the command addresses from a store.
2. It selects the outcome in the order of ESS's precedence plan: input refusals, whether the
   instance exists, what it holds, causes outside the model (`external:`), then the default.
3. It applies the outcome's effect (`creates:`, `moves:`, `updates:`, `deletes:`), then its
   secondary effects (`affects:`): each entry moves, updates or removes the rows of its entity its
   filter selects in the store as it was before the outcome, the subject excepted, in the order
   written. It checks every written value against its declared type and every row left in place
   against its entity's invariants, and commits every row in one write, or none.
4. It returns the events the outcome emits, or the error it declares.

A guard that evaluates to *unknown* refuses the command. It does not take a branch by default.

A `creates:` outcome whose identity is neither an input nor a payload source (`generated: true`,
or no source) leaves the identity to the runtime, which mints it. A `Uuid` identity, or a `Uuid`
newtype without invariants, is minted as a UUIDv4. A `String` newtype identity whose prefix and
alphabet admit a UUID's text, and whose invariants bound only its length, is minted as its prefix
and a UUIDv4's text. Any other identity type is refused. This needs the `identity.mint` capability
([capabilities](/docs/concepts/capabilities)).

The kernel runs the constructs Spindle's own specification and the rally example use. Any other
construct is refused as unsupported before anything is written, and the refusal names the
construct.

- A construct that decides which outcome a request selects, a condition or an outcome's shape,
  refuses the whole command before it reads anything.
- A construct only the selected outcome reads refuses that outcome alone. Examples are a value it
  would generate of a type the runtime does not mint, and a secondary effect over an input list
  (an `affects:` entry with `each:`). A
  request that selects the outcome is refused with the outcome in the message, and every other
  outcome of the command runs.

`admit` and `run` print each such outcome on stderr. A command none of whose outcomes runs is
printed once instead, as not served.

## Views

A view is answered from the store of the component that owns its source: one row by identity, or a
list over every instance of the entity, ranked by `order_by: [<field> asc|desc]`.

A field read through a reference, `field: {related: {via: resource_id, field: name}}`, answers what
the row the reference names holds now: a stored field, its identity or its `state`. The row may
belong to another component. Each row a list names is loaded once by its identity, however many
rows name it. The field is left out where the reference is empty or names no row, a removed one
included, as ESS's interpreter answers it. A filter that reads through a reference is refused.

A filter is read where each leaf does one of these:

- compares a field with a literal of its type: `archived == false`, `priority >= 3`. The ordered
  operators apply only where ESS orders the type. A timestamp also compares against `now`, read on
  the host's clock.
- lists a field `in:` or `none_of:` literals
- compares a parameter with a field by `==` or `!=`
- asks `defined`

The leaves are combined by `any`, `all` and `not`, or quantified by `exists` or `forall` over a
list parameter or a list field of the row. Each filter is evaluated as ESS evaluates it. A filter
ESS leaves unknown for a row, such as a comparison with an optional field the row does not hold,
refuses the read as ESS's interpreter does.

Computed fields, aggregation, fields read through a
reference, a default row for an absent identity, paging and any other filter are refused as
unsupported, and the refusal names the construct.

## Scenarios

`b10x-spindle run <spec> --environment <env> --scenario <file>` replays an authored
`ess-scenario` document. ESS's own reader reads the document and checks it against the model.
ESS's conformance runner runs its steps against every component of the system in one process, on
empty in-memory stores whatever `store.durable` is bound to. It compares what happened with what
each step expects: the outcome, the error, the events, an identity captured as `$instance` for a
later step, and each view assertion.

Spindle prints what the document arranges, each timeline act and each assertion that holds. At the
first that does not, it prints what was expected and what was observed and exits 1.

That step is found by replaying ever shorter prefixes of the document from an empty system, halving each time.
The replay it prints ran no step after it. A document ESS refuses, such as one with an unknown
`type:` or one of more than 1000 steps, exits 1 before any step runs.

An entity the document arranges with `setup:`, an identity with its fields and a lifecycle state,
is checked by ESS. It is written as it rests into the store of the component that owns it, as
ESS's interpreter establishes it: no command runs, no event is published, and the acts after it
read it.

A replay forces no external cause, sends no command as an actor and reads no view with
parameters. A step that needs one is reported as unsupported, which also exits 1.

## Conformance

`b10x-spindle conform` runs the suite ESS synthesises from a specification, or one written for it,
against every component in one process. `task check` runs it on Spindle's own specification. It
lists every synthesis refusal with its code and holds the executed and skipped counts
([status](/docs/status)).
