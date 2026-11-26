---
format: aep.planning-md/3
id: story:scenario-without-components
kind: story
status: draft
title: run --scenario replays a specification that declares no component
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`run --scenario` replays a specification that declares no component, the way stdin mode runs one:
every command is accepted by one implied component that owns every domain. Today every act of such
a specification diverges with `no component accepts <command>` and the run exits 1, so a consumer
adds a generated component file to a copy of each specification before replaying it.

## Acceptance

- `a_scenario_replays_a_specification_without_components`: a specification with one command and no
  component, and a scenario with one act of that command, prints `holds` and exits 0.
- `a_scenario_on_declared_components_still_routes_by_accepts`: a specification whose components do
  not accept the act's command still diverges with `no component accepts <command>` and exits 1.
- `stdin_and_scenario_agree_on_a_specification_without_components`: the same command sent on stdin
  and as a scenario act answers the same outcome.

## Not covered

- Acts sent as a system actor: ESS's scenario authoring refuses them (`ESS-AUTHOR-044`,
  `ESS-AUTHOR-045`); Spindle applies ESS's rules unchanged. story:stdin-arrange-and-line-references
  covers sending such commands on stdin.

## Origin

A consumer's simulation, 2026-11-26, on 0.21.0, with a standalone reproduction.
