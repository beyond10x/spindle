---
format: aep.planning-md/3
id: story:route-literal-segment-precedence
kind: story
status: draft
title: A literal path segment wins over a path variable
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/tests/fixtures/route-precedence
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/route_precedence.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 2
---
# A literal path segment wins over a path variable

## Outcome

When two routes of one method both match a request path, the route with a literal segment where the
other has a variable answers it: `GET /things/modules` reaches the route bound to
`/things/modules`, never the one bound to `/things/{id}`, whatever order the bindings are declared
in. Today `Surface::resolve` (`crates/spindle-ingress/src/surface.rs`) answers with the first route
in declaration order whose template carries the path, so a by-id route declared earlier shadows a
literal sibling. The precedence follows what ESS's HTTP binding defines for overlapping templates
(read through `ess-http`; if ESS defines none, the most specific template wins, segment by segment
from the left, and the gap is reported to ESS).

## Acceptance

A neutral fixture with `GET /things/{id}` declared before `GET /things/modules` and
`GET /things/actions`, in the spindle-cli test binary:

- `a_literal_segment_wins_over_a_path_variable`: `GET /things/modules` and `GET /things/actions`
  answer their own commands' outcomes; `GET /things/42` answers the by-id route.
- `declaration_order_does_not_change_the_match`: the same requests, with the bindings declared in
  the opposite order, answer the same routes.
- `a_method_neither_route_serves_answers_405`: a method served by neither route answers 405 naming
  the methods of the routes whose templates carry the path.

## Scope

- `crates/spindle-ingress/src/surface.rs` — cited: `Surface::resolve`, route order.
- `crates/spindle-cli/tests/spindle-cli/route_precedence.rs` — inferred, new.
- `crates/spindle-cli/tests/fixtures/route-precedence/` — inferred, new.

## Origin

Reported 2026-11-07 by a consumer serving its specification on spindle: two literal routes under a
collection are answered by the collection's by-id route.
