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
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/log.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/json_logs.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
revision: 11
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

- `every_stderr_line_is_one_json_object`: a served run with `--log-format json`, sent a command, a
  refused command and a SIGTERM, writes only lines that each parse as one JSON object with `time`,
  `level` and `message`.
- `a_dropped_binding_command_names_its_fields`: the line for a dropped command carries `component`,
  `command`, `binding`, `attempts` and `error` as fields.
- `a_seed_entry_line_names_its_request_id`: on a served run with a seed, the line for a refused seed
  entry carries the entry's `request_id`.
- `json_logs_on_a_stdin_run_exit_1`: `--log-format json` on a run that does not serve HTTP exits 1
  before reading stdin.
- `text_stays_the_default`: without the flag, the drop line equals today's text.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `run --log-format <text|json>`, default `text`; one flat object per line
  with `time` (RFC 3339 UTC), `level` (`info`, `warn`, `error`), `message` and named fields at the
  top level (`component`, `command`, `binding`, `attempts`, `error`, `request_id`).
- D2 A: `--log-format json` on a run that does not serve HTTP exits 1 before
  reading stdin, as `--observe` and `--seed` refuse there.
- D3 A: no schema marker; the field names are documented in the flag's help.
- D4 A: no line per answered request; only the lines written today become JSON.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` — cited
- **Files:** `crates/spindle-cli/src/serve.rs` — cited; all 11 `eprintln!` (58, 65, 286, 309, 317,
  357, 373, 380, 420, 473, 534; the dropped command is 524-537, not 467-470 as the story says)
- **Files:** `crates/spindle-cli/src/seed.rs` — cited; 14 `eprintln!` (210-757), not 4, all on the
  served path (`begin`, `forget`, `Seeding`, `downstream`)
- **Files:** `crates/spindle-cli/src/main.rs` — cited; `main` at 67 (the exit error, which needs the
  format read from `Cli` before `run(cli)` consumes it), the non-loopback refusal at 505, the
  `Command::Run` destructure at 198-235 and `Execute`; 11 `eprintln!`, not 8, and the other 9
  (admit 178/182, play 387/393, stdin 701/717/822-830) are off the served path
- **Files:** `crates/spindle-cli/src/cli.rs` — cited; one new field on `Command::Run` (220-380)
- **Also likely:** `crates/spindle-cli/src/log.rs` — inferred; a new module holding the format set
  once at start and one writer for both formats, declared in `main.rs`
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/json_logs.rs` and
  `crates/spindle-cli/tests/spindle-cli/main.rs` (`mod json_logs;`) — inferred; the 3 acceptance
  tests, reusing the fixture `crates/spindle-cli/tests/fixtures/retried` that `unserved.rs:463-480`
  already drives to "did not run … after 5 attempts"
- **Symbols:** `serve::serve`, `serve::Serving`, `serve::binding`, `seed::begin`, `seed::forget`,
  `seed::Seeding`, `Execute`, `Command::Run`, `spindle_host::Failed` (read only: `component`,
  `command`, `cause`, `attempts`, `error`), `ess_primitives::Rfc3339Instant::from_epoch_millis` —
  cited
- **Documents:** the `run` flag help in `cli.rs`; nothing outside the crate — inferred
- **Confidence:** high for `serve.rs`, `seed.rs` and `main.rs`, because every stderr write was
  grepped; medium for the new module and test file names, which are a design reading
- **Would collide with:** any unit that adds a `run` flag (`cli.rs` `Command::Run`, the `main.rs`
  destructure and `Execute`, `serve::Serving`) or edits the serve loop in `serve.rs` (signal exit,
  console reset, failed-binding report) or the stderr lines of `seed.rs`; and every unit that adds
  a module to `crates/spindle-cli/tests/spindle-cli/main.rs`
- **Safety fact:** in the non-test sources of `spindle-host`, `spindle-ingress` and `spindle-kernel`
  there is no `eprintln!`/`eprint!` and no logging crate, so JSON mode only has to cover the
  `spindle-cli` call sites listed above; the text path stays byte for byte as today if each
  replaced `eprintln!` keeps its current wording in text mode — step 2 (`git grep 'eprint'` over
  `crates/*/src`), unproven
