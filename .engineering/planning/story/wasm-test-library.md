---
format: aep.planning-md/3
id: story:wasm-test-library
kind: story
status: draft
title: A TypeScript library runs specifications over the wasm host for tests
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:wasm-host-builds
revision: 1
---
# A TypeScript library runs specifications over the wasm host for tests

## Outcome

A TypeScript library over the wasm host loads ESS specifications at run time, runs several
components in one in-memory runtime joined by in-process channels, and takes every capability
(publish channel, session store, identity mint, clock, external answers) as a binding supplied from
TypeScript. A test can then mock a capability or fake one endpoint, and start one isolated stack per
test with its own seeded world and external answers.

## Acceptance

- `a_test_starts_an_isolated_stack_with_a_faked_endpoint` — two components from a neutral
  specification, one external answer faked from TypeScript, a command answered as the interpreter
  answers it.
- Two stacks in one test process share no state.

## Origin

Operator idea, 2026-11-06: "for later, probably, maybe not even necessary". Prior art:
`ess verify conform synthesize --target web` (stateless). Depends on `story:wasm-host-builds`.
