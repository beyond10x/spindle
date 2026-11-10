---
format: aep.planning-md/3
id: story:deploy-launches-any-model
kind: story
status: draft
title: deploy launches every instance of any specification with run --component
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:run-one-component
scope:
- confidence: cited
  path: crates/spindle-cli/src/component.rs
- confidence: cited
  path: crates/spindle-cli/src/deploy.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli
- confidence: cited
  path: crates/spindle-control/tests/spindle-control
revision: 3
---
# deploy launches every instance of any specification with run --component

## Outcome

`deploy` launches each placed instance as `b10x-spindle run <spec> --environment <file>
--component <component>` instead of `play … --component` (`crates/spindle-cli/src/deploy.rs:56`),
so a placement of any specification runs, not only rally. Telemetry, the ingress settings and the
token guard stay as they are (`crates/spindle-cli/src/cli.rs:255-261`).

## Acceptance

- `deploy_runs_a_second_specification`: a placement of the neutral two-component fixture deploys,
  and `/api/deployment` lists both instances as running.
- `a_second_specification_reports_telemetry`: each of those instances has at least one telemetry
  sample at `/api/deployment`.
- `rally_still_deploys`: with `examples/placements/rally-processes.toml`, `/api/deployment` lists
  the three rally instances as running, each with at least one telemetry sample.

## Scope

- `crates/spindle-cli/src/deploy.rs` (the launch arguments, line 56).
- `crates/spindle-cli/src/component.rs` (shared with `story:run-one-component`, which runs first).
- `crates/spindle-control/tests/`, `crates/spindle-cli/tests/`.
