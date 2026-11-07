---
format: aep.planning-md/3
id: story:wire-names-everywhere
kind: story
status: draft
title: Every value the ingress writes carries ESS's wire names
tags:
- from-finding
relations:
- informed_by: review-result:w12-u3-adversary-1
- serves: vision:spindle
revision: 1
---
# Every value the ingress writes carries ESS's wire names

## Outcome

Since wave 12 the ingress writes a command's response under the wire names ESS publishes
(`crates/spindle-ingress/src/wire.rs`). Four other answers still key a value by its declared name,
so a specification that gives a member `naming: {wire}` gets an answer the schema ESS projects
refuses: event payloads in the envelope's `published`, the error `payload` of a refusing outcome,
a view's `rows`, and the console's `/api/events`.

## Acceptance

- One neutral fixture with a wire-renamed member in an event payload, an error field, a view row
  and a console event; each answer validates against the schema `ess generate --kind openapi`
  publishes and equals what ESS's interpreter answers for it.

## Origin

Reported by the implementor of `story:http-outcome-bodies` in its correction round after
`review-result:w12-u3-adversary-1`; pre-existing, read from the code, not run.
