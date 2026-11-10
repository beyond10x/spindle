---
format: aep.planning-md/3
id: review-result:w18-u4-adversary-2
kind: review-result
status: active
title: Wave 18 U4 adversary pass 2
relations:
- reviews: story:live-topology
revision: 1
---
needs-revision

```
unit: wave 18 U4 story:live-topology, commits aa961108 + 2a5c8859 on branch unit/w18-topology (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed spindle-cli 312→314, red 2
origin: introduced 1 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_topology_emitted.rs`, each serving a `bell` specification with `--observe` and comparing the emits edges into each event with `spindle_events_appended_total{event}`:

- `an_outcome_without_a_subject_adds_to_the_emits_edge_what_the_appended_counter_adds`: red, `bell.desk.Rang`, `left: 1.0 right: 0.0`.
- `a_set_outcome_over_no_row_adds_to_the_emits_edge_what_the_appended_counter_adds`: red, `bell.desk.Swept`, `left: 2.0 right: 1.0`.

Suite: `cargo test -p spindle-cli --test spindle-cli`: `FAILED. 311 passed; 3 failed; 5 ignored` (the third is the `play` signal-timing flake, green alone).

Finding: `crates/spindle-host/src/host.rs:1380`. `Host::emitted` counts every event of a successful execute; the appended counter counts only the events a store commit carries (`crates/spindle-telemetry/src/kernel.rs` committed/removed). An outcome without a subject (`crates/spindle-kernel/src/execute.rs:1327`) and a set outcome over no row (`crates/spindle-kernel/src/execute/set.rs:35`) return events no store records. The `Host::emitted` doc, the `observe/topology.rs:18` doc and the story's acceptance claim equality. Reached by fixtures in the repository (`route-precedence`, `error-object-bodies`). NEEDS-CHANGE, introduced, blocker.

Attacked and not broken: kept across a reset; refused, undecided, store conflicts and failed commits never reach the increment; retries count only the committing attempt; timer-fired commands and cross-component deliveries count as direct calls; keys bounded by commands × events; written and read on the host thread only; the `specification:` prefix is consistent across the specification, generated types, design doc and fixture, which validates.

```findings
- file: crates/spindle-host/src/host.rs
  line: 1380
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: Host::emitted counts the events of every successful execute, including those of a subject-less outcome and of a set outcome over zero rows that no store commits, so an emits edge exceeds spindle_events_appended_total (1 vs 0, 2 vs 1) against the story's acceptance and the Host::emitted and topology.rs docs
```
