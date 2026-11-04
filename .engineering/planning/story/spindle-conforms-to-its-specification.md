---
format: aep.planning-md/3
id: story:spindle-conforms-to-its-specification
kind: story
status: draft
title: Spindle conforms to its own specification
relations:
- decomposes: epic:m0-run-in-process
revision: 1
---
# Spindle conforms to its own specification

## Outcome

The conformance suite ESS synthesises from `ess/` runs against Spindle's executor through an
in-process target, and `task check` runs it with its counts held.

## Acceptance

- Every synthesised scenario of `spindle.deployment` executes; none is skipped without a recorded
  reason.
- `task check` fails when a scenario fails.

## Scope

`crates/spindle-conformance/` (new), `Taskfile.yml`.
