---
format: aep.planning-md/3
id: story:several-writers-one-postgres
kind: story
status: proposed
title: Several processes of one component serve on one PostgreSQL store
summary: Two or more run processes on one PostgreSQL store lose no write, apply none twice, and keep serving when one is killed.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- depends_on: story:request-id-idempotency
- informed_by: epic:split-run
- informed_by: story:kernel-row-set-commit-conflict
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
---
# Several processes of one component serve on one PostgreSQL store

## Outcome

Two or more `run` processes of one specification serve on one PostgreSQL store at once: a request
reaches any of them, no write is lost or applied twice, and when one is killed the others keep
answering.

## Why

- A store has one writer: the host counts only the writes it lands, and "a second writer on the
  same store ... moves its rows without moving these" (`crates/spindle-host/src/host.rs:1341-1344`).
- Only SQLite state is locked against a second process (`story:state-directory-lock`); PostgreSQL
  has no lock and no lease (`crates/spindle-kernel/src/postgres.rs`).
- `epic:split-run` puts high availability out of scope, and the design names ownership leases with
  fencing as M5 (`docs/design/spindle.md:336-338`, `docs/design/spindle.md:454`).
- A consumer's service in production runs more than one replica.

## Acceptance

- `two_processes_write_one_store_without_loss`: two processes on one PostgreSQL store, each sent
  half of a set of commands, leave the rows one process leaves after the whole set.
- `a_row_set_guard_holds_across_processes`: a guard over a row set refuses the second of two
  conflicting commands sent to two processes (`story:kernel-row-set-commit-conflict` across
  processes).
- `a_killed_process_leaves_the_other_serving`: SIGKILL of one process; the other answers every
  following request.
- `a_retried_request_lands_once`: a request id sent to both processes writes once
  (`story:request-id-idempotency`).

## Decided when scoped

- Every replica writing, or one leader holding a lease with the others standing by; and what each
  replica's views, counters and published events read.

## Architecture decision

Option A, picked by the operator on 2026-11-27: one leader holds a lease with a fencing token in a
new lease table; standbys answer 503 on `/__spindle/ready`. Replicas serve failover; the rule of
one writer per store stays.

Not taken: B (every replica writes, with guards across processes, optimistic streams and request-id
records), C (defer).
