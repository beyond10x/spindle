---
format: aep.planning-md/3
id: story:reset-on-a-busy-store-keeps-the-store
kind: story
status: active
title: A reset on a busy store answers 503 and keeps the store
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/rows.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/supplied.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/adversary_w33_u2.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/busy_store.rs
- confidence: cited
  path: website/docs/concepts/capabilities.md
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
revision: 24
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T14:14:08Z", actor: "human:operator", revision: 23, decided_on: {"recorded":{"review_outcome":8}}}
- {from: "proposed", to: "active", at: "2026-11-28T14:14:08Z", actor: "human:operator", revision: 24, decided_on: {"recorded":{"review_outcome":8}}}
---
## Outcome

A `POST /api/reset` whose store misses its deadline (another session holds the lock it needs) keeps
the component's store, so the next command runs as before. While the reset has changed no row, it
answers `503` with `{"refused", "committed": false}` and nothing reset. Once it has changed rows (a
row removed, an earlier store emptied, a world row written), it answers `500`, saying that the store
is kept and that the reset can be sent again. A reset whose store fails with any other store error
answers `500` and loses the store, as today. Today every busy reset answers `500` and replaces the store with a lost one, and
every later command answers `501` until the run restarts.

## Why

Line numbers at 18715a80.

Wave 33's U2 adversary (round 2, `story:new-leader-write-while-old-paused`) built the state with a
stub store whose writes are busy: `POST /api/reset` answered `500` "its store could not be
reopened, and every read and write of it now fails", and the next command answered `501` "store:
desk's store was lost in a reset". `Host::empty` (`crates/spindle-host/src/host.rs:1951-2028`)
replaces a supplied store (PostgreSQL) with `Lost` (`:1963-1966`) when `removed_all` (`:3016`) or
the reset mark (`forget_events`) fails, whatever the error: `removed_all` turns each `StoreError`
into a `String`, so a busy one cannot be told apart. `console::reset`
(`crates/spindle-ingress/src/console.rs:572-597`) answers every such error `500` (`:595`). Each
removal is its own store call (`removed_all` calls `remove` once per instance), so a busy error on
a later removal leaves the earlier ones done. On PostgreSQL the removal and the reset mark go
through eventlog's guarded append, which takes the publication lock shared, so a reset in the
window a paused leader leaves can reach it. Present before wave 33. The case
`adversary_w33_u2::a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store`
(spindle-ingress, `adversary_w33_u2.rs:528-572`) holds it, ignored with this story's id.

## Acceptance

Each test below runs in the default test step (`task check:the_tests_pass_within_the_budget`) on
stub or memory stores, with no real deadline wait, and ends within 1 s. The story adds about 2 s of
test time to the step and no test to the PostgreSQL lane.

- `adversary_w33_u2::a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store`
  (`crates/spindle-ingress/tests/spindle-ingress/adversary_w33_u2.rs:530`), its `#[ignore]`
  removed: a reset held by a busy store answers `503` with `committed: false`, and once the store
  answers again the command `study.desk.OpenDesk` answers `202` `opened`.
- `a_reset_whose_first_step_is_busy_keeps_the_supplied_store` (new, in
  `crates/spindle-host/tests/spindle-host/supplied.rs`; D2 A): with a busy `scan` and a busy first
  `remove` on a supplied store holding one row, and a busy `forget_events` on a supplied store
  holding no row (removals run before `forget_events`, `host.rs:1955-1960`), each in turn: `Host::reset`
  returns `HostError::Store(StoreError::Busy(..))` whose text names the component and the failed
  step, `Host::reset_changed_rows()` is `false`, the row (where the store held one) is still read, and a command then commits
  on the same store.
- `a_reset_busy_after_a_removal_landed_answers_500_and_keeps_the_store` (new, in
  `crates/spindle-ingress/tests/spindle-ingress/busy_store.rs`; D1 A): on a store holding two rows
  whose second `remove` fails busy, `POST /api/reset` answers `500` whose `refused` says the store
  is kept and the reset can be sent again; the next command is answered by the model, not `501`
  "lost in a reset"; a reset sent once the store answers returns `200`.
- `a_busy_store_after_one_that_only_forgot_its_events_is_nothing_reset` (new, in
  `crates/spindle-host/tests/spindle-host/supplied.rs`; D3 A): with two supplied stores, the first
  holding no row and the second's first removal busy, `Host::reset` returns
  `HostError::Store(StoreError::Busy(..))` and `reset_changed_rows()` is `false`, which
  `console::reset` answers `503`.
- `a_reset_whose_store_fails_otherwise_still_loses_it` (new, in
  `crates/spindle-host/tests/spindle-host/supplied.rs`): on a supplied store whose removal fails
  `StoreError::Backend`, `Host::reset` returns `HostError::Reset { restored: false }`, which
  `console::reset` answers `500` (`console.rs:595`, unchanged), and every later command fails
  naming the store lost in a reset, as today.
- `website/docs/concepts/capabilities.md` (`:250-252`) names the two answers of a busy reset and the
  lost store of any other failure, `environment-and-admission.md` (`:363-364` and `:671-673`) says
  the same, and the doc comments of `HostError::Reset` (`host.rs:405-421`), `Host::supplied`
  (`:1100-1104`), `Host::reset` (`:1844-1851`) and `reset_changed_rows` (`:2035-2041`) no longer
  say that every failed removal loses the store; the change goes under `[Unreleased]` `### Fixed`
  in `CHANGELOG.md`, and `task docs-check` stays green.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W34 A): the scoper's default, option A, of every
decision, each reversible. No new dependency.

- D1 A busy error after the reset changed rows: A, keep the store; answer `503` with
  `committed: false` only while `Host::reset_changed_rows()` is `false`, otherwise `500` saying
  the store is kept and the reset can be sent again. This keeps the documented meaning of `503`
  (nothing was written), and a `503` always comes with the leased path's put-back of the seed
  record (`crates/spindle-cli/src/serve.rs:796`).
  Not taken: B (`503` with `committed` set to `reset_changed_rows()`, a new meaning of `503` on this route), C (`503` with `committed: false` always, untrue after a partial reset).
- D2 How the host reports a busy reset: A, the existing `HostError::Store(StoreError::Busy(..))`,
  the component and the failed step in its text; `removed_all` returns a typed `StoreError`. A busy
  `scan` inside `removed_all` counts as a busy write: the store is kept. No public type changes
  shape.
  Not taken: B (a new `HostError` variant, breaking spindle-host's API: `HostError` is not `#[non_exhaustive]`, `host.rs:316-317`), C (a `held` field on `HostError::Reset`, whose kept-store text is untrue after a partial removal).
- D3 An earlier store whose events alone were forgotten: A, counts as nothing reset, so the answer
  is `503` with `committed: false`, as `reset_changed_rows` and the seed put-back already read it.
  Not taken: B (counts as a change, so `500`).

Only `StoreError::Busy` keeps the store. Every other store error loses it and answers `500` as
today.

## Scope

Derived 2026-11-28 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host`, the reset of a supplied store — cited (story names `host.rs`, reset of a supplied store)
- **Files:** `crates/spindle-host/src/host.rs:1951-2028` (`Host::empty`, the `Kept::Supplied` arm that replaces the store with `Lost` and sets `component.lost`) — cited
- **Files:** `crates/spindle-host/src/host.rs:3016` (`removed_all`, which turns each `StoreError` into a `String`, so `Busy` cannot be told apart today) — cited
- **Files:** `crates/spindle-ingress/src/console.rs:563-597` (`console::reset`, today `Err(error) => 500` at `:595`; gains a busy arm answering `503` through `Answer::busy`) — cited
- **Files:** `crates/spindle-ingress/tests/spindle-ingress/adversary_w33_u2.rs:528-572` (the parked case, `#[ignore]` removed) — cited
- **Symbols:** `Host::empty`, `removed_all`, `Lost`, `HostError::Reset`, `StoreError::Busy`, `Host::reset_changed_rows`, `console::reset`, `Answer::busy` — cited
- **Also likely:** `crates/spindle-host/src/host.rs` doc comments at 405-421 (`HostError::Reset`), 1100-1104 (`Host::supplied`), 1844-1851 (`Host::reset` # Errors) and 2035-2041 (`reset_changed_rows`) — inferred, they state that a failed removal loses the store
- **Also likely:** `crates/spindle-host/tests/spindle-host/rows.rs` or `supplied.rs`, a host-level case where a busy removal or reset mark keeps the store and a store that fails otherwise is still lost — inferred
- **Also likely:** `crates/spindle-ingress/src/answer.rs:93-104` (`Answer::busy`), only if D1 B — inferred
- **Also likely:** `crates/spindle-cli/src/serve.rs:771-782`, the comment listing what a reset that changed no rows answers — inferred, comment only; the code (`restore = answer.status != 200 && !host.reset_changed_rows()`, :796) already puts the seed record back for a `503` that changed no rows
- **Documents:** `website/docs/concepts/capabilities.md:250-252` (replace "answers `500` and leaves the component's store lost" with both answers) — cited (acceptance)
- **Documents:** `website/docs/concepts/environment-and-admission.md:671-673` (same sentence) and `:363-364` ("`500` where a store refused its first removal" gains the busy `503`) — cited (671) / inferred (363)
- **Documents:** `CHANGELOG.md:57-58` (the [Unreleased] entry naming this story as "as before") plus a Fixed entry — cited
- **Decisions:** D1: what a busy error answers after the reset already changed rows: **A (default)** keep the store, answer `503` `committed: false` only while `reset_changed_rows()` is false, otherwise `500` naming that the store is kept and the reset may be sent again; B `503` with `committed` = `reset_changed_rows()`; C `503` `committed: false` always (untrue after a partial reset). D2: how the host reports it: **A (default)** `HostError::Store(StoreError::Busy(..))` with component and step in the detail, `removed_all` returning a typed `StoreError`; B a new `HostError` variant (breaking: `HostError` is not `#[non_exhaustive]`, host.rs:317); C a `held` field on `HostError::Reset` (its "as it was before the reset" text is wrong after a partial removal). D3: an earlier store whose events alone were forgotten: **A (default)** counts as nothing reset, as `reset_changed_rows` and the seed put-back read it; B counts as changed, so `500` — inferred
- **Confidence:** high — the story names the defect site and the case; the tree shows the single `Lost` replacement on the supplied path and the single `500` fallback in `console::reset`
- **Would collide with:** `crates/spindle-host/src/host.rs` reset (`Host::empty`, `reset_to`, `load`); `console::reset`; `crates/spindle-ingress/src/answer.rs` busy and unfinished mapping and `StoreError` in `crates/spindle-kernel/src/store.rs` (`story:unknown-commit-answered-as-unknown` edits the same answer mapping and adds `StoreError::Unknown`, which this story's reset loses the store on like any error other than `Busy`; the case for it, in `crates/spindle-host/tests/spindle-host/supplied.rs` beside this story's cases, is that story's, revision round 1); the `capabilities.md:242-252` paragraph (same paragraph as `story:unknown-commit-answered-as-unknown`: run the two in sequence); `environment-and-admission.md` § Several processes on one store and the seed-reset paragraph near :355-370, together with `serve.rs:768-806` and `seed.rs:374-430` (wave 33 U3's leased reset path); `CHANGELOG.md` [Unreleased]
- **Test time:** about 1-2 s of test run time — inferred: the parked case's stub store answers busy at once, with no real deadline wait, and one or two host-level cases with in-memory stores take about 0.1 s each; no PostgreSQL case is needed
- **Safety fact:** keeping the store after a busy error is sound because `StoreError::Busy` means the write was refused before it began to commit, so the store holds exactly what it held before that write (`crates/spindle-kernel/src/store.rs:341-345`; eventlog's `Deadline` maps to `Busy` and `UnknownCommit` does not, `crates/spindle-kernel/src/sqlite.rs:2961-2967`) — step 2, unproven
- **Checked when planned (2026-11-28, read against 18715a80):** moved since the scoper read them: `console::reset` ends at `:597`, its `500` arm `:595`; `Host::reset` # Errors `:1844-1851`; the `serve.rs` comment `:771-782`; `StoreError::Busy` `store.rs:341-345`; `backend` `sqlite.rs:2961-2967` — cited. Under D1 A this story calls `Answer::busy` and does not edit `answer.rs` or `store.rs`; its reset classification lives in `Host::empty`, where only `StoreError::Busy` keeps the store — cited from the decision. `CHANGELOG.md:57-58` is wave 33's entry; once release 0.28.0 is cut it sits under `[0.28.0]` and is not edited: this story adds its own `### Fixed` entry under `[Unreleased]` — cited. The new tests go in `crates/spindle-host/tests/spindle-host/supplied.rs` and `crates/spindle-ingress/tests/spindle-ingress/busy_store.rs`, existing modules, so this story does not edit the ingress tests' `main.rs` — inferred. `removed_all` calls `remove` once per instance, so on PostgreSQL each removal commits on its own and a busy error on a later one leaves the earlier ones done — cited
