---
format: aep.planning-md/3
id: story:postgres-lane-kills-slow-tests
kind: story
status: draft
title: The PostgreSQL lane kills a test at 60 s and ends within 2 minutes
summary: task check:postgres runs under cargo-nextest with the repository slow-timeout, prints its ten slowest tests and fails over 120 s.
relations:
- serves: vision:spindle
- informed_by: story:test-step-within-budget
scope:
- confidence: cited
  path: .config/nextest.toml
- confidence: cited
  path: .github/workflows/ci.yml
- confidence: cited
  path: Taskfile.yml
revision: 2
---
# The PostgreSQL lane kills a test at 60 s and ends within 2 minutes

## Outcome

`task check:postgres`, the lane of CI's `PostgreSQL` job, runs its tests under cargo-nextest with
the repository's `slow-timeout` and `terminate-after` (`.config/nextest.toml`), as the default test
step does: a test over 60 s is killed and counts as failed, the lane prints its ten slowest tests,
and it fails when its test run, after a separate build step, takes over 120 s, locally and in CI.
It selects the same tests the seven `cargo test` commands select today, the two `--skip`s
included.

## Why

- `check:postgres` runs `cargo test` (`Taskfile.yml:183-189`), so no test there is killed at 60 s,
  the lane prints no slowest tests and nothing bounds its wall time; the default step has run under
  cargo-nextest and `scripts/within.sh 120` since `story:test-step-within-budget`
  (`check:the_tests_pass_within_the_budget`, `Taskfile.yml:61-66`).
- CI's `PostgreSQL` job (`.github/workflows/ci.yml:137-203`) installs no cargo-nextest.
- Wave 31 adds PostgreSQL tests that start one or two `run` processes each
  (`story:several-writers-one-postgres`, `story:spec-change-over-stored-rows`,
  `story:event-egress-webhook`, `story:dead-letters-kept-and-replayed`). Found by the scoper of
  `story:several-writers-one-postgres`, 2026-11-27.

## Acceptance

- `postgres::a_probe_over_60_s_is_killed`: a probe test that sleeps 61 s, in a `postgres::` module
  and run by `task check:postgres`, fails as timed out; the change's report shows the run. The probe
  is not committed. This is how the default step proved its kill (`story:test-step-within-budget`,
  first acceptance line); no committed check in the default step proves it.
- The lane's test run is `cargo nextest run` under `scripts/within.sh 120`, after a separate build
  step (`cargo nextest run --no-run`) whose compile time is not part of the run;
  `check:within_fails_over_its_bound` already proves the timer.
- On the pull request's `PostgreSQL` job the lane ends within 120 s and prints its ten slowest
  tests; its log shows both.
- The nextest run selects the tests the seven `cargo test` commands select on `wave/31`'s fork
  point, and no other; the change's report lists both counts.

## Scope

Derived 2026-11-27 by the coordinator from the scoper of `story:several-writers-one-postgres` and
the tree at 8eaea47a. Every line is **cited** (read from the story or the tree) or **inferred** (a
reading that could be wrong).

- **Primary surface:** gate wiring: `Taskfile.yml` task `check:postgres`, `.config/nextest.toml`,
  `.github/workflows/ci.yml` job `postgres` — cited
- **Files:** `Taskfile.yml` `check:postgres` `:178-189` (seven `cargo test` commands `:183-189`),
  `test:slowest` `:68-71` (reads `target/nextest/default/junit.xml` only) — cited
- **Files:** `.config/nextest.toml` (`[profile.default]` `slow-timeout = { period = "30s",
  terminate-after = 2 }`, JUnit `junit.xml`) — cited
- **Files:** `.github/workflows/ci.yml` job `postgres` `:137-203`: install cargo-nextest as the
  `Gate` job does (`:70-83`) — cited
- **Confidence:** high for the three files; the filter expression is the implementor's
- **Would collide with:** any unit touching `Taskfile.yml` (`story:spindle-image-published` the
  `desc` at `:49-52`, `story:dead-letters-kept-and-replayed` `OBSERVE_ROOTS` at `:7`: other hunks)
  or `.github/workflows/ci.yml`
- **Safety:** under nextest each test runs in its own process; a fixture built once per binary
  under `cargo test` is built once per test, so the switch can make the lane slower before it is
  bounded — inferred

## Decided when scoped

Taken 2026-11-27 on the coordinator's recommended option, each reversible.

- D1 A: one `cargo nextest run --run-ignored all` over `spindle-kernel`, `spindle-host` and
  `spindle-cli` with one filter expression equal to today's seven filters, under
  `scripts/within.sh 120`. Not taken: seven nextest runs, each timed apart.
- D2 A: a nextest profile `postgres` in `.config/nextest.toml` that inherits the default's
  `slow-timeout` and writes its own JUnit report, so the lane does not overwrite the default
  step's; `test:slowest` reads the profile it is given.

## Not established when scoped

- The lane's wall time today. If it runs over 120 s under nextest, that is a test-budget defect of
  its own, fixed before the PostgreSQL tests of wave 31 land.
