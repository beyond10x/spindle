---
format: aep.planning-md/3
id: story:stdin-run-fires-bindings
kind: story
status: draft
title: A stdin run fires the bindings its commands' events match
relations:
- serves: vision:spindle
revision: 2
---
# A stdin run fires the bindings its commands' events match

## Outcome

A stdin `run` fires the specification's bindings: when a line's command emits an event that a
binding maps to a system-issued command, the bound command runs in the same step, as a served run
runs it, and its answer is printed after the line's own answer. Today a stdin run prints "run
reading stdin carries no binding" (`crates/spindle-cli/src/main.rs:649`) and fires none, so a
specification that wires a reaction as a binding cannot be checked with stdin lines; bindings fire
only when Spindle serves HTTP, which needs a component `reached_by: network`.

## Acceptance

- `a_stdin_line_fires_the_binding_its_event_matches`: a specification with one binding from event
  `E` to a system command `C`; a stdin line whose command emits `E` prints its answer, then `C`'s
  answer, marked as caused by that line, and `C`'s write is visible to the next line.
- `a_bound_command_refused_is_printed_and_does_not_fail_the_line`: `C` answering a declared
  refusing outcome prints that outcome; the run exits 0 (story:run-fails-on-rejected-line counts a
  rejected bound command as a rejection).
- `a_stdin_run_and_a_served_run_fire_the_same_bindings`: one sequence of commands sent on stdin and
  over HTTP leaves the same rows and the same event log.
- `a_specification_without_bindings_prints_as_today`: no extra line.

## Not covered

- Periodic bindings in a stdin run (no `run` mode fires them; story:clock-and-mint-in-every-mode).
- Bindings across processes of a split run.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: its gate and a simulation drive Spindle on stdin and
wire reactions as bindings.
