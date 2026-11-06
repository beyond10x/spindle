---
format: aep.planning-md/3
id: story:served-fixed-caller
kind: story
status: active
title: Served commands that read the caller run with a caller fixed at start
relations:
- serves: vision:spindle
- depends_on: story:actor-command-grants
scope:
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-06T00:36:51Z", actor: "agent:b10x-chef-orchestrator", revision: 5}
---
# Served commands that read the caller run with a caller fixed at start

## Outcome

`run` serving `ingress.network` accepts an explicit opt-in flag that fixes, for the whole run, the
actor and caller attributes every HTTP command is sent as (`Host::submit_as`). A command that
reads its caller is then served instead of being marked unserved. Today
`crates/spindle-ingress/src/surface.rs` (`served`) marks every such command unserved with "the
caller attributes it reads (...), which no HTTP caller proves here", and
`crates/spindle-ingress/src/answer.rs` sends every command with `host.submit`, as no caller. How
an HTTP caller proves its attributes stays undecided (design §11); this is a test-backend switch,
not authentication.

## Acceptance

- `a_command_reading_the_caller_runs_with_the_fixed_caller` — with the flag, a command guarded on
  `caller.<attribute>` runs and its guard reads the fixed value.
- `without_the_flag_nothing_changes` — no default: without the flag, the same command is still
  answered `501` naming the caller attributes it reads.
- `the_fixed_caller_is_printed_on_stderr_at_startup` — with the flag, stderr names the actor and
  every attribute fixed, before the first request is served.
- `the_fixed_caller_does_not_bypass_the_grant_check` — with the flag fixing an actor whose `may:`
  does not name a command, a request for that command is refused "not granted", as
  `story:actor-command-grants` refuses it, and nothing is written.
- `a_fixed_caller_missing_a_required_attribute_is_refused_at_start` — the run exits non-zero
  before listening, naming the attribute.

## Origin

Requested by a consumer that serves a specification over HTTP in memory as a test backend for a
browser UI suite, 2026-11-06.

## Scope

- `crates/spindle-ingress/src/surface.rs` — cited: the caller-attributes refusal in `served`.
- `crates/spindle-ingress/src/answer.rs` — cited: `host.submit` with no caller.
- `crates/spindle-cli/src/cli.rs` — inferred: the opt-in flag.
- `crates/spindle-cli/src/serve.rs` — inferred: validating the caller and printing it at startup.
