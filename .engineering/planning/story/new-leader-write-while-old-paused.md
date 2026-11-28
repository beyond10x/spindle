---
format: aep.planning-md/3
id: story:new-leader-write-while-old-paused
kind: story
status: implemented
title: A write to a new leader while the old one is paused never answers 501
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/busy_store.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: cited
  path: crates/spindle-kernel/src/store.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: cited
  path: website/docs/concepts/capabilities.md
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 15
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T09:24:51Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-28T09:24:51Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "active", to: "implemented", at: "2026-11-28T16:06:04Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"test_result":1,"review_outcome":10}}}
---
## Outcome

While a paused former leader still holds an open database session, a write sent to the new leader
either commits or answers `503` with `{"refused", "committed": false}`, which
`website/docs/concepts/capabilities.md` names as a store that did not answer within its deadline,
safe to retry; never `501` with "event store deadline exceeded during database statement or lock".
The process that answers it keeps leading. An eventlog `Deadline` reaches the ingress as a
`StoreError` variant of its own, its display `store: …` unchanged (D1 A), so a SQLite store's
deadline answers `503` too. The window is not closed: it lasts until the store's idle timeout,
10 s, ends the paused leader's session (D2 A). Writes only: a view or events read behind the same
lock still answers `501` (`## Not covered`).

## Why

`a_resumed_leader_posts_no_event_the_new_leader_committed`
(`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:379-420`) got `501` with "event store
deadline exceeded during database statement or lock" for the new leader's write while the old
leader was paused (SIGSTOP): twice in wave 31's U6 adversary rounds (round 5; round 8, case 1) and
once in the test-time budget work of 2026-11-28 on a 4-CPU slice. It has not been reproduced on
demand. A `501` is the status for an unsupported feature, not for a lock wait. Tree: 276298bb.

Likely cause, read and not reproduced (unproven): eventlog 0.10.0's writers take the publication
advisory lock shared, and its feed reads take it exclusive until their transaction commits
(`publication_gate`, `eventlog-postgres/src/lib.rs:2742`); the egress lanes read the feed. A leader
paused inside such a read holds that lock until the server's 10 s idle timeout ends its session.
It does not hold the lease row, so the standby takes over, and the new leader's write waits out its
2 s lock timeout: eventlog's `Deadline` becomes `StoreError::Backend`
(`crates/spindle-kernel/src/sqlite.rs:2960-2965`, `backend`), which the ingress answers with
`Answer::unfinished(.., false)`, `501` (`crates/spindle-ingress/src/answer.rs:565-573`). The
failing test is the only one of its siblings with a webhook egress. The other reading: the lane's
load pushed the write past its 2 s lock wait.

The docs name no status for it: `website/docs/concepts/environment-and-admission.md:633-634` says
the paused leader's locks may hold up the new leader's writes, and
`website/docs/concepts/capabilities.md:239` gives `501` for a command the runtime cannot complete.

## Acceptance

- `a_write_behind_the_publication_lock_answers_503`, the red-capable reproduction, written first
  (`crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs`, `task check:postgres`, under 15 s):
  a session outside spindle holds the component's publication lock exclusive, as eventlog's feed
  read takes it (`pg_advisory_xact_lock` on eventlog's publication key for the component's prefix);
  a command sent to the leader meanwhile answers `503` with `committed: false` within 5 s, and the
  process still leads; once that session ends, the same command answers `201`. On 276298bb it
  answers `501`; the PR names that red run.
- `a_commit_behind_the_publication_lock_is_busy`
  (`crates/spindle-kernel/tests/spindle-kernel/postgres.rs`, `task check:postgres`, under 5 s):
  with the publication lock held exclusive by a foreign session, a commit returns the new
  `StoreError` variant, displayed `store: …` as before, and appends nothing.
- `a_deadline_is_the_busy_store_error` (unit test in `crates/spindle-kernel/src/sqlite.rs`, default
  test step): `backend` maps `EventLogError::Deadline` to the new variant, `GuardRefused` to
  `Backend(code)` and every other error to `Backend` as before.
- `a_store_deadline_answers_503_and_commits_nothing`
  (`crates/spindle-ingress/tests/spindle-ingress/busy_store.rs`, default test step, under 1 s; a
  stub `Store` as `concurrent_reads.rs:66` has): a commit that fails with the new variant answers
  `503` `{"refused", "committed": false}`, nothing is appended, and `GET /__spindle/requests`
  records the request as `failed` (`observe.rs:167-169`), not `not-served`.
- `a_resumed_leader_posts_no_event_the_new_leader_committed` (`adversary_w31_u6.rs:379`,
  `task check:postgres`, under 30 s): `t2` sent to the new leader answers `201`, or `503` and then
  `201` on a retry within 10 s; never `501`. The rest of the case is unchanged.
- `a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease`
  (`adversary_w31_u6.rs:1863`, `task check:postgres`), which today asserts that the write answers
  neither `201` nor `503` (`:1889`): it now asserts `503` with `committed: false` and a refusal
  other than the lease-lost one (`LEASE_LOST`, `crates/spindle-cli/src/serve.rs:1038`), within
  11 s; the leader keeps running and writes `t3` `201`, and the standby takes nothing.
- Evidence, not in the gate: 20 separate lane runs of
  `a_resumed_leader_posts_no_event_the_new_leader_committed`, each passing, the count and times in
  the unit's closing record (a POSIX sh loop in the session's scratch directory, started without the
  `slot` prefix; each `cargo` inside it takes a slot through the shim).
- `capabilities.md:239` names `503` with `committed: false` for a command whose store did not answer
  within its deadline, safe to retry; § Several processes on one store of
  `environment-and-admission.md` (`:628-635`) says a write to the new leader held behind a paused
  leader's locks answers `503` until that leader's session ends, at most 10 s; the `CHANGELOG.md`
  entry under `[Unreleased]` names the status change and the publication lock as the likely,
  unproven cause.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W33b A): the scoper's default, option A, of every
decision, each reversible. No new dependency, no `depends_on` edge.

- D1 How the deadline reaches the ingress: A, a new `StoreError` variant with the same display
  (`store: {0}`), made by `backend` from `EventLogError::Deadline` and answered `503`.
  Not taken: B (match the error text in the ingress), C (A plus `Overloaded`, rewriting
  `crates/spindle-kernel/src/state_move.rs:218-224`).
- D2 Close the window or answer it: A, answer `503` only; the docs name it; the window lasts at most
  the store's 10 s idle timeout.
  Not taken: B (the new leader waits until no foreign session holds the publication lock, read from
  `pg_locks`, which touches `crates/spindle-kernel/src/postgres.rs`), C (a shorter idle timeout for
  leased pools).
- D3 `Retry-After`: A, none, as on the other `503`s.
  Not taken: B (`Retry-After: 1`).

## Scope

Derived 2026-11-28 by `story-scoper` on `plan/wave-33` (276298bb). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` (the answer to a failed command) and `crates/spindle-kernel` (store error kinds) — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:565-573` (`ExecuteError::Store` → `Answer::unfinished`, the 501) — cited
- **Files:** `crates/spindle-kernel/src/store.rs:322-341` (`StoreError`: only `Conflict` and `Backend(String)`) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:2960-2965` (`backend`: every `EventLogError` but `GuardRefused` → `Backend(to_string())`) — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:379-420` (`a_resumed_leader_posts_no_event_the_new_leader_committed`: `t2` is asserted `201` with no retry, `assert_eq!(status, 201)` at `:400`; this story adds the retry of a `503` until `201` within 10 s) — cited
- **Symbols:** `StoreError` (new variant), `backend`, `Answer::unfinished`, `EventLogError::Deadline` (SQLSTATE 57014/55P03) — cited
- **Also likely:** `crates/spindle-ingress/tests/spindle-ingress/busy_store.rs` (new) and its `mod` line in `main.rs`; stub `Store` as `concurrent_reads.rs:66` — inferred
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/postgres.rs`; `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs` — inferred
- **Documents:** `website/docs/concepts/capabilities.md:239`, `website/docs/concepts/environment-and-admission.md:628-635`, `CHANGELOG.md` — cited; `website/docs/reference/cli.md:107` — inferred
- **Would collide with:** `answer.rs` command-answer mapping, `StoreError`/`sqlite.rs::backend`, `capabilities.md` status tables, § Several processes on one store, `postgres/replicas.rs`, `adversary_w31_u6.rs`, `CHANGELOG.md`
- **Safety fact:** the new variant keeps the display `store: {0}`; no code matches the deadline text; `state_move.rs:218-224` matches only `Overloaded`'s text; the only outside match on a `StoreError` variant is `spindle-conformance/src/establish.rs:52` (`Conflict`, fallthrough); eventlog returns `Deadline` only before a commit starts (`timed_out_write`, `eventlog-postgres/src/lib.rs:2526`; after: `UnknownCommit`), so the 503 says `committed: false` — level 2, unproven
- **Checked when planned (2026-11-28, read against 276298bb):** every line above holds;
  `state_move.rs` is `crates/spindle-kernel/src/state_move.rs`. Missed by the scope above:
  `a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease`
  (`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:1863-1896`) asserts
  `status != 503` for a write behind a foreign table lock (`:1889`), which D1 A turns into `503`; it
  is rewritten here — cited. `website/docs/reference/cli.md:107` is generated from
  `crates/spindle-cli/src/cli.rs:211-215`, so a change to it is a change there — inferred
- **Revised after plan-critic round 1 (2026-11-28, read against fc19c7d9):** the retry in
  `a_resumed_leader_posts_no_event_the_new_leader_committed` is added, not already there
  (`adversary_w31_u6.rs:400`). In wave 33 `crates/spindle-ingress/src/answer.rs` is this story's
  alone: `story:listener-limits` builds its queued-request `503` in `listener.rs` and does not edit
  it. `story:postgres-lane-margin` merges first and may edit lane tests of `adversary_w31_u6.rs`;
  the two cases rewritten here are rebased onto it — cited

## Not covered

- A view or events read behind the same publication lock still answers `501`
  (`crates/spindle-ingress/src/answer.rs:412`, `listed`). No story is filed for it.

## Delivered

Wave 33, unit U2, merged into wave/33 at f03764dd; adversary rounds 1-3, round 3 refuted.

Changed from the plan, by the coordinator during the wave:

- Only the PostgreSQL store returns `StoreError::Busy`: eventlog-sqlite 0.10.0 never returns
  `Deadline`, so a command whose SQLite write is busy still answers `501`. The Outcome's SQLite
  `503` is not delivered.
- The `503` covers commands. A held view read answers `501`, a held events or history read `500`
  (`503` during a drain, as before).
- A `POST /api/reset` held by a busy store still answers `500` and loses the store:
  `story:reset-on-a-busy-store-keeps-the-store`, its case parked with `#[ignore]`.
- `UnknownCommit` still answers `501` `committed: false`: `story:unknown-commit-answered-as-unknown`.
