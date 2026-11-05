---
format: aep.planning-md/3
id: epic:downstream-kernel-constructs
kind: epic
status: draft
title: Run a downstream specification's constructs
relations:
- serves: vision:spindle
revision: 1
---
# Run a downstream specification's constructs

## Outcome

A downstream specification (44 commands, 16 event bindings, `format: ess/34`) runs on spindle
without a command or binding refused for a construct the kernel or the channel does not run. A
probe on `831202d` (2026-11-05) refuses 21 of its 44 commands, the first construct per command
being: set outcomes 6, `when_related:` over a row set 3, over one row 3, a command response 3,
secondary effects 2, a value read from a related row 2, a branch the caller decides 1,
a generated `Bytes` 1. Behind those it also needs caller values, generated `List` values and
String-newtype identities, and its bindings need retry, policies per refusal and mappings across
a declared conversion.

Every construct is reproduced on a neutral fixture; nothing from the downstream specification
enters this repository.

## Stories

- `story:kernel-set-outcomes`
- `story:kernel-related-row-guards`
- `story:kernel-related-row-values`
- `story:kernel-row-set-guards`
- `story:kernel-command-responses`
- `story:kernel-secondary-effects`
- `story:kernel-caller-attributes`
- `story:generated-bytes-and-list-values`
- `story:binding-mappings-across-conversions`
- `story:binding-refusal-policies`

Already planned elsewhere: `story:mint-string-newtype-identities`,
`story:event-bindings-retry-on-failure`, `story:serve-what-runs`.
