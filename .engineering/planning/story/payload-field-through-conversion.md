---
format: aep.planning-md/3
id: story:payload-field-through-conversion
kind: story
status: draft
title: An outcome field filled through a declared conversion runs
relations:
- serves: vision:spindle
- informed_by: story:binding-mappings-across-conversions
revision: 1
---
## Outcome

An outcome whose payload or `sets:` field is filled through a declared conversion (a component's
`conversions:` entry, such as a stamped instant into a `Timestamp` field) runs in `run` (stdin and
served) and `conform`, writing the converted value as ESS 0.99.1's interpreter does. Today the
specification is admitted, and a request that selects such an outcome is refused with
"unsupported: the conversion `<because text>` into `<field>`"
(`crates/spindle-kernel/src/supported.rs:839-843`, refused whenever `field.conversion` is set).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: six outcomes on its first path are refused
this way, four of them needed by its walking skeleton (an assignment stamped `assigned_at`, a
session opened and a heartbeat stamped `last_seen_at`, a lease renewed). `story:binding-mappings-across-conversions`
covers binding mappings only.

## Acceptance

- `a_payload_field_through_a_conversion_is_written_as_ess_converts_it`: a neutral fixture with a
  `conversions:` entry from an instant into a `Timestamp` field; the command answers, the row holds
  the converted value, and ESS's interpreter (`ess_conformance::interpret`) gives the same row.
- `conform_runs_an_outcome_with_a_conversion`: `conform` on the same fixture; the scenario of that
  outcome executes and passes.
