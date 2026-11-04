---
format: aep.planning-md/3
id: story:admit-against-environment
kind: story
status: implemented
title: Admit a deployment against an environment
relations:
- decomposes: epic:m0-run-in-process
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 2, decided_on: {"recorded":{"test_result":1}}}
- {from: "proposed", to: "active", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-04T18:28:15Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# Admit a deployment against an environment

## Outcome

A `spindle-environment/1` document maps capabilities to implementations in order of preference.
`admit` binds each requirement to the most preferred implementation, or refuses and names every
missing capability; `b10x-spindle admit` exits 0 when admitted and 3 when refused. This is
`spindle.deployment.DecideAdmission`'s `admitted` and `refused` outcomes.

## Acceptance

- `admission_takes_the_most_preferred_implementation`.
- `refusal_names_every_missing_capability`.
- `a_prefix_key_provides_every_capability_under_it_and_an_exact_key_wins`.
- `an_environment_in_another_format_is_refused`, `an_empty_implementation_list_is_refused`.
- `admitted_exits_zero_and_prints_the_bindings`, `refused_exits_three_and_names_the_missing_capability`.

## Scope

`crates/spindle-capability/src/{environment,admission}.rs`, `crates/spindle-cli/`,
`examples/environments/`.
