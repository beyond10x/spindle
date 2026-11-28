---
format: aep.planning-md/3
id: story:unknown-commit-answered-as-unknown
kind: story
status: draft
title: A command whose commit outcome is unknown is not answered committed false
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/unknown_commit.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
- confidence: cited
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
revision: 8
---
## Outcome

A command whose commit outcome is unknown, because the store's deadline fired after COMMIT was sent,
is not answered `committed: false`: it answers `500` with `{"refused", "committed": "unknown"}`, so
a caller does not send again a write that may have landed. The kernel reports it as a store error of
its own, `StoreError::Unknown`. Only a PostgreSQL store returns it: a SQLite store reports a failed
COMMIT as a backend error, and the stdin run and replay never use PostgreSQL. The seed and the
bound-command retry paths are `story:unknown-commit-in-seed-and-retries`.

## Why

Line numbers at 18715a80.

Wave 33's U2 adversary (round 1, `story:new-leader-write-while-old-paused`) found that eventlog's
`UnknownCommit` maps to `StoreError::Backend` (`crates/spindle-kernel/src/sqlite.rs:2961-2967`,
`backend`, shared by `postgres.rs:41`; its test `a_deadline_is_the_busy_store_error` at `:2988`
asserts so at `:3015`), and the ingress answers it `501` with `committed: false`
(`crates/spindle-ingress/src/answer.rs:585`). eventlog 0.10.0 returns `UnknownCommit` once COMMIT
was started and the deadline fired (`eventlog-postgres/src/lib.rs` `timed_out_write`;
`atomic_group.rs:1008-1012` sets `commit_started` before `commit()`), so the write may have landed.
Present before wave 33. The publication-lock window of that story does not reach it: those writes
fail before COMMIT and answer `503` (`answer.rs:584`).

The answer cannot stay a `501`: ESS's published `501` body has `committed` as a required boolean and
allows no other key (`additionalProperties: false`), so `"committed": "unknown"` under `501` breaks
the contract. ESS declares no `500` on a command.

## Acceptance

Each test below runs in the default test step (`task check:the_tests_pass_within_the_budget`), on
loopback or in process, and ends within 1 s. The story adds about 1.5 s of test time to the step and
no test to the PostgreSQL lane.

- `an_unknown_commit_is_the_unknown_store_error` (unit test in `crates/spindle-kernel/src/sqlite.rs`,
  beside `a_deadline_is_the_busy_store_error`; D3 A): `backend(EventLogError::UnknownCommit)` is
  `StoreError::Unknown` carrying eventlog's text, displayed `store: <text>`.
  `a_deadline_is_the_busy_store_error` drops `UnknownCommit` from its list of errors that stay
  `Backend` (`:3015`) and keeps the rest of that list.
- `an_unknown_commit_answers_500_with_committed_unknown` (new
  `crates/spindle-ingress/tests/spindle-ingress/unknown_commit.rs`, a stub store built like
  `busy_store.rs` whose `commit`, `commit_all` and `remove` fail `StoreError::Unknown`; D1 A, D2 A):
  a command answers `500` with exactly the keys `refused` (naming the outcome as unknown) and
  `committed`, whose value is the string `"unknown"`, and `GET /__spindle/requests` records the
  request `failed`.
- `an_unknown_commit_is_not_read_as_uncommitted` (same file): `Exchange::serve_with_unless`
  (`crates/spindle-ingress/src/listener.rs:163`) passes `false` to `instead` for that answer, so a
  run whose lease was lost does not replace it with its `503` (`crates/spindle-cli/src/serve.rs:814`).
- A store failure before COMMIT still answers as it does today: the existing
  `busy_store::a_store_deadline_answers_503_and_commits_nothing` (`503`, `committed: false`) and
  `adversary_w33_u2::a_write_the_store_failed_otherwise_still_answers_501` (`501`,
  `committed: false`) pass unchanged.
- `website/docs/concepts/capabilities.md`'s store-failure paragraph (`:242-252`) names the `500`
  with `{"refused", "committed": "unknown"}` for a PostgreSQL commit whose outcome is unknown, and
  says the write may have landed, so a caller reads before it sends again; the `answer` doc
  (`answer.rs:145-148`) says the same; the change goes under `[Unreleased]` `### Fixed` in
  `CHANGELOG.md`, and `task docs-check` stays green.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W34 A): the scoper's default, option A, of every
decision, each reversible. No new dependency.

- D1 Status of an unknown outcome: A, `500`: ESS declares no `500` on a command, so no client reads
  it as the `501` body; the request log records it `failed`, and it does not mean "send again".
  Not taken: B (`504`, which proxies and clients often retry), C (`501`, which breaks ESS's `501` schema).
- D2 Body: A, `{"refused", "committed": "unknown"}`: the key callers already read, never equal to
  `false`, and the listener does not count it as uncommitted.
  Not taken: B (`{"refused", "outcome": "unknown"}` without `committed`), C (`"committed": null`, which a JavaScript caller reads as false).
- D3 Kernel shape: A, a new `StoreError::Unknown(String)`, mapped in `backend` from
  `EventLogError::UnknownCommit`; no match on `StoreError` in the workspace lists every variant
  (read, 18715a80).
  Not taken: B (keep `Backend` and recognise eventlog's message text in the ingress).
- D4 Other paths that treat a store error as "did not run": A, this story covers the HTTP command
  answer and the kernel variant; the bound-command retry and the seed go to
  `story:unknown-commit-in-seed-and-retries` (draft).
  Not taken: B (include them now).

## Scope

Derived 2026-11-28 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` (the command answer) and `crates/spindle-kernel` (the store-error mapping) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:2961` `backend` (shared by `postgres.rs:41`), and its test `a_deadline_is_the_busy_store_error` at `:2988`, which today asserts that `UnknownCommit` maps to `Backend` (`:3015`) — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:584-585` (the `ExecuteError::Store` arms), `Answer::unfinished` `:85`, `Answer::busy` `:97`, the `answer` doc `:145-148` — cited
- **Files:** `crates/spindle-kernel/src/store.rs:323` `StoreError` — inferred, the new variant under D3 A
- **Files:** `crates/spindle-ingress/tests/spindle-ingress/` a new `unknown_commit.rs` plus `main.rs`, a stub store built like `busy_store.rs` whose `commit`/`commit_all`/`remove` fail with the new variant — inferred
- **Symbols:** `EventLogError::UnknownCommit`, `StoreError::Backend`, `StoreError::Busy`, `ExecuteError::Store`, `Answer::unfinished` — cited
- **Documents:** `website/docs/concepts/capabilities.md:242-252` (the paragraph on store failures) — cited; `CHANGELOG.md` `[Unreleased]` `### Fixed` — inferred, the repository's convention; `website/docs/concepts/environment-and-admission.md:669` — inferred, only if it needs a sentence beside the `503`
- **Not touched, read:** the stdin path `crates/spindle-cli/src/main.rs:1016` refuses PostgreSQL, and eventlog-sqlite 0.10.0 maps a failed COMMIT of a group append to `Backend` (`finish_transaction`, `eventlog-sqlite/src/lib.rs:3150-3160`); `UnknownCommit` comes only from its blob path (`finish_blob_transaction`, `atomic_group.rs:246-258`), which spindle does not call. So stdin and SQLite never see it — cited
- **Not touched, read:** replay runs on memory stores (`crates/spindle-cli/src/replay.rs:9`) — cited
- **Request log class:** `Class::of` (`crates/spindle-ingress/src/observe.rs:162`) records `501` as `not-served` and other `5xx` as `failed`. Under D1 A the unknown answer records `failed` without a code change — cited
- **Listener:** `Exchange::serve_with_unless` (`crates/spindle-ingress/src/listener.rs:163`, the check at `:170-174`) counts a 5xx as uncommitted only where `committed` reads back as JSON `false`. Under D2 A a string `"unknown"` is not counted, so the lease-lost `503` (`crates/spindle-cli/src/serve.rs:814`) does not replace it — inferred from reading, no change needed
- **Confidence:** high — the story names both defect sites, and the eventlog 0.10.0 source shows where `UnknownCommit` comes from
- **Would collide with:** the store-failure paragraph of `capabilities.md`, `[Unreleased]` in `CHANGELOG.md`, the `ExecuteError::Store` arms of `answer.rs`, and the `backend` mapping plus its test in `spindle-kernel/src/sqlite.rs`. `story:reset-on-a-busy-store-keeps-the-store` edits the same `capabilities.md` paragraph and `CHANGELOG` section, and calls `Answer::busy` from `console.rs` for its reset `503` without editing `answer.rs` (its D1 A) — cited. `story:release-artifacts-signed` shares only `CHANGELOG.md`; its code surface is `.github/workflows/release.yml` — cited
- **Safety fact:** eventlog 0.10.0 returns `UnknownCommit` only after COMMIT was started (`eventlog-postgres/src/lib.rs:2526-2528` `timed_out_write`; `eventlog-postgres/src/atomic_group.rs:1008-1012` sets `commit_started` before `commit()`). A failure before COMMIT stays `Deadline`→`Busy` or `Backend`, so the second acceptance bullet keeps today's answers. `adversary_w33_u2::a_write_the_store_failed_otherwise_still_answers_501` already pins the `Backend` one — step 2, unproven
- **Test time:** about 1.5 s of run time. The kernel mapping unit test takes under 0.1 s. Two stub-store ingress cases on a loopback listener, like `busy_store.rs`, take about 0.3-0.5 s each. A real PostgreSQL case that fires the deadline after COMMIT is not proposed; eventlog's `timed_out_write.rs` covers it — inferred
- **Checked when planned (2026-11-28, read against 18715a80):** moved since the scoper read them: the kernel test `:2988` (was `:2986`), `Answer::busy` `:97` (was `:94`), the `answer` doc `:145-148` (was `:139-142`), `Class::of` `:162` (was `:161`), `serve_with_unless` `:163` (the scoper's `:170` is its check); eventlog-sqlite's `:3156` is in `finish_transaction`, not `append_group_guarded` — cited. No match on `StoreError` in the workspace lists every variant (`state_move.rs:224` matches one `Backend` text, every other site a single variant or a catch-all) — cited
