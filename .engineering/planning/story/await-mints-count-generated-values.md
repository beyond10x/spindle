---
format: aep.planning-md/3
id: story:await-mints-count-generated-values
kind: story
status: draft
title: An awaited system step mints the value ESS's interpreter mints, after generated values
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

When `HostTarget` takes a step the system issues itself and the step mints a value, the value equals
the one ESS's interpreter mints at the same point of the scenario, also after earlier commands
generated values.

## Why

Wave 32's U2 (`story:conform-awaits-system-steps`) mints an awaited step's values from a counter
that starts at the scenario's first await. ESS's interpreter's counter also counts every value the
scenario's earlier commands generated; the host mints those from its own seed, and neither
`HostTarget` nor the host's report says how many values a command generated. The adversary case
`adversary_w32_u2::an_await_after_a_generated_value_mints_as_the_interpreter` shows it: after a
command whose event carries a generated value, the interpreter's `Stamped.code` is 3 and the host's
is 2. Only an authored document that states a minted value reaches it; a synthesised suite does not.
The case is ignored with this story's id.

## Acceptance

- The ignored case runs and passes.
- The host or kernel reports the number of values each command generated, or the target counts
  them from the model; the choice is recorded in the design row of the ESS-divergence table.
- The synthesised-suite parity cases of `adversary_w32_u2` stay green.
