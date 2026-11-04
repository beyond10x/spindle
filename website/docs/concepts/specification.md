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

`spindle-spec` reads every `.yaml` and `.yml` file under the directory in path order, parses them
together (a type declared in one file may be used in another), assembles them and compiles them
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

The kernel runs the constructs Spindle's own specification and the rally example use. Any other
construct is refused by name as unsupported, before the command reads or writes anything.

:::caution[Planned]
List views with filters and ordering, minted creation identities, and replaying ESS scenarios
against the kernel are planned; see [status](/docs/status).
:::
