---
format: aep.planning-md/3
id: story:unknown-commit-in-seed-and-retries
kind: story
status: draft
title: A bound command or seed entry whose commit outcome is unknown is not sent again
summary: The bound-command retry and the seed treat StoreError::Unknown as an outcome to check, not a command that did not run.
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A bound command or a seed entry whose commit outcome is unknown (`StoreError::Unknown`, from
`story:unknown-commit-answered-as-unknown`) is not treated as a command that did not run: a bound
command under `on_failure: retry` is not sent again, and a seed entry is not marked rejected and
sent again at the next start. Each records that its outcome is unknown, so an operator can check
before anything is sent twice.

## Why

Line numbers at 18715a80. Found while scoping `story:unknown-commit-answered-as-unknown` (its D4 A
left these paths out). Both are reachable on PostgreSQL, the only store that returns an unknown
commit.

- Bound commands: the retry loop in `crates/spindle-host/src/host.rs:2669-2700` counts any `Err`
  from `execute` as a refusal (`Err(_) => Some(None)`) and, under `on_failure: retry`, sends the
  command again, keeping the last attempt as a dead letter. A commit that may have landed is
  retried as if it had not.
- Seed: `crates/spindle-cli/src/seed.rs:1111` marks an entry whose command failed with any
  `HostError::Execute` as `Rejected`, and a restart sends it again.

## Acceptance

- A host-level case with a stub store whose commit fails `StoreError::Unknown`, on a bound command
  under `on_failure: retry`: the command is attempted once, not retried, and its dead letter (or
  report line) names the outcome as unknown.
- A seed case with the same stub store: the entry is not recorded `Rejected`; the run says its
  outcome is unknown and a restart does not send it again without the operator's word.
- A store error that is not `StoreError::Unknown` keeps today's retry and today's `Rejected`.
- Each case runs in the default test step and ends within 1 s; no PostgreSQL-lane test is added.
