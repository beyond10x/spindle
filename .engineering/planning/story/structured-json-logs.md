---
format: aep.planning-md/3
id: story:structured-json-logs
kind: story
status: draft
title: A served run writes its log as JSON lines on stderr
summary: With a log-format flag every stderr line of a served run is one JSON object with time, level, message and named fields.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:otlp-trace-export
revision: 1
---
# A served run writes its log as JSON lines on stderr

## Outcome

With a log-format flag, every line a served `run` writes on stderr is one JSON object with a
time, a level, a message and the fields it names (component, command, binding, request id), so a
log collector reads it without parsing prose.

## Why

A served run's stderr is free text written with `eprintln!` (11 calls in
`crates/spindle-cli/src/serve.rs`, 8 in `main.rs`, 4 in `seed.rs`; for one,
`serve.rs:467-470`, a dropped binding command). A consumer's log collector cannot tell a refusal
from a start line without matching wording. Traces are `story:otlp-trace-export`; this story is
the log only.

## Acceptance

- `every_stderr_line_is_one_json_object`: a served run with the JSON format, sent a command, a
  refused command and a SIGTERM, writes only lines that each parse as one JSON object with `time`,
  `level` and `message`.
- `a_dropped_binding_command_names_its_fields`: the line for a dropped command carries the
  component, command, binding and attempts as fields.
- `text_stays_the_default`: without the flag, stderr is as today.

## Decided when scoped

- Field names, and whether each answered request writes a line.
