---
format: aep.planning-md/3
id: review-result:w18-u4-adversary-1
kind: review-result
status: active
title: Wave 18 U4 adversary pass 1
relations:
- reviews: story:live-topology
revision: 1
---
needs-revision

```
unit: wave 18 U4 story:live-topology, commit aa961108 on f9c99a67, tree spindle-w18-topology (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed spindle-cli 305→312, ui 56→59, red 4 (2 cargo, 2 vitest)
origin: introduced 3 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_topology.rs` and `ui/src/panel/TopologyPanel.adversary.test.ts`:

- `emits_edges_add_up_to_the_events_a_per_row_set_outcome_appended`: red. `probe.desk.Stamped: the emits edges into it add up to spindle_events_appended_total`, `left: 1.0 right: 3.0`
- `node_ids_are_unique_when_the_system_is_called_ingress`: red. `["ingress", "ingress", "domain:ingress.board", ...]`, `left: 13 right: 12`
- `two_reads_without_traffic_answer_the_same_bytes_but_their_time`: green.
- `the_reserved_hosted_routes_answer_other_methods_and_paths_as_before`: green.
- `after_a_reset_the_routes_and_emits_edges_still_equal_the_metrics`: green.
- `the_topology_read_beside_concurrent_requests_ends_equal_to_the_metrics`: green.
- `the_rally_and_review_topologies_validate_against_the_schema`: green.
- `an empty graph draws an SVG of no negative size`: red. `expected -52 to be greater than or equal to 0`
- `a specification called ingress keeps the ingress's edges on the ingress`: red.
- `a graph of 2000 commands draws every node and edge on finite coordinates`: green.

Suites: `cargo test -p spindle-cli --test spindle-cli`: `FAILED. 309 passed; 3 failed; 5 ignored` (the third, `play::play_stopped_by_a_signal...`, passed alone: a host-load flake). `npx vitest run`: `Tests 2 failed | 57 passed (59)`.

Findings:

- `crates/spindle-ingress/src/observe/topology.rs:198`: an `emits` edge counts outcomes selected, not events appended; a per-row set outcome (ess/25 `each: changed`) appends 3 and the edge says 1, against the module doc at :18 and the story. NEEDS-CHANGE, introduced, blocker.
- `crates/spindle-ingress/src/observe/topology.rs:126`: the specification node takes its bare system id, so a system named `ingress` yields two nodes with id `ingress`; the panel's `Map` (`ui/src/panel/topology.ts:60`) draws `routes` edges from the specification node. CONFIRMED, introduced, note.
- `ui/src/panel/topology.ts:66`: a graph with no nodes gets height -52. INFEASIBLE from the server, introduced, note.
- `crates/spindle-cli/tests/spindle-cli/topology.rs:670`: no test reads a periodic binding's `invokes` edge; returning 0 for fired ticks would stay green. CONFIRMED, introduced, note (mutant not run).

Attacked and not broken: byte-identical reads apart from `at`; methods, query strings, trailing slashes and content types of the reserved routes as at the base; counts after a reset; concurrent writers; schema validation of the rally and review topologies; unrouted 404/405 get no edge; duplicate `routes` edges (ESS-HTTP-005); the ring is newest first; a 2000-command graph.

```findings
- file: crates/spindle-ingress/src/observe/topology.rs
  line: 198
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: an emits edge counts outcomes selected, so a per-row set outcome (ess/25 each changed) shows 1 where spindle_events_appended_total shows 3, against the module doc's add-up claim
- file: crates/spindle-ingress/src/observe/topology.rs
  line: 126
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the specification node takes its bare system id, so a system named ingress yields two nodes with id ingress and the panel draws routes edges from the specification node
- file: ui/src/panel/topology.ts
  line: 66
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a graph with no nodes is drawn with height -52 and a negative viewBox, though the server always sends the ingress and specification nodes
- file: crates/spindle-cli/tests/spindle-cli/topology.rs
  line: 670
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no test reads a periodic binding's invokes edge, so returning 0 for fired ticks would leave the suite green
```
