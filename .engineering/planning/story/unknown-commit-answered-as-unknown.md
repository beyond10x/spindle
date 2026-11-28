---
format: aep.planning-md/3
id: story:unknown-commit-answered-as-unknown
kind: story
status: draft
title: A command whose commit outcome is unknown is not answered committed false
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A command whose commit outcome is unknown, because the store's deadline fired after COMMIT was sent,
is not answered `committed: false`: the answer says the outcome is unknown, so a caller does not
send again a write that may have landed.

## Why

Wave 33's U2 adversary (round 1, `story:new-leader-write-while-old-paused`) found that eventlog's
`UnknownCommit` maps to `StoreError::Backend` (`crates/spindle-kernel/src/sqlite.rs`, `backend`), and
the ingress answers it `501` with `committed: false` (`crates/spindle-ingress/src/answer.rs`). eventlog
returns `UnknownCommit` once COMMIT was sent and the deadline fired, so the write may have landed.
Present before wave 33. The publication-lock window of that story does not reach it: those writes fail
before COMMIT.

## Acceptance

- A stub store whose commit fails with the unknown-commit error: the command's answer does not say
  `committed: false`, and names the outcome as unknown, with its status written in
  `website/docs/concepts/capabilities.md`.
- A store failure before COMMIT still answers as it does today.
