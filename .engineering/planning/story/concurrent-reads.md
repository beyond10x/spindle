---
format: aep.planning-md/3
id: story:concurrent-reads
kind: story
status: implemented
title: Reads are answered while a command runs
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T01:48:13Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T04:46:07Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T07:37:33Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"review_outcome":1}}}
---
# Reads are answered while a command runs

## Outcome

On `run` serving HTTP, view reads, `GET /api/*` and `GET /__spindle/*` are answered concurrently
with each other and with a running command, from the state as of the last committed write. Commands
stay one at a time per component, in arrival order, as the single-writer design requires.

## Why

The operator called it a defect on 2026-11-11, relayed by a downstream consumer: during one slow
request (a 42 s metrics read, `story:metrics-read-without-scan`) the readiness probe
`GET /api/model` and view reads got no answer.

Read from the code at 0.12.0:

- `Listener::serve` (crates/spindle-ingress/src/listener.rs ~239) runs the HTTP side on one
  thread, `spindle-ingress`, with a current-thread tokio runtime; each connection is a task there,
  so accepting and parsing are concurrent.
- Every routed request becomes an `Exchange` sent over a channel to the main thread, where
  `serve` (crates/spindle-cli/src/serve.rs ~405) takes one at a time and runs it with
  `&mut host`. So every request that reads the host, a view read included, waits for the one
  before it.
- The design (docs/design/spindle.md § 4) runs each stateful cell as a single-threaded loop: one
  writer per account. Nothing in the design asks for reads to wait behind that writer.

## Acceptance

1. While a command runs and holds the writer for at least 1 s (a test hook or a slow external
   call), a view read, `GET /api/model` and `GET /__spindle/requests` are each answered within
   100 ms, with the state as of the last committed write.
2. Two slow reads run at once: each answers in about its own time, not the sum.
3. Commands to one component still run one at a time in arrival order; the existing ordering and
   conformance tests pass unchanged (79 of 79).
4. A read never sees a partly applied command: a property test over drawn command sequences with
   concurrent reads finds every read equal to the state after some prefix of the commands.
5. Memory and SQLite stores; the PostgreSQL store when it lands.
