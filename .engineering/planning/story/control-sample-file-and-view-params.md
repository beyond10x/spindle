---
format: aep.planning-md/3
id: story:control-sample-file-and-view-params
kind: story
status: draft
title: The control process writes a sample file and passes view parameters through
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/src/deploy.rs
- confidence: inferred
  path: crates/spindle-control/src/history.rs
- confidence: cited
  path: crates/spindle-control/src/server.rs
revision: 3
---
# The control process writes a sample file and passes view parameters through

## Outcome

The control process can write its samples to a file for later replay (`docs/design/observe.md`
§ 5, "optional write to a file for later replay"), and `GET /api/views/<component>/<view>` passes
a view's path and query parameters through to the instance's ingress. Today neither: no sample
file, and the view route in `crates/spindle-control/src/server.rs` (`view`) asks the ingress
without them.

## Acceptance

- `samples_are_written_to_the_sample_file` — with the sample file option set, every streamed
  sample is in the file, one per line, valid against `spindle.observe.Sample`.
- `a_view_with_parameters_is_answered_through_the_ingress` — a view route that takes a path or
  query parameter returns the instance's answer for those parameters.

## Origin

Cut from `story:control-process-serves-the-api` (wave 6).

## Scope

- `crates/spindle-control/src/server.rs` — cited: `view`.
- `crates/spindle-control/src/history.rs` — inferred: where samples are kept.
- `crates/spindle-cli/src/deploy.rs` — inferred: the option.
