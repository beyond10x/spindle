---
format: aep.planning-md/3
id: story:refused-bound-command-recorded
kind: story
status: draft
title: A refused bound command is recorded where a run can read it
relations:
- informed_by: story:stdin-run-fires-bindings
revision: 1
---
# A refused bound command is recorded where a run can read it

## Outcome

When a binding's command is refused (a declared refusing outcome, or a kernel rejection), a served run
records it: the bound command, the binding, the refusal's outcome or error code, and the command
that caused it. Today the host reports a failed invocation once on stderr and records nothing a run
can read, so a consumer cannot count or show refused reactions. A request record of
`GET /__spindle/requests` already lists every binding whose input was unmet on the way (`unmet`,
`crates/spindle-ingress/src/observe.rs`); a refused bound command has no such entry.

Where the record lives is decided when scoped:

- A: in the request record of the request that caused it, beside `unmet` (`refused_bound`), and as a
  count in `GET /__spindle/metrics`; the event log stays the events ESS commits.
- B: an entry in the event log (`GET /api/events`), which ESS's interpreter does not write.

## Acceptance

- `a_refused_bound_command_is_listed_with_its_cause`: a binding from `E` to `C` where `C` refuses
  with a declared outcome; the record names `C`, the binding, the outcome and the causing command.
- `a_bound_command_that_runs_adds_no_refusal_entry`.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0 over HTTP: a reaction refused when a balance is unchanged
left no trace a run could count.
