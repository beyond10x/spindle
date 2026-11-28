# Wave 32

**Delivered.** Planned on the operator's pick of 2026-11-28 (option A, relayed by the host
operator). All stories serve `vision:spindle`.

Integration branch `wave/32`, forked from `main` after `integration/2026-11-28-1` lands there
(`main` is 546cc9d2, release 0.26.0, when this plan was written). Release 0.27.0 follows this wave.

## Selection

Option A picked five drafts: `story:postgres-lane-runs-locally`,
`story:conform-awaits-system-steps`, `story:seed-under-a-lease`,
`story:leader-exits-promptly-on-sigterm` and `story:view-filter-absent-optional`. Each was scoped
against `plan/wave-32` at 546cc9d2 by one `story-scoper`; each story's `## Scope` holds its
surface, its `## Decided when scoped` the options taken, and its `## Outcome` and `## Acceptance`
were amended where the scoper found a line stale or not checkable within the 60 s test bound.

`story:view-filter-absent-optional` is not in the wave: it is archived, because ESS answered that
the refusal is its rule.

Acceptance amendments, one line per story:

- `story:postgres-lane-runs-locally`: "PostgreSQL 17" became 13 or later with a warning off 17
  (D3 A); "a test compares the options" became a `task check` sh check, red first on a literal
  `-c max_connections=100`; added: no cluster when `SPINDLE_TEST_DATABASE_URL` is set, the CA is
  exported for the hosted TLS cases, nothing is left behind after a failed test or SIGINT.
- `story:conform-awaits-system-steps`: the awaited step is a command every grant of which is a
  `system: true` actor, not a deadline, tick or binding (ESS 0.99.2 refuses a binding that invokes
  one); "fails when the host does not take the step" became parity with `Interpreted::for_model`
  plus a `NotIssued` wrapper that fails with `ESS-CF-AWAIT`; the skip count is taken on a fixture
  (D5 A); the ESS pin stays 0.99.2.
- `story:leader-exits-promptly-on-sigterm`: the bound is the one `run --help` names (D2 A); "20 of
  20 runs" became 20 separate nextest runs of one test under 60 s, recorded as evidence (D5 A);
  the open transaction is held by another session from before the last cursor save; "the standby
  leads within one poll" became `took the lease of <component>` within two thirds of the lease and
  ready `200` within 10 s.
- `story:seed-under-a-lease`: two lines became six named PostgreSQL-lane tests, adding the takeover,
  resume-after-count and upgrade (D5 B) cases; the three D5 A tests of `adversary_w31_u6.rs` are
  rewritten or retired; the docs that state D5 A of `story:several-writers-one-postgres` change.

Revision round 1 of the plan critics (2026-11-28, review results `wave32-acc-r1`, `wave32-des-r1`,
`wave32-sco-r1`, `wave32-par-r1`; 20 findings), on the coordinator's decisions:

- `story:postgres-lane-runs-locally`: `scripts/postgres-lane.test.sh` checks the clean-up around a
  failing command and a SIGINT, and the version handling (major printed, warning off 17,
  `SPINDLE_PG_BIN`); the options check requires `max_connections` of at least 300, one value in
  script and CI; the `pg_stat_activity` peak of one lane run goes into the closing record.
- `story:conform-awaits-system-steps`: `## Not covered` records the deadline, tick and binding
  awaits and the repository's own await skip count; the fixture holds a stated, an `Optional` and a
  `when_related:` input; `no_await_scenario_of_the_fixture_is_skipped` and
  `a_replayed_scenario_runs_its_await` are added.
- `story:leader-exits-promptly-on-sigterm`: `depends_on story:postgres-lane-runs-locally`; the
  end-to-end test states its numbers (bound 7 s = 0 s drain + 2 s `timeout_ms` + 5 s, a hold to 8 s after SIGTERM,
  standby within 1.33 s of the exit) and asserts t1 is not posted again; the wait is named in the
  `CHANGELOG.md` entry; `a_release_under_a_long_lease_waits_at_most_one_second` (lease 3600 s) is
  added; the kernel stop test is not red first.
- `story:seed-under-a-lease`: the Outcome's first sentence states D5 B; the refusal of another file
  is split in two; reset (D6 A) and deposed-writer (D3 A) tests are added.

## Units

| unit | stories | branch | tree |
|---|---|---|---|
| U1 | `story:postgres-lane-runs-locally` | `unit/w32-pg-local` | `spindle-w32-pg-local` |
| U2 | `story:conform-awaits-system-steps` | `unit/w32-await` | `spindle-w32-await` |
| U3 | `story:leader-exits-promptly-on-sigterm` | `unit/w32-sigterm` | `spindle-w32-sigterm` |
| U4 | `story:seed-under-a-lease` | `unit/w32-seed-lease` | `spindle-w32-seed-lease` |

`depends_on` edges inside the wave: U3 on U1 (its 20 evidence runs use U1's local lane), U4 on U3
(the same lease thread and teardown). No cycle; U2 has no edge. Each unit is built by one
`aep:implementor` and gets one `aep:adversary` pass when green.

## Order and parallel safety

Merge order: U1 and U2 in parallel, then U3, then U4.

- Before the fork: `integration/2026-11-28-1` (the test-time budget of CI's two test steps) lands
  on `main` first. It changes `.config/nextest.toml` (`test-threads = 8` in the default profile,
  three new `profile.postgres.overrides`, a wider priority-10 filter) and 12 test files under
  `crates/spindle-{kernel,host,cli}/tests/`, among them `adversary_w31_u6.rs:904-912`, and
  `crates/spindle-cli/src/seed.rs` (`Seeding::end`, `:640-675`). The last two are in U4's surface;
  `wave/32` forks after the merge, so U4 starts on them.
- `CHANGELOG.md` `[Unreleased]` (empty on `main`, `:7`) is edited by all four units, so every
  pair shares it: each unit adds its own bullet naming its story id, and each merge into `wave/32`
  keeps both sides, ordered by story id. No other pair below lists it again.
- U1 and U2 share no other file. U1: `Taskfile.yml` `check:postgres` (`:180-197`) and a new
  `check:postgres-lane-script`, `.github/workflows/ci.yml` (`:188-214`), `AGENTS.md:132-136`,
  `scripts/`. U2: `crates/spindle-conformance`, `crates/spindle-cli/tests/spindle-cli/replay.rs`,
  one row of the ESS-divergence table in `docs/design/spindle.md`. U2 leaves the ESS pin
  (`Taskfile.yml`, `ci.yml`, `Cargo.toml`) at 0.99.2, so it does not touch U1's files.
- U1's server options cover the lane's concurrency after `integration/2026-11-28-1` (the postgres
  profile's `test-threads = 12`, heavy group `max-threads = 3`): U1's own acceptance holds the
  `pg_stat_activity` peak of one lane run and a `max_connections` of at least 300, one value in
  script and CI.
- U3 next (`depends_on` U1): a defect of 0.26.0, and U4 builds on its exit path. Its 20 evidence
  runs use the local lane U1 starts. It shares no file with U1 or U2.
- Lane budget: `story:postgres-lane-margin` records the PostgreSQL lane at 112.3 s of 120 s on CI
  (spindle#101). U3 adds one process test (at most 13 s: a transaction held to 8 s after SIGTERM) and two kernel
  tests; U4 adds nine process tests and one kernel test and rewrites three. `integration/2026-11-28-1`,
  which cuts the time of CI's two test steps, lands before `wave/32` forks. Each unit times
  `task check:postgres` before and after its tests and names both times in its report; a lane run
  over 120 s is a defect fixed before the wave's PR, not by a longer bound. U3 and U4 each sample
  the `pg_stat_activity` peak of one lane run after their tests, as U1 does, and name it beside the
  lane time; a peak within 50 of the script's `max_connections` raises the value in the script and
  in CI in the same unit.
- U4 after U3 (`depends_on`): both change `crates/spindle-cli/src/serve.rs` (U3 the teardown
  `:860-866` and loop top `:676-697`; U4 the seed branch `:594-625` and resets `:749-777`),
  `crates/spindle-cli/src/database.rs` (U3 `Leases::renewing` `:416-480`, `Kept::confirmed`
  `:508-526` and `Kept::drop` `:547-556`; U4 `Leases::keep`'s channel `:391-411`, `renewing` and
  `Kept::taken_again` `:537-544`), `crates/spindle-kernel/src/postgres.rs` `Lease` (U3 the release
  timeout, `:463-485`, `:638`; U4 the `seed` row), `crates/spindle-cli/src/cli.rs` (U3 `run` help
  `:158-164`; U4 `--seed` help `:549-555`) with the generated `website/docs/reference/cli.md`, and
  `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs`. U4's tree forks from `wave/32` after
  U3 merges.
- U4 and U1 share `AGENTS.md` (U4 `:40`, U1 `:132-136`); U4 and U2 share `docs/design/spindle.md`
  (U4 the row at `:436`, U2 a new row in the same table). Other rows, merged by hand.
- U3's and U4's tests are in `postgres/replicas.rs` and the kernel's `postgres.rs`, matched by
  `check:postgres`'s `postgres::` filter, so neither edits the `TESTS` filter. U1 does not rewrite
  that filter either: it drops `requires` and moves the task's body, filter included, into an
  inner task (D2 A).

## Decisions taken

Every decision takes the scoper's default, each reversible; the operator's pick (option A) covered
them.

- `story:postgres-lane-runs-locally`: D1 A (the script prints the options, CI's `docker run` reads
  them), D2 A, D3 A, D4 A, D5 A, D6 A, D7 A, D8 (off the `slot` prefix, nextest through the shim).
- `story:conform-awaits-system-steps`: D1 A (issue the command as the `system: true` actor, inputs
  in the interpreter's order), D2 A, D3 A, D4 A, D5 A.
- `story:leader-exits-promptly-on-sigterm`: D1 A (a stopped lane's feed read stops waiting), D2 A,
  D3 A (release at exit at most 1 s per statement), D4 A, D5 A; `depends_on
  story:postgres-lane-runs-locally`.
- `story:seed-under-a-lease`: D1 A, D2 A, D3 A (the lease thread writes the record), D4 A (a `seed`
  row in `<prefix>_p_lease`), D5 B (rows and no record: record complete, skip with one line; the
  scoper's default), D6 A, D7 A; `depends_on story:leader-exits-promptly-on-sigterm`.

## What a unit must not do

- Write Python in anything that stays in the repository: tools, tasks, gates, CI steps and test
  helpers are Rust or POSIX sh (U1's `scripts/postgres-lane.sh` is POSIX sh).
- Add a test that runs over 60 s: nextest kills it (`.config/nextest.toml`, `slow-timeout` with
  `terminate-after`). A test over 5 s is named with its time in the unit's report.
- Let a test step run over 120 s: the default step and the PostgreSQL lane run under
  `scripts/within.sh 120`; U1's cluster start stays outside that bound.
- Hold a pool job past 10 minutes; a job that needs longer names its time with `slot --timeout`,
  at most 30 minutes. A script that waits or measures (U1's lane script, U3's 20 evidence runs)
  starts without the `slot` prefix; `cargo` inside it takes a slot through the shim.
- Run the whole workspace's tests; run the tests of the crates the unit touched.
- Make a model call, a call to a real API or a network request in a test; loopback only.
- Commit `.pglane.docker.sh` or any harness outside `scripts/postgres-lane.sh`.
