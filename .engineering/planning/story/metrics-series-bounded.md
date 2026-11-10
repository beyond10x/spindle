---
format: aep.planning-md/3
id: story:metrics-series-bounded
kind: story
status: draft
title: An operator bounds the series a metrics scrape writes
relations:
- decomposes: epic:runtime-observability-depth
- serves: vision:spindle
revision: 1
---
## Outcome

An operator can bound how many series `GET /__spindle/metrics` writes for a large specification.
The operator chooses whether commands are counted one series each or aggregated, and the scrape
states which was chosen.

## Why

Measured on 0.8.0 by a downstream deployment, reported 2026-11-10: on its bundle one scrape of
`/__spindle/metrics` wrote 12,225 series in 1,198,629 bytes. Commands are labelled by qualified
name: `Series` is method, route template, operation, kind, class and status class
(`crates/spindle-ingress/src/observe.rs:446-448`). External causes are counted by command, outcome
and source (`observe.rs:468`). The family text is written in
`crates/spindle-ingress/src/observe/metrics.rs`.

Every label value comes from the specification, so the count is bounded, but the bound grows with
the specification's size. The downstream keeps `--observe` off in its dev deployment for this reason
and because of `story:refusal-reason-bounded`.

## Acceptance

- `run --observe` can be told to count requests without the route and operation labels. Each
  method, kind, class and status class then has one series, whatever the number of commands.
- On the default, the series of a scrape are the same as on 0.9.0.
- The scrape, or its family description, says which mode is in effect.
- A served run over a specification with 300 commands, all answered once, writes at most a stated
  number of series in the aggregated mode. The test states that number.
- `website/docs/concepts/observe.md` states the series bound of each mode.

## Scope

- `crates/spindle-ingress/src/observe.rs` (`Series`, `Traffic`), inferred
- `crates/spindle-ingress/src/observe/metrics.rs`, inferred
- `crates/spindle-cli/src/cli.rs` (the `--observe` flags), inferred
- `ess/domains/observe.yaml`, where the label vocabulary is specified
  (`story:observe-label-vocabulary`, draft), inferred
- `website/docs/concepts/observe.md`, inferred
