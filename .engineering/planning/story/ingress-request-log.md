---
format: aep.planning-md/3
id: story:ingress-request-log
kind: story
status: active
title: An observed run lists the requests it answered
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability
- depends_on: story:observe-ingress-model
- depends_on: story:http-router-is-transport-free
- depends_on: story:outcome-scoped-refusal
- depends_on: story:fixed-caller-per-grant
scope:
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/observe.rs
- confidence: cited
  path: website
revision: 15
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:46Z", actor: "human:timo", revision: 14, decided_on: {"recorded":{"review_outcome":3}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:46Z", actor: "human:timo", revision: 15, decided_on: {"recorded":{"review_outcome":3}}}
---
# An observed run lists the requests it answered

## Outcome

`b10x-spindle run --observe [--observe-requests <N>]` (N from 1 to 10 000, default 256; only where
`run` serves HTTP) records every request its listener answers outside `/__spindle/`, including
those answered `404`, `405` and `400` on the listener thread, and answers
`GET /__spindle/requests` with a `spindle.observe.RequestLog`: the totals per class since start
and the last N records, newest first. `?class=<class>` (repeatable) keeps only those classes;
`?class=not-served` is the 501/unanswered filter. The `reason` of a record is the answer's
`refused` text byte for byte. The JSON line `run` prints on stdout once listening is unchanged by
the flag.

A record's `duration_us` is measured on the listener thread. It runs from the moment the
request's head has been read to the moment the answer is handed back to the connection, so the
time a request waits for the host's thread behind another request is included. A request answered
on the listener thread (`404`, `405`, `400`) is timed the same way.

A record's class is read from the answer `answer.rs` builds: its status and its `refused` text
(`Answer::refusal`, `Answer::not_granted`, `Answer::unfinished`). The class rules are the epic's.

The reserved prefix is laid out here for all of the epic's routes: `/__spindle/requests` is
answered; `/__spindle/metrics`, `/__spindle/ui` and `/__spindle/topology` answer `404` until their
stories land. At start, a specification route whose first literal segment is `__spindle` is
refused with `SurfaceError::Reserved`, with or without the flag. On every request, flag on or
off, `Surface::resolve` matches the reserved prefix before any specification route. No path under
`/__spindle/` reaches a specification route, and with the flag off it answers `404` (coordinator
decision 5, 2026-11-06). Without the flag the surface holds no recorder.

The record's `externals` stays empty here; `story:external-answers-observed` fills it.

## Acceptance

- `without_observe_the_reserved_routes_answer_404`: `/__spindle/requests`, `/metrics`, `/ui` and
  `/topology` answer `404`.
- `the_listening_line_is_unchanged_by_observe`: the stdout listening line has the same keys with
  and without `--observe`.
- `a_surface_without_observe_holds_no_recorder` (unit, `spindle-ingress`).
- `a_specification_route_under_the_reserved_prefix_is_refused_at_start`: a binding routing a
  command at `/__spindle/x` exits 1 before listening, naming the operation, method, path and
  `/__spindle/`, both with and without `--observe`.
- `a_variable_only_route_never_answers_under_the_reserved_prefix`: with a binding routing a view
  at `GET /{a}/{b}` and the flag off, `GET /__spindle/metrics` answers `404` and records nothing,
  while `GET /x/y` reaches the view. With the flag on, `GET /__spindle/requests` answers the
  request log.
- `the_request_log_lists_a_scripted_sequence_newest_first`: on the `ingress` test specification,
  `PUT /tasks/t1`, `POST /tasks/t1/close` twice, `GET /tasks`, `PUT /tasks/t2` with an undeclared
  field, `GET /nowhere`, `DELETE /tasks` give the classes, statuses, routes, operations, outcomes
  and events of `ess/fixtures/observe/requests/scripted.json` (times and durations aside).
- `a_request_is_classed_by_its_answer` (unit): 201 and a declared 410 are `served`, 400 is
  `refused`, 403 "not granted" is `not-granted`, 501 is `not-served`, 500 is `failed`, 404 and
  405 are `unrouted`.
- `a_request_held_on_the_host_thread_is_timed_with_its_wait` (`spindle-ingress` integration):
  through `Listener` with a recorder, the test takes the `Exchange` from `Listener::next` and
  waits 50 ms before it responds. The record's `duration_us` is at least 50 000. A `404` answered
  on the listener thread is recorded with a `duration_us` below that.
- `the_request_log_keeps_the_refusal_text`: for a 400, a 403 and a 501, `reason` equals the
  answer body's `refused`.
- `the_not_served_filter_lists_only_501_answers`.
- `the_ring_drops_the_oldest_beyond_its_capacity`: with `--observe-requests 2`, three requests
  give two records and `recorded` 3.
- `reserved_requests_are_neither_recorded_nor_counted`.
- `the_request_log_reads_as_its_specified_type`: the answer deserialises into
  `spindle_observe_api::RequestLog` and validates against its schema.

## Scope

- `crates/spindle-ingress/src/surface.rs` — cited: `SurfaceError::Reserved`, the reserved check
  beside `with_console`, `resolve` returning a reserved route.
- `crates/spindle-ingress/src/listener.rs` — cited: recording at `NotFound`, `WrongMethod`,
  `Malformed` and around `Exchange`; the clock read at the head read and at the answer handed
  back, only when a recorder exists. If `story:http-router-is-transport-free` moves routing into a
  transport-free function first, the classification moves there and the clock stays in the
  listener (inferred).
- `crates/spindle-ingress/src/answer.rs` — cited, read: `Answer`, `refusal`, `not_granted`,
  `unfinished`, the status and text a record is classed by.
- `crates/spindle-ingress/src/observe.rs` — inferred, new: recorder, ring, totals, classification,
  the reserved-route dispatch with all four arms.
- `crates/spindle-ingress/src/lib.rs` — cited: crate docs and exports.
- `crates/spindle-ingress/tests/observe.rs` — inferred, new: the timing test against `Listener`.
- `crates/spindle-cli/src/cli.rs`, `serve.rs` — cited: the two flags and `Serving`.
- `crates/spindle-cli/tests/observe.rs` — inferred; under `tests/spindle-cli/` if
  `story:one-test-binary-per-crate` lands first.
- `README.md`, generated CLI pages under `website/` (`task docs-generate`) — cited.

## Order

After `story:observe-ingress-model`. After `story:http-router-is-transport-free` (active, scope
`crates/spindle-ingress`), which reshapes the files this story records in. After
`story:outcome-scoped-refusal` (active), which edits `surface.rs` and `answer.rs` and changes
when a route answers `501`. After `story:fixed-caller-per-grant` (implemented in wave 10), which
edits `cli.rs`, `serve.rs` and `surface.rs`. Before `story:ingress-prometheus-metrics`,
`story:ingress-observe-panel`, `story:live-topology` and `story:external-answers-observed`.

## Origin

Consumer asks (1), (3) and (5), and the reserved-prefix decision, 2026-11-06, condensed (see the
epic).
