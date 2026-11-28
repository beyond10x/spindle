# Wave 33

**Proposed.** Planned on the operator's pick of 2026-11-28 (D-W33 A, D-W33b A, relayed by the
host operator). All stories serve `vision:spindle`.

Integration branch `wave/33`, forked from `main` after release 0.27.0 lands (`main` is 276298bb
when this plan was written). Release 0.28.0 follows this wave.

## Selection

D-W33 A picked four drafts: `story:listener-limits`, `story:new-leader-write-while-old-paused`,
`story:seed-record-after-a-component-is-removed` and `story:entity-invariant-over-stored-rows`.
Each was scoped against `plan/wave-33` at 276298bb by one `story-scoper`, and every claim written
into a story was read against the tree at the cited line. Each story's `## Scope` holds its surface,
its `## Decided when scoped` the options taken (D-W33b A: the scoper's default, option A, of every
decision), and its `## Outcome`, `## Why` and `## Acceptance` were amended where the scoper found a
line stale or not checkable within the 60 s test bound, or where a decision changed it.

Not in the wave; both stories stay as they are:

- `story:sensitive-fields-redacted`: waits on an ESS release with a field sensitivity marker; none
  has one up to ESS 0.99.12.
- `story:release-artifacts-signed`: adds the workflow permissions `id-token: write` and
  `attestations: write`; held for the owner's word.

Acceptance amendments, one line per story:

- `story:listener-limits`: the cap test became `a_connection_over_the_cap_waits_until_one_closes`
  (D2 A: not accepted, held in the backlog); `431` may carry an empty body and "writes nothing"
  means no exchange and no request record; added `an_idle_connection_is_closed_and_frees_its_place`
  (idle keep-alive closed within the head read time), the `503` of a queued request past its time
  apart from the `504` of a taken one (D3 A), the `spindle-control` copy of `same` (D5 A) and
  `run_flags_bound_the_listener` (D6 A).
- `story:new-leader-write-while-old-paused`: the first line is the red-capable reproduction
  `a_write_behind_the_publication_lock_answers_503` (`501` on 276298bb); a kernel, a mapping and an
  ingress test pin the new `StoreError` variant and its `503`; the two `adversary_w31_u6.rs` cases
  that see the status are rewritten (one asserts `!= 503` today); 20 of 20 runs became evidence in
  the closing record, not a gate; a view or events read behind the lock stays `501`
  (`## Not covered`).
- `story:seed-record-after-a-component-is-removed`: the lane test is two named tests on the
  existing fixtures `adversary-w32-u4-grown` then `ingress`; the move is guarded by the orphan
  table's own lease and the first component's (D1 A), not the first component's alone; added kernel
  tests for the scan by table name (hashed prefix), a pattern match without a `lease` row, a deposed
  mover and a record under another holder's lease.
- `story:entity-invariant-over-stored-rows`: the two cases became six named tests (three kernel
  `fit.rs`, two `spec_change.rs`, one PostgreSQL-lane), a row whose invariant is `Unknown` fits,
  each line names the statement as well as the entity and the count (D2 A).

## Units

| unit | story | surface | branch | tree |
|---|---|---|---|---|
| U1 | `story:listener-limits` | `crates/spindle-ingress` (`listener.rs`, `surface.rs`, `lib.rs`), `run` flags (`cli.rs`, `main.rs`, `serve.rs`), `crates/spindle-control/src/server.rs` | `unit/w33-listener-limits` | `spindle-w33-listener-limits` |
| U2 | `story:new-leader-write-while-old-paused` | `crates/spindle-kernel` (`store.rs`, `sqlite.rs` `backend`), `crates/spindle-ingress/src/answer.rs`, `adversary_w31_u6.rs` | `unit/w33-busy-store` | `spindle-w33-busy-store` |
| U3 | `story:seed-record-after-a-component-is-removed` | `crates/spindle-kernel/src/postgres.rs` (`Lease`), `crates/spindle-cli/src/database.rs` (`Leases`) | `unit/w33-orphan-seed` | `spindle-w33-orphan-seed` |
| U4 | `story:entity-invariant-over-stored-rows` | `crates/spindle-kernel/src/fit.rs` (`Shapes::misfits`, `Misfit`) | `unit/w33-entity-invariant` | `spindle-w33-entity-invariant` |

No `depends_on` edge inside the wave. Each unit is built by one `aep:implementor` and gets one
`aep:adversary` pass when green.

## Surfaces

Files two or more units share, and how they are kept apart:

- `CHANGELOG.md` `[Unreleased]`: all four. Each unit adds its own bullet naming its story id; each
  merge into `wave/33` keeps both sides, ordered by story id.
- `AGENTS.md`: U1 (`:38`, the `spindle-ingress` row) and U3 (`:40`, the `spindle-cli` row's seed
  sentence). Different rows. U2 and U4 do not list it: the file states neither the `501` of a store
  error nor the misfit rule; a unit that edits a row anyway keeps the other side.
- `website/docs/concepts/environment-and-admission.md`: U2 § Several processes on one store
  (`:628-635`), U3 § A seed on PostgreSQL with a lease (`:307-336`), U4 § A specification change
  over stored rows (`:676-692`). Different sections.
- `website/docs/concepts/capabilities.md`: U1 the status table (`:228-237`: `431`, and the `503`
  and `504` of a request past its time), U2 the line after it (`:239`: `503` for a store deadline).
  Adjacent lines, merged by hand keeping both.
- `crates/spindle-cli/src/cli.rs` and the generated `website/docs/reference/cli.md`: U1 the new
  `run` flags (beside `--drain`, `:575-594`), U3 the `--seed` help (`:555-575`, `cli.md:153`), U4
  `run`'s description (`:298`, `cli.md:123`), U2 only if it changes `:211-215` (`cli.md:107`).
  Different doc comments in `cli.rs`; `cli.md` is not merged by hand: after each merge
  `task docs-generate` rewrites it and `task docs-check` is green.
- `crates/spindle-kernel/tests/spindle-kernel/postgres.rs`: U2 (one test) and U3 (four tests). Each
  appends its own tests; both kept.
- `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs`: U2 (one test) and U3 (its two tests
  go in `replicas/adversary_w32_u4.rs`, declared at `replicas.rs:25`; listed in case it adds a
  helper). Both kept.
- `crates/spindle-ingress/tests/spindle-ingress/main.rs`: U1 `mod limits;` and U2
  `mod busy_store;`, one line each, kept in name order.
- `crates/spindle-kernel/src/postgres.rs`: U3 only; D2 A of U2 leaves it untouched (D2 B would have
  shared it).

No other file is in two units' scope.

## Order and parallel safety

All four are built in parallel from `wave/33`. Merge order: U4, U2, U1, U3, each merge resolving
the shared files above by keeping both entries, `CHANGELOG.md` ordered by story id.

- Default test step: U1 adds seven ingress and control tests (each under 3 s) and one `spindle-cli`
  test (under 5 s); U2 one ingress test (under 1 s) and one kernel unit test; U4 three kernel and
  two `spindle-cli` tests (each expected under 5 s). The step stays under 120 s; each unit names a
  test over 5 s with its time.
- PostgreSQL lane (`task check:postgres`, its parallel part under 100 s): U2 adds two tests (under
  15 s and 5 s) and rewrites two `adversary_w31_u6.rs` cases; U3 adds two `spindle-cli` and four
  kernel tests (each expected under 10 s); U4 adds one. Each of U2, U3 and U4 times the lane before
  and after its tests and names both times in its report; a lane over its bound is a defect fixed
  before the wave's PR, not by a longer bound. All new lane tests sit under the lane's `postgres::`
  filter, so no unit edits the `TESTS` filter in `Taskfile.yml`.
- U2's 20 evidence runs are a measurement, not a gate: a POSIX sh loop in the session's scratch
  directory, started without the `slot` prefix.

## Decisions taken

Every decision takes the scoper's default, option A, each reversible (D-W33b A).

- `story:listener-limits`: D1 A (512 connections, 16 KiB head, 100 headers, 30 s), D2 A (stop
  accepting at the cap), D3 A (queued `503` and skipped, taken `504` and completed), D4 A (time from
  the end of the body read), D5 A (the `spindle-control` copy fixed here), D6 A
  (`Surface::limited(Limits)` and three `run` flags).
- `story:new-leader-write-while-old-paused`: D1 A (a new `StoreError` variant from eventlog's
  `Deadline`, answered `503`), D2 A (answer the window, do not close it), D3 A (no `Retry-After`).
- `story:seed-record-after-a-component-is-removed`: D1 A (the orphan table's own lease taken for
  the move), D2 A (scan only when no current table holds a record), D3 A (wave 32's rule; one
  schema, one leased deployment).
- `story:entity-invariant-over-stored-rows`: D1 A (every declared invariant over every row on a
  digest change), D2 A (one line per broken invariant).

## What a unit must not do

- Write Python in anything that stays in the repository: tools, tasks, gates, CI steps and test
  helpers are Rust or POSIX sh.
- Add a test that runs over 60 s: nextest kills it (`.config/nextest.toml`, `slow-timeout` with
  `terminate-after`). A test over 5 s is named with its time in the unit's report.
- Let a test step run over 120 s, or the PostgreSQL lane's parallel part over 100 s.
- Hold a pool job past 10 minutes; a job that needs longer names its time with `slot --timeout`,
  at most 30 minutes. A script that waits or measures (U2's 20 evidence runs) starts without the
  `slot` prefix; `cargo` inside it takes a slot through the shim.
- Run the whole workspace's tests; run the tests of the crates the unit touched.
- Make a model call, a call to a real API or a network request in a test; loopback only.
- Add a dependency: none of the four needs one.
