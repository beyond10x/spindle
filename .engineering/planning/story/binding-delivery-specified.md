---
format: aep.planning-md/3
id: story:binding-delivery-specified
kind: story
status: implemented
title: How an event binding delivers is specified
relations:
- decomposes: epic:spec-states-runtime
- informed_by: review-result:ess-review-2026-11-09
- serves: vision:spindle
scope:
- confidence: inferred
  path: ess/components.yaml
- confidence: inferred
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/ess-inputs.yaml
- confidence: inferred
  path: ess/system.yaml
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T21:33:09Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-10T06:35:52Z", actor: "human:timo", revision: 8, decided_on: {"recorded":{"test_result":1,"review_outcome":3,"verification":1}}}
---
# How an event binding delivers is specified

## Outcome

An event binding between components delivers through an outbox, with a delivery guarantee and a
retry limit of five attempts. A binding whose input is not met invokes nothing. This is in
`crates/spindle-channel/src/lib.rs:13-14`, `docs/design/spindle.md:386,393` and
`website/docs/concepts/components-and-bindings.md:36-48`. `ess/components.yaml:9-36` declares only
what `spindle-control` accepts and publishes. The guarantee, the retry count and the unmet-input
case have no typed home (H13, `R`, `review-result:ess-review-2026-11-09`).

As in `story:periodic-timing-specified`, first establish whether ESS defines these terms. Then
specify what is Spindle's in Spindle's specification, and make the observe answers publish what is
ESS's. `AGENTS.md` forbids technical words: never name a queue or a protocol.

## Acceptance

- `ess specify validate --path ess` exits 0.
- The report says, for the guarantee, the retry count and the unmet input, whether each is ESS's or
  Spindle's, with a citation.
- Every added declaration cites its source, and every unread rule is an `UNMAPPED:` comment.
- An unmet binding input has a typed, observable answer (an event, a counter or a request-record
  field), or the report cites why none can be given.
- If observe types change, `task generate:observe` is run, and both observe contract checks pass.

## Shared surfaces

A new domain file adds one line to `ess/system.yaml` and `ess/ess-inputs.yaml`, appended at the
end. Observe changes are appended at the end of `observe.yaml`'s types.
