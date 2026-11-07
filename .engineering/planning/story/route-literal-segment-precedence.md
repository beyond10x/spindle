---
format: aep.planning-md/3
id: story:route-literal-segment-precedence
kind: story
status: draft
title: A literal path segment wins over a path variable
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/route-precedence
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/route_precedence.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 5
---
# A literal path segment wins over a path variable

## Outcome

When two routes of one method both match a request path, the route with a literal segment where the
other has a variable answers the request. `GET /things/modules` reaches the route bound to
`/things/modules`, never the one bound to `/things/{id}`, whatever order the bindings are declared
in. Today `Surface::resolve` (`crates/spindle-ingress/src/surface.rs`, line 796) answers with the
first route in declaration order whose template carries the path, so a by-id route declared
earlier shadows a literal sibling.

The precedence is the one ESS 0.90.0 defines (`ESS-HTTP-011`, `ess-http` `src/lib.rs` line 3089 at
rev 9a2468e). From 0.89.0 a nested pair, where one template holds the literal at every position the
two differ (`/items/current` beside `/items/{item_id}`), is admitted, and the more specific template
answers. Templates that cross stay refused when the binding compiles.

The existing 405 answer is unchanged: a method that no route matching the path serves answers 405,
naming the methods of those routes.

## Acceptance

The fixture is a neutral specification in the spindle-cli test binary with two `ess-http/1`
bindings. The precedence binding declares `GET /things/{id}` before `GET /things/modules` and
`GET /things/actions`. The method binding declares `GET /things/{id}` and `POST /things/modules`,
and no other route whose template carries `/things/modules`.

- `a_literal_segment_wins_over_a_path_variable` (precedence binding): `GET /things/modules` and
  `GET /things/actions` answer their own commands' outcomes. `GET /things/42` answers the by-id
  route.
- `declaration_order_does_not_change_the_match`: the same requests answer the same routes with the
  precedence binding's routes declared in the opposite order.
- `a_method_no_matching_route_serves_answers_405` (method binding): `DELETE /things/modules`
  answers 405 naming both `GET` and `POST`. That method list can only come from both routes, so the
  test pins the existing 405 answer as a regression check.

## Scope

- `crates/spindle-ingress/src/surface.rs`: cited. `Surface::resolve`'s choice among matching routes
  (line 796). The order of `Surface::routes` (line 767) does not change. The stale doc comment of
  `Surface::resolve` (lines 789 to 795) says compiling the binding refused every pair of templates
  a path can both match, so at most one route of a method matches. That stopped being true from
  ESS 0.89.0 and is rewritten.
- `crates/spindle-cli/tests/spindle-cli/route_precedence.rs`: inferred, new.
- `crates/spindle-cli/tests/spindle-cli/main.rs`: inferred. The `mod route_precedence;` line.
- `crates/spindle-cli/tests/fixtures/route-precedence/`: inferred, new.

## Order

- `crates/spindle-ingress/src/surface.rs` is shared with `story:live-topology`, which only reads it.

## Origin

Reported 2026-11-07 by a consumer serving its specification on spindle: two literal routes under a
collection are answered by the collection's by-id route.
