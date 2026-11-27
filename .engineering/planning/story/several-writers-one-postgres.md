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
- informed_by: epic:split-run
- informed_by: story:kernel-row-set-commit-conflict
- informed_by: story:request-id-idempotency
- depends_on: story:spec-change-over-stored-rows
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/database.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-ingress/src/health.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/health.rs
- confidence: cited
  path: crates/spindle-kernel/src/postgres.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: inferred
  path: crates/spindle-observe-api/source.schema.json
- confidence: inferred
  path: crates/spindle-observe-api/types-report.json
- confidence: inferred
  path: crates/spindle-observe-api/types.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: inferred
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/generated/jsonschema/schema/types/spindle.observe.Readiness.schema.json
- confidence: inferred
  path: ui/src/api/generated/source.schema.json
- confidence: inferred
  path: ui/src/api/generated/types-report.json
- confidence: inferred
  path: ui/src/api/generated/types.ts
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/concepts/observe.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
---
# Several processes of one component serve on one PostgreSQL store

## Outcome

Two or more `run` processes of one specification start on one PostgreSQL store at once. One of
them leads: it holds a lease with a fencing token and is the store's one writer. The others stand
by: each answers `/__spindle/live` `200`, `/__spindle/ready` `503` with `standby: true` and every
other request `503`, and writes nothing. When the leader is killed, a standby takes the lease
within the lease time and answers every later request; a leader paused past its lease lands no
write once it resumes, and exits. No write is lost or applied twice across a failover.

## Why

- A store has one writer: the host counts only the writes it lands, and "a second writer on the
  same store ... moves its rows without moving these" (`crates/spindle-host/src/host.rs:1341-1344`).
- Only SQLite state is locked against a second process (`story:state-directory-lock`); PostgreSQL
  has no lock and no lease (`crates/spindle-kernel/src/postgres.rs`).
- `epic:split-run` puts high availability out of scope, and the design names ownership leases with
  fencing as M5 (`docs/design/spindle.md:336-338`, `docs/design/spindle.md:454`).
- A consumer's service in production runs more than one replica.

## Acceptance

- `a_commit_under_a_taken_lease_is_refused` (`spindle-kernel` `postgres.rs`, PostgreSQL lane):
  handle A holds the lease, handle B takes it, and A's commit is refused by name with the rows
  unchanged.
- `a_second_process_stands_by` (`spindle-cli` `postgres/replicas.rs`, PostgreSQL lane): a second
  `run` reads ready `503` with `standby: true`, answers a command `503` and writes nothing.
- `a_killed_leader_leaves_the_standby_serving` (same): after SIGKILL of the leader, the standby
  reads ready `200` within the lease plus a bound and answers every later request.
- `no_write_is_lost_across_a_failover` (same): half of a set of commands go to the leader, the
  leader is killed, the other half go to the new leader; the rows equal those one process leaves
  after the whole set.
- `a_paused_leader_is_fenced` (same): the leader is stopped (SIGSTOP) past the lease, the standby
  takes over and writes, then the old leader resumes (SIGCONT): it lands nothing and exits 1, and
  the rows are as the new leader left them.
- `a_standby_posts_no_webhook_and_keeps_no_dead_letter` (same): with an egress binding, a standby
  posts nothing and keeps no dead letter until it leads (`story:event-egress-webhook`,
  `story:dead-letters-kept-and-replayed`; this unit merges after both).
- `a_standby_reads_not_ready` (`spindle-ingress` `health.rs`, default step).
- `a_malformed_lease_setting_is_refused_before_connecting` (`spindle-cli` `postgres.rs`, default
  step; that file's refusals run with no database).

`database_lease` is 2 s in the PostgreSQL tests. `a_retried_request_lands_once` moved to
`story:request-id-idempotency` (D8 A).

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: the fence is an eventlog guard in the append transaction (`append_group_guarded`) that
  reads the lease row as an inline read model; no upstream change. If that does not work, the
  fallback is C (a new eventlog tag whose append takes a SQL precondition), not B (a token checked
  on another connection, which a paused leader slips past).
- D2 A: one lease per component's tables (`spindle_<component>_lease`), taken in name order; a
  process leads once it holds all of them.
- D3 A: a standby binds the listener, answers `live` `200`, `ready` and every other request `503`,
  and opens no store; once it leads it opens the stores, builds the host and seeds.
- D4 A: a leader that loses its lease answers queued requests `503` and exits 1 naming the lease;
  its supervisor restarts it.
- D5 A: the seed runs only under the first token of the lease; on a takeover it is skipped with a
  log line.
- D6 A: `spindle.observe.Readiness` gains `standby: Boolean`, and the types are regenerated.
- D7 A: a `database_lease` setting in seconds, default 10, renewed at a third of that, expiry
  judged by the database's clock.
- D8 A: `a_retried_request_lands_once` moves to `story:request-id-idempotency`, and the edge to
  that story becomes `informed_by`; put to the operator, and this default stands until answered.

## Architecture decision

Option A, picked by the operator on 2026-11-27: one leader holds a lease with a fencing token in a
new lease table; standbys answer 503 on `/__spindle/ready`. Replicas serve failover; the rule of
one writer per store stays.

Not taken: B (every replica writes, with guards across processes, optimistic streams and request-id
records), C (defer).

## Scope

Derived 2026-11-27 by `story-scoper`, for option A of `## Architecture decision`. Every line is
**cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` PostgreSQL store and `crates/spindle-cli` serve
  path — cited
- **Files:** `crates/spindle-kernel/src/postgres.rs` (`PostgresStore::migrate`, `open`, `impl Log
  for PostgresEventStore`: the lease table, its take, renew and fencing token) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs` (the generic `LogStore`: `commit_all` `:579`
  with `append_group` at `:659`, `append` `:846` with `append_group` at `:855`, reached by
  `commit_events` `:761` and `forget_events` `:788`; every write carries the fence; the `Log`
  trait gains the hook, SQLite's a no-op) — cited
- **Files:** `crates/spindle-cli/src/database.rs` (`stores` `:101`, `migrate` `:139`, a
  `database_lease` setting beside `count` `:38`, the connection budget `:103-130`) — cited
- **Files:** `crates/spindle-cli/src/serve.rs` (`serve` `:301`: take the lease before
  `Host::with_stores` `:463`; the listener `bind_reading_on` `:498` must answer before the host
  exists; a renew beside the loop; a fenced leader exits) — cited
- **Files:** `crates/spindle-ingress/src/health.rs` (`ready` `:153`, `Probed` `:113-125`: standby
  reads `503`) — cited
- **Files:** `crates/spindle-host/src/host.rs` (doc comments `:535-537` on `Routed` and
  `:1681-1684` on `Host::rows`: the one-writer limit now enforced on PostgreSQL) — cited
- **Files:** `docs/design/spindle.md` (`:429` decision row, `:336-338`, `:457` M5) — cited
- **Files:** `website/docs/concepts/environment-and-admission.md` (`:262` seed on replicas; the
  lease setting; the grants on the lease table near `:395-458`) — cited
- **Symbols:** `PostgresStore::migrate`, `PostgresStore::open`, `Log`, `LogStore::commit_all`,
  `LogStore::append`, `database::stores`, `serve::serve`, `health::ready`, `Probed`,
  `Listener::stopping` — cited
- **Also likely:** `crates/spindle-ingress/src/listener.rs` (a standby flag beside `stopping`
  `:154`, `:174`, `:494`; a listener bound before the host's `Reader`) — inferred
- **Also likely:** `crates/spindle-cli/src/cli.rs` (the `run` doc `:155-170` on readiness) and the
  generated `website/docs/reference/cli.md` — inferred
- **Also likely:** `ess/domains/observe.yaml` (`spindle.observe.Readiness` `:926` gains
  `standby`), regenerated by `task generate:observe` into
  `ess/generated/jsonschema/schema/types/spindle.observe.Readiness.schema.json`,
  `crates/spindle-observe-api/{types.rs,source.schema.json,types-report.json}`,
  `ui/src/api/generated/{types.ts,source.schema.json,types-report.json}`, and
  `website/docs/concepts/observe.md` — inferred, only under D6 A
- **Tests:** `crates/spindle-kernel/tests/spindle-kernel/postgres.rs`,
  `crates/spindle-cli/tests/spindle-cli/postgres.rs` with a new submodule
  `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs` (matched by `task check:postgres`'s
  `postgres::` filter, so no `main.rs` or Taskfile change),
  `crates/spindle-ingress/tests/spindle-ingress/health.rs` — inferred
- **Documents:** `CHANGELOG.md` `[Unreleased]` — inferred
- **Confidence:** medium — the store, serve and readiness sites are read in the tree; how the fence
  reaches the append transaction (D1) and when a standby builds its host (D3) are open and move
  `sqlite.rs`, `listener.rs` and `host.rs` in or out
- **Would collide with:** any unit on the PostgreSQL store's open and migrate path
  (`postgres.rs`, `database.rs`), the `LogStore` commit path in `sqlite.rs`, the start of
  `serve.rs`, the ingress listener's readiness and stopping state, and the observe model's
  `Readiness`
- **Safety fact:** a stale leader's write cannot land because the lease token is checked inside
  the same PostgreSQL transaction as the append (`append_group_guarded`, eventlog-core 0.10.0
  `atomic_group.rs:183`, its guard reading through `ProjectionStore::get_for_update`), while
  today every Spindle append is unguarded (`sqlite.rs:659`, `:855`) — step 2, unproven
