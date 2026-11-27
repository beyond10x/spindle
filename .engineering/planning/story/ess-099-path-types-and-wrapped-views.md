---
format: aep.planning-md/3
id: story:ess-099-path-types-and-wrapped-views
kind: story
status: implemented
title: Path variables of ESS 0.99 types, and view answers under one member, are served
summary: 'run --http serves a path variable of a Uuid, Boolean, Decimal, Timestamp or Duration source or an unconstrained newtype of one, and a view body {<member>: array} or {<member>: single}'
relations:
- serves: vision:spindle
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T03:20:51Z", actor: "human:operator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-25T03:20:54Z", actor: "human:operator", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-27T07:11:14Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Path variables of ESS 0.99 types, and view answers under one member, are served

## Outcome

A consumer's specification repositories bind two `ess-http/1` shapes that ESS 0.98.0 refused and
ESS 0.99.0 accepts (beyond10x/ess#755). `run --http` serves both as ESS's generated servers
answer them:

1. A route path variable `{id}` whose source is a `Boolean`, `Decimal`, `Timestamp`, `Duration`
   or `Uuid`, or an unconstrained newtype of one (an id type: a newtype of `Uuid`, or of
   `String`), on a command route and on a view route. The segment is percent-decoded and read as
   the field's type, as ESS's servers read a `query` value; a segment the type refuses answers
   `400` (`malformed`). A constrained newtype (a prefix or an alphabet) stays refused by ESS with
   `ESS-HTTP-009`, and `run --http` reports that refusal and serves nothing.
2. A view route whose `body` is `{<member>: array}` or `{<member>: single}`, which PR 78 named not
   served: `array` answers the rows as `{"<member>": [...]}`, `single` the one row as
   `{"<member>": {...}}`, and a `single` view holding no row answers its `empty` status and body,
   unwrapped, as ESS's generated server does. The bare `array` and `single` forms are served the
   same way without the member.

## Acceptance

- `binding_shapes::a_newtype_of_uuid_id_is_read_from_the_path_of_a_command_and_a_view_route`
  (spindle-ingress): with a newtype of `Uuid` bound as `{id}` on `GET /lockers/{id}` and
  `PUT /lockers/{id}/touch`, a request with a UUID reads and writes the locker it names; a segment
  that is no UUID answers `400` with `refused`; each view answer validates against the OpenAPI
  document ESS 0.99.0 projects for the binding.
- `binding_shapes::a_path_variable_of_each_admitted_type_is_read_as_the_type`: for each of a
  newtype of `String`, `Uuid`, `Boolean`, `Decimal`, `Timestamp` and `Duration`, a command route
  and a view route carrying it in the path answer `200` for a value of the type and `400` for text
  ESS's generated servers refuse (a `Duration` and a `String` refuse no text).
- `binding_shapes::a_wrapped_view_answers_its_rows_or_its_one_row_under_its_member`: `{lockers:
  array}`, `{locker: single}`, bare `array` and bare `single` answer the bytes ESS's generated
  server writes, the empty `single` its declared status, each validated against the projected
  OpenAPI document; none is named not served.
- `binding_shapes::a_constrained_newtype_stays_refused_in_the_path_as_ess_refuses_it`
  (spindle-ingress) and `binding_shapes::run_refuses_a_constrained_newtype_in_the_path_as_ess_does`
  (spindle-cli): ESS refuses a newtype with a prefix and an alphabet in the path with
  `ESS-HTTP-009` "which a path segment cannot carry"; `run --http` prints it and exits 1.
- `binding_shapes::run_serves_a_uuid_id_in_the_path_and_views_under_one_member` (spindle-cli):
  the same shapes answered over the loopback listener.
- `CHANGELOG.md` `[Unreleased]` and the capabilities page say what is served; `task docs-check`
  passes.

## Origin

Goal story for a consumer whose three specification repositories wait for these shapes (ESS
0.99.0, beyond10x/ess#755). Spindle on `main` (PR 78) names a view answered under one member not
served.
