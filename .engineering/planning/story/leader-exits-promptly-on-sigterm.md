---
format: aep.planning-md/3
id: story:leader-exits-promptly-on-sigterm
kind: story
status: active
title: A leader on PostgreSQL exits promptly on SIGTERM under load
relations:
- serves: vision:spindle
- depends_on: story:postgres-lane-runs-locally
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/database.rs
- confidence: cited
  path: crates/spindle-cli/src/egress.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/egress.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs
- confidence: inferred
  path: crates/spindle-host/src/host/rows.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: inferred
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T04:16:53Z", actor: "human:operator", revision: 5, decided_on: {"recorded":{"review_outcome":11}}}
- {from: "proposed", to: "active", at: "2026-11-28T04:16:53Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"review_outcome":11}}}
---
## Outcome

A leading process on PostgreSQL that receives SIGTERM releases its leases and exits within a bound
the docs name, also while the store is loaded, so a standby takes over without waiting for the lease
to run out. The bound is named in `run --help` beside `--drain` (D2 A): `--drain` (counted only
while requests are queued) plus the longest egress call's `timeout_ms` plus a fixed teardown
allowance of 5 s, measured and adjusted before merge. A post under way is finished, every cursor
an egress lane handed back is saved before the leases are released, and each release at exit
waits at most 1 s per statement whatever `database_lease` is (D3 A).

## Why

Found by wave 31's U6 implementor: `a_standby_posts_no_webhook_and_keeps_no_dead_letter` failed in
2 of 9 PostgreSQL lane runs because the leader took more than 10 s to exit after SIGTERM (14.9 s in
one probe). At 8 s its database sessions were idle and unblocked, the standby had ended no session,
and the leader's threads were waiting in the process. Bulk-insert transactions of 13 s from other
tests were open on the cluster at the time. The cause inside the process is not found.

## Acceptance

All tests below are PostgreSQL-lane tests (`#[ignore]`, run by `task check:postgres`), each under
the 60 s bound.

- The `CHANGELOG.md` `[Unreleased]` entry names the in-process wait that held the exit after
  SIGTERM (the scoper's candidate, unproven: the egress lane's read in `LogStore::published`, up to
  `PUBLISH_WAIT` = 30 s, `crates/spindle-kernel/src/sqlite.rs:1201-1232`).
- `a_leader_exits_within_the_bound_under_an_open_transaction` (`spindle-cli` `postgres/replicas.rs`),
  red first on 0.26.0, where the exit comes past the bound. The test's numbers: `database_lease` 2 s
  (`LEASE`); `--drain` at its default 20 s, which adds 0 s since no request is queued; the egress
  call's `timeout_ms` 2000 (`crates/spindle-cli/tests/spindle-cli/egress.rs:191`); so the bound is
  0 + 2 + 5 = 7 s from SIGTERM. Steps: the webhook answers `500` for t1; another session (any role)
  opens a transaction with a transaction id at T0; the webhook then answers `200`, so the leader
  saves its cursor after T0; the test waits until the webhook has received t1's `200` post and only
  then sends SIGTERM at instant S, and holds the transaction until S + 8 s (the 7 s bound plus 1 s),
  so the cursor save is ordered after T0 and before SIGTERM, and the hold outlasts the bound. The
  test fails, naming the delay, when S comes more than 5 s after T0 (the egress retry pause's cap,
  `BACKOFF_CAP`), so the hold stays at most 13 s. Asserted: the leader exits within 7 s of SIGTERM;
  the standby prints `took the lease of <component>` within 1.33 s (two thirds of the 2 s lease)
  counted from the instant the test reads the old leader's exit status, and answers ready `200`
  within 10 s of it; 3 s after that, the webhook has received t1 from the old leader only (its
  `500` and its `200`) and never from the new leader. If D2 A's 5 s allowance is adjusted before
  merge, the 7 s and the hold change with it in the same change, the hold
  staying at most 13 s after T0 and ending at least 1 s past the bound.
- That end-to-end test passes in 20 of 20 separate runs, each its own nextest call under the 60 s
  bound, on the lane `story:postgres-lane-runs-locally` starts, recorded as evidence; the 20 runs
  are not part of the gate.
- `a_release_under_a_long_lease_waits_at_most_one_second` (`spindle-kernel` `postgres.rs`), red
  first on 0.26.0 (a third of the lease, 1200 s, per statement; killed at 60 s): with a lease of
  3600 s and another session holding the lease row `FOR UPDATE`, the release at exit returns an
  error within 1.5 s (1 s statement and lock timeout, D3 A, plus 0.5 s margin).
- `a_stopped_feed_read_returns_while_an_older_transaction_is_open` (`spindle-kernel` `postgres.rs`),
  added with the stop D1 A introduces, so not red first: with a transaction held open by another
  session, an `events_placed_after` whose stop flag is set returns within 0.5 s.
- `run --help` and the generated `website/docs/reference/cli.md` name the exit bound after a signal.
- The unit's closing record names the time of one `task check:postgres` run before and after this
  story's tests and the `pg_stat_activity` connection peak of the run after, sampled as
  `story:postgres-lane-runs-locally` does; when the peak comes within 50 of the `max_connections`
  in `scripts/postgres-lane.sh`, the same change raises that value in the script and in
  `.github/workflows/ci.yml`, and the options check of `task check` stays green.

## Decided when scoped

Taken 2026-11-28 on the scoper's default, each reversible.

- **D1 Where the egress wait is cut.**
  A (default, taken): the lane's read stops waiting once the lane is stopped: `published()` (and
  the turn wait) checks a stop flag the egress passes in; the join then waits at most one feed
  read.
  B: the lane's reads skip the own-append wait, since its own appends are only cursor marks it
  never posts. Changes what a `Reading` promises.
  C: `Egress::halt` joins with a deadline and leaves a lane still waiting. A post may then land
  after the leases are released.
- **D2 The bound the docs name (`run` help and `--drain`).**
  A (default, taken): exit within `--drain` (only while requests are queued) + the longest egress
  call `timeout_ms` + a fixed teardown allowance of 5 s (measured, adjusted before merge). A post
  under way is finished, not abandoned.
  B: a fixed bound after the drain; a post under way past it is abandoned and sent again after
  a restart, possibly twice if the new leader also posts it.
- **D3 How long a lease release may wait at exit.**
  A (default, taken): the releases at exit use a fixed short statement and lock timeout (at most
  1 s), so the bound does not grow with `database_lease`.
  B: keep a third of the lease per statement. Fine at 2 s, hours with a day-long lease.
- **D4 The red-first test.**
  A (default, taken): an end-to-end case in `postgres/replicas.rs`: the webhook answers 500 for
  t1; the test opens a transaction with a transaction id on a connection of its own and holds it
  for at most 13 s; the webhook then answers 200, so the leader saves the cursor after that
  transaction began; then SIGTERM, and the exit is asserted within the D2 bound. Plus a kernel
  case: a stopped `events_placed_after` returns while an older transaction is open.
  B: the kernel case alone.
- **D5 The "20 of 20 runs" line.**
  A (default, taken): 20 separate runs of the D4 end-to-end case, each its own nextest call under
  the 60 s test bound, recorded as evidence, not part of the gate.
  B: one test that loops 20 times. Not allowed: about 190 s at the 9.4 s measured for the
  existing case.

## Scope

Derived 2026-11-28 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` (the served run's teardown and egress) — cited
- **Files:** `crates/spindle-cli/src/egress.rs` (`Egress::halt` 327-337, `Drop for Egress` 365, `Delivery::deliver` 385-472) — cited
- **Files:** `crates/spindle-cli/src/serve.rs` (teardown 860-866, `CLOSE_GRACE` 1022, loop top 676-697) — cited
- **Files:** `crates/spindle-cli/src/database.rs` (`Kept::drop` 547-556, `Leases::renewing` 416-480, `Kept::confirmed` 508-526) — cited
- **Files:** `crates/spindle-cli/src/cli.rs` (`run` help 158-164, `--drain` 566-577: where the bound is documented) — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs` (`a_standby_posts_no_webhook_and_keeps_no_dead_letter` 486-534, `exit_within` 160) — cited
- **Symbols:** `LogStore::published`, `PUBLISH_WAIT`, `SharedReading::events_placed_after`, `TURN_WAIT` in `crates/spindle-kernel/src/sqlite.rs` (1201-1241, 1292-1318) — inferred, the wait named as the cause, proven to level 3 only
- **Also likely:** `crates/spindle-kernel/src/sqlite.rs` — inferred, where the wait is made stoppable (D1 A)
- **Also likely:** `crates/spindle-kernel/src/store.rs` (`trait Reading` 624, default 663) and `crates/spindle-host/src/host/rows.rs` (129) — inferred, only if D1 A changes the `Reading` signature
- **Also likely:** `crates/spindle-kernel/src/postgres.rs` (`Lease::connected` 463-485, `Lease::release` 638) — inferred, only with D3 A
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/postgres.rs` — inferred, the kernel red test of D4 A
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/egress.rs` (`Webhook::answer_with` 78, used by the red test) — inferred
- **Documents:** `CHANGELOG.md` — inferred
- **Confidence:** medium — the story names no site; the cause is inferred from the code path and matches the reported symptoms, unproven
- **Would collide with:** any unit touching the served run's teardown or lease code (`serve.rs`, `database.rs`), the egress lanes (`egress.rs`), the kernel's PostgreSQL read path (`sqlite.rs` `published`/`SharedReading`), or `postgres/replicas.rs`
- **Safety fact:** every cursor a lane handed back is saved before the leases are released, because `Kept` drops after `Egress` in `serve()` (declared at `serve.rs:356`, egress dropped at `serve.rs:865`), so the new leader does not post t1 again; a change that releases first or abandons a post loses this — step 2, unproven
