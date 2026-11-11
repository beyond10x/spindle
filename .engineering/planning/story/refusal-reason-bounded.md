---
format: aep.planning-md/3
id: story:refusal-reason-bounded
kind: story
status: implemented
title: A refused command's logged reason is bounded and holds no input value
relations:
- decomposes: epic:runtime-observability-depth
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: cited
  path: website/docs/concepts/observe.md
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:40:09Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-11T09:40:09Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:03Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A refused command's request-log record holds a bounded `reason` that names the field and the type
it expected, never the value the caller sent. Every record has a stated byte bound.

## Why

Measured on 0.8.0 by a downstream deployment, reported 2026-11-10: with `run --observe`, one refused
command with an 800 KB mistyped field made `GET /__spindle/requests` 804,364 bytes longer
(7,403 → 811,767). Example text: invalid input: `name`: ["…"] is not a String.
- At the default 256 records that is about 196 MiB held in the process and served on every poll.
  At `--observe-requests 10000` it is about 7.5 GiB (`crates/spindle-cli/src/cli.rs:225`).
- The body values of refused commands can be read by whoever reads the log.

Where it comes from:
- `RequestRecord.reason` is "the `refused` text of the answer, as the caller received it"
  (`ess/domains/observe.yaml:341-345`, tag 0.8.0).
- The ingress copies it unchanged (`crates/spindle-ingress/src/observe.rs:506`).
- So the answer the caller receives also carries the value back. Whether the refusal text in the
  answer should drop it too is part of this story.

## Acceptance

- A command refused for a mistyped field of 800 KB grows `GET /__spindle/requests` by at most the
  stated per-record bound.
- That record's `reason` names the field and the expected type.
- That record's `reason` holds no byte of the value sent.
- The `RequestRecord.reason` summary states the bound.
- The `RequestRecord.reason` summary says that a refused value is never recorded.

## Scope

- `ess/domains/observe.yaml` (`RequestRecord.reason`)
- `crates/spindle-ingress/src/observe.rs` (record construction near :506)
- where the input-decoding refusal text is built, in `crates/spindle-ingress` or `crates/spindle-kernel`
- `website/docs/concepts/observe.md`
