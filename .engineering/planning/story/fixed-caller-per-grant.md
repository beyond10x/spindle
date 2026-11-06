---
format: aep.planning-md/3
id: story:fixed-caller-per-grant
kind: story
status: active
title: A fixed caller holds several actors, chosen per command by grant
relations:
- serves: vision:spindle
- depends_on: story:served-fixed-caller
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T10:10:59Z", actor: "agent:b10x-chef-orchestrator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-06T10:10:59Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
---
# A fixed caller holds several actors, chosen per command by grant

## Outcome

`run --fixed-actor` may be given more than once (each with its own `--fixed-caller`). A served
command is sent as the first fixed actor whose `may:` names it, and a read-granted view is read as
the first fixed actor that may read it; an operation no fixed actor may run is refused 403 "not
granted" as today. A UI test backend whose screens span several actors' domains can then serve
all of them in one run. It stays a test-backend switch, not authentication (design §11).

## Acceptance

- `each_command_runs_as_the_fixed_actor_granted_it` — two actors, each granted one command of a
  neutral fixture; both commands answer their outcome, and each guard reads its own actor's
  caller attributes.
- `an_operation_no_fixed_actor_may_run_is_refused_403` — nothing is written.
- `the_fixed_actors_are_printed_at_startup_in_order`.
- `two_fixed_actors_granted_one_command_take_the_first_given` — the choice is stated and tested.

## Origin

A consumer serving a 30-actor specification as a UI test backend (2026-11-06, on main 62b97a9):
with one fixed actor, every other domain's commands answer 403; without one, those that read the
caller answer 501. Follows `story:served-fixed-caller`.
