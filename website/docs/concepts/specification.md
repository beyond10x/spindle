---
title: The specification
sidebar_position: 1
description: What Spindle reads, how it compiles it, and what the kernel executes from it.
lede: A specification is a directory of ESS documents, compiled by ESS's own compiler into the program Spindle runs.
source: crates/spindle-spec/src/lib.rs, crates/spindle-spec/tests/spindle-spec/load.rs, crates/spindle-ingress/src/console.rs, crates/spindle-cli/tests/spindle-cli/served_operations.rs, crates/spindle-kernel/src/lib.rs, crates/spindle-kernel/src/supported.rs, crates/spindle-cli/src/conform.rs, crates/spindle-cli/tests/spindle-cli/conform.rs, docs/design/spindle.md §3a
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

A `when_related:` guard over a row set, `{entity, where: [...], count: {gte: 1}}` or `exists:` or
`forall:`, reads every stored row of the entity, as ESS's interpreter does. A comparison with an
`Optional` field a row does not hold, such as `box_id == input.id` on an item in no box, is
*unknown* for that row, not false. The row may or may not be in the set, so a count it could change
refuses the command as undecidable. The refusal names the filter, the entity and the absent field, and
no stored row. To leave such a row out of the set, write `defined(<field>)` beside the
comparison: `where: [defined(box_id), box_id == input.id]`.

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
included, as ESS's interpreter answers it.

A filter may read a field of the row a reference names, `person_id.state == Active`, from any
component. That row is loaded the same way, once per read by its identity, and the referenced
entity is never scanned. A row whose reference is empty or names no row is left out where the
filter is unknown only for that reason, as ESS's interpreter leaves it out.

A filter is read where each leaf does one of these:

- compares a field with a literal of its type: `archived == false`, `priority >= 3`. The ordered
  operators apply only where ESS orders the type. A timestamp also compares against `now`, or
  `now - 5m`, read on the host's clock.
- lists a field `in:` or `none_of:` literals
- compares a field with a parameter or another field: `person_id == param.viewer`,
  `person_id.name == holder`. The ordered operators apply where ESS orders both fields and they
  are of one kind: `started_at > person_id.joined_at`.
- asks `defined`

A field here is a field of the row, or of the row a reference names.

The leaves are combined by `any`, `all` and `not`, or quantified by `exists` or `forall` over a
list parameter or a list field of the row. Each filter is evaluated as ESS evaluates it. A filter
ESS leaves unknown for a row, such as a comparison with an optional field the row does not hold,
refuses the read as ESS's interpreter does.

A view with aggregate fields, `open_sessions: {aggregate: {count: {}}}` or `sum` or `avg`, answers
the rows ESS's interpreter answers. The filter keeps the instances first. They are then grouped by
the `group_by` fields, one row per group in the order of the group keys. A view without `group_by`
answers one row, also over no instance. A measure's own `where:` counts only the instances it
keeps, and is read where it is a filter that is read. A count and a sum over no instance are `0`,
an average over none is `null`, as is an optional group key a group holds no value for. Integers
are answered as integers. An aggregate view has no row by identity, and an aggregation beside a
field read through a reference is refused.

A computed field, `score_change: {type: Integer, expr: "score - coalesce(previous_score, score)"}`,
answers what ESS's evaluator computes over the row's own fields, as ESS's interpreter answers it,
beside any field read through a reference. An absent optional operand, or a zero divisor outside
`coalesce`, answers `null`. An `Integer` is answered as an integer, a `Decimal` as ESS writes it.
A value ESS determines none of, such as an `Integer` past 64 bits, refuses the read.

A default row for an absent identity, paging and any other filter are refused as unsupported, and
the refusal names the construct.

`run` prints each view and command it does not serve at startup, as
``not served: view `<name>` at GET <path>: <reason>``. `admit` prints the same line for each view
without serving. A served `run` also answers `GET /api/served`: one row per command and view it
routes, with `served` and, for one it does not serve, the same `reason`
(`spindle.observe.ServedOperations`).

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
