---
format: aep.planning-md/3
id: story:live-topology
kind: story
status: implemented
title: An observed run shows its topology with live numbers
relations:
- serves: vision:spindle
- decomposes: epic:runtime-observability
- depends_on: story:observe-ingress-model
- depends_on: story:ingress-request-log
- depends_on: story:ingress-prometheus-metrics
- depends_on: story:ingress-observe-panel
- depends_on: story:external-answers-observed
scope:
- confidence: inferred
  path: crates/spindle-cli/tests/observe.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/observe/topology.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: ess/fixtures/observe/topology
- confidence: cited
  path: examples/rally/ess
- confidence: cited
  path: ui/src/lib/topology.ts
- confidence: inferred
  path: ui/src/panel/TopologyPanel.vue
revision: 14
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 12, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "proposed", to: "active", at: "2026-11-06T14:45:47Z", actor: "human:timo", revision: 13, decided_on: {"recorded":{"review_outcome":1}}}
- {from: "active", to: "implemented", at: "2026-11-10T14:42:22Z", actor: "human:timo", revision: 14, decided_on: {"recorded":{"test_result":1,"review_outcome":4,"verification":1}}}
---
# An observed run shows its topology with live numbers

## Outcome

With `--observe`, `GET /__spindle/topology` answers a `spindle.observe.Topology`
(`spindle-topology/1`). The graph is derived from the loaded specifications, never authored:
the ingress at the top; each served specification and its domains; their commands and views;
each external cause of a command's outcome; each event a command emits; each binding an event
travels and the command it invokes. Edges are `routes`, `contains`, `asks`, `emits`, `carries`
and `invokes`.

Each edge except `contains` carries live numbers, read from the same counters as
`/__spindle/metrics` and the same ring as `/__spindle/requests`. A `routes` edge has requests and
the status-class mix, plus the sequences of its latest five requests still in the ring. An `asks`
edge has the count per answer source (decision table, file, default, unanswered). An `emits` edge
has events emitted. A `carries` edge has envelopes sent. An `invokes` edge has commands invoked.

`/__spindle/ui` gains a topology screen that draws the graph top-down, ingress first, as SVG with
no chart library (the observe design's rule, § 6), and refreshes the numbers by polling. A host
can embed the page or redraw from the JSON.

Within one specification, a binding's `invokes` edge already exists. The rally example's event
bindings invoke the players' commands in process. With several specifications behind one ingress
(`story:one-ingress-several-specifications`), the graph holds every one of them and the `invokes`
edges between them. Until that story lands, a run serves one specification and the graph holds
one.

Without the flag the route answers `404`.

## Acceptance

- `without_observe_the_topology_answers_404`.
- `the_topology_is_derived_from_the_specification`: for the `ingress` test specification the
  nodes and the edges other than live numbers equal `ess/fixtures/observe/topology/tasks.json`;
  adding a command to the specification adds its node and its `routes` and `contains` edges with
  no other change.
- `every_served_route_has_one_routes_edge`: one per route of the surface; an unserved (`501`)
  route has its edge too.
- `routes_edges_count_a_scripted_sequence`: after the sequence of
  `the_request_log_lists_a_scripted_sequence_newest_first`, each `routes` edge's count and
  status mix equal the matching `spindle_http_requests_total` samples, and its `latest` names
  records that `/__spindle/requests` holds.
- `asks_edges_count_answers_by_source`: equal to `spindle_external_causes_total`.
- `emits_and_carries_edges_follow_the_counters`: equal to `spindle_events_appended_total` and
  `spindle_bindings_total{carried="sent"}`, on the `rally` example served by `run` in one process.
- `invokes_edges_follow_the_counters`: on the `rally` example, a single specification, each event
  binding has one `invokes` edge from its binding node to the command it invokes. After the match
  is opened and ticked, each edge's count equals that binding's
  `spindle_bindings_total{carried="delivered"}` sample, and it grows with the next tick.
- `the_topology_reads_as_its_specified_type`: deserialises into
  `spindle_observe_api::Topology` and validates against its schema.
- `the topology screen draws the ingress first and one edge per routes edge` (vitest, against
  the fixture).
- `the topology screen shows each edge's status mix and latest calls` (vitest).
- Pending `story:one-ingress-several-specifications`:
  `a_topology_spans_every_specification_behind_one_ingress`, with an `invokes` edge from one
  specification's binding to the other's command. Written as `#[ignore]` with that story's
  id until it lands.

## Scope

- `crates/spindle-ingress/src/observe/topology.rs` — inferred, new: the graph from the IR and the
  surface, the live numbers from the recorder and the counters.
- `crates/spindle-ingress/src/observe.rs` — inferred: the `topology` arm (one line, shared).
- `crates/spindle-ingress/src/surface.rs` — cited: the routes per operation (read only).
- `crates/spindle-host/src/host.rs` — cited: counters and bindings per component (read only).
- `ui/src/panel/TopologyPanel.vue` — inferred, new; `ui/src/lib/topology.ts` — cited: deploy's
  topology layout, reused if it fits a top-down ingress graph, else left alone.
- `crates/spindle-cli/tests/observe.rs` — inferred.
- `examples/rally/ess` — cited, read only: the single specification with event bindings that the
  `emits`, `carries` and `invokes` tests serve.
- `ess/fixtures/observe/topology/` — inferred: drafted with `story:observe-ingress-model`.

## Order

Last. After `story:ingress-observe-panel` (the page it extends), `story:ingress-prometheus-metrics`
and `story:external-answers-observed` (the numbers it reads). The multi-specification acceptance
waits for `story:one-ingress-several-specifications` (draft); this story does not depend on it.

## Origin

The operator, 2026-11-06, relayed by the coordinator (condensed): a live topology and data-flow
view, from the ingress/router down through each served specification, the external causes and
how they were answered, and the emitted events and their channels. Edges carry live numbers
(requests, status mix, latest calls, events per command). The static graph comes from the loaded
specifications, the numbers from the same counters as the metrics and request routes. It is
served in the embeddable panel, with the raw graph as JSON at `/__spindle/topology`, and spans
several specifications behind one ingress. Same opt-in flag.
