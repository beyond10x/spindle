---
format: aep.planning-md/3
id: story:postgres-lane-runs-locally
kind: story
status: implemented
title: The PostgreSQL lane starts its own cluster with the CI's options
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: .github/workflows/ci.yml
- confidence: cited
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: scripts/check-postgres-options.sh
- confidence: inferred
  path: scripts/postgres-lane.sh
- confidence: inferred
  path: scripts/postgres-lane.test.sh
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T04:16:53Z", actor: "human:operator", revision: 5, decided_on: {"recorded":{"review_outcome":11}}}
- {from: "proposed", to: "active", at: "2026-11-28T04:16:53Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"review_outcome":11}}}
- {from: "active", to: "implemented", at: "2026-11-28T08:49:46Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"test_result":2,"review_outcome":14}}}
---
## Outcome

`task check:postgres` starts its own throwaway PostgreSQL cluster when no database is given, with
the same server options as the CI job (`.github/workflows/ci.yml`, the postgres step: TLS,
`max_locks_per_transaction=256`, `max_connections=300`), and removes it afterwards. One sh script in
the repository holds those options and both the task and the CI job read them (D1 A). The cluster is
PostgreSQL 13 or later from `PATH` or `SPINDLE_PG_BIN`; the script prints its major version and
warns when it is not CI's 17 (D3 A). With `SPINDLE_TEST_DATABASE_URL` set, the task starts nothing
and runs as in 0.26.0 (D2 A). `max_connections` holds the lane's peak of open connections with
`.config/nextest.toml` as `main` has it once `integration/2026-11-28-1` has landed (the postgres
profile's `test-threads = 12`, heavy group `max-threads = 3`).

## Why

Wave 31 ran the PostgreSQL lane locally through a harness outside the repository. Its options
drifted from CI's: at 147 cases the local cluster still had the default 100 connections and 2 of 3
lane runs failed on "too many clients already" while CI had been raised to 300. The rule "the local
gate equals the forge" asks for runner differences to be written in the repository.

## Acceptance

- With `initdb` and `pg_ctl` of PostgreSQL 13 or later on `PATH` or in `SPINDLE_PG_BIN`, and no
  `SPINDLE_TEST_DATABASE_URL`, `task check:postgres` passes and leaves no cluster process or
  directory behind: at its end the task asserts that the cluster directory under `target/` is gone
  and the postmaster PID is not alive (D7 A). The nextest run inside it stays under
  `scripts/within.sh 120` (each test killed at 60 s); the cluster start is outside that bound.
- `scripts/postgres-lane.test.sh` (POSIX sh, task `check:postgres-lane-script`, on a host with
  `initdb` and `pg_ctl`, under 60 s) runs the lane script twice: around `false`, where the script
  exits non-zero, and around `sleep 30`, sent SIGINT once the cluster answers `pg_isready`. After
  each it asserts that the cluster directory is gone and the postmaster PID is not alive; red first
  on a copy of the script without its `trap`.
- The same check observes the version handling (D3 A): with binaries of a major other than 17 on
  `PATH`, the script prints that major and a warning naming 17; with `SPINDLE_PG_BIN` naming a
  directory of wrappers that log each call and report major 17, the log shows the cluster was made
  and started by those wrappers, and no warning is printed.
- With `SPINDLE_TEST_DATABASE_URL` set, `task check:postgres` starts no cluster (no `initdb` or
  `pg_ctl` process, no directory under `target/postgres-lane.*`).
- The script exports `SPINDLE_TEST_DATABASE_CA`, the CA of a server certificate it issued for
  `localhost` and `127.0.0.1`, and the hosted (TLS, `sslmode=require`) cases run in the lane.
- `scripts/check-postgres-options.sh` in `task check` (no database) fails when the CI PostgreSQL
  step names a `-c` server option other than through the script, when the script lacks `ssl=on` or
  `max_locks_per_transaction=256`, or when its `max_connections` is below 300, so script and CI
  carry one value; red first on a copy of `ci.yml` with a literal `-c max_connections=100` and on a
  copy of the script with `max_connections=200`.
- One `task check:postgres` run on the script's cluster, after `integration/2026-11-28-1` has
  landed, samples the count of `pg_stat_activity` rows every 0.5 s (a sampler beside the run,
  without the `slot` prefix); its peak, below the script's `max_connections`, is written in the
  story's closing record.

## Decided when scoped

Taken 2026-11-28 on the scoper's default, each reversible.

- **D1 Where the options live and how CI reads them.**
  A (default, taken): `scripts/postgres-lane.sh options` prints the `-c name=value` arguments; CI's
  `docker run` line takes them from it, so CI keeps its container.
  B: CI drops the container and runs the same `pg_ctl` script after installing PostgreSQL 17 from
  apt (bigger CI change, one more place a version can drift).
  C: CI keeps literal options and only the check compares them.
- **D2 What `check:postgres` does with and without a database.**
  A (default, taken): drop `requires`. When `SPINDLE_TEST_DATABASE_URL` is set, it runs as today.
  When it is unset, it runs `sh scripts/postgres-lane.sh run -- task check:postgres:run`, and that
  inner task holds today's body.
  B: always start its own cluster, even in CI.
- **D3 PostgreSQL version.** The distribution's `initdb` is 18.
  A (default, taken): accept any major 13 or later (`AGENTS.md`'s floor), print it, and warn when
  it is not 17. A `SPINDLE_PG_BIN` variable may name a PostgreSQL 17 `bin/` directory.
  B: refuse anything that is not 17.
- **D4 Login and database.** Mirror the image.
  A (default, taken): `initdb -U owner --pwfile` (`owner`), `--auth-host=scram-sha-256
  --auth-local=trust`, then `createdb spindle`. The URL is
  `postgres://owner:owner@127.0.0.1:<port>/spindle`.
  B: trust everywhere (tests that log in as a created role would no longer test its password).
- **D5 Port and socket.**
  A (default, taken): a free port picked by the script, `listen_addresses=127.0.0.1`, and
  `unix_socket_directories` inside the cluster directory, so several runs of the lane on one host
  do not collide.
  B: fixed 5432.
- **D6 Cluster directory.**
  A (default, taken): `mktemp -d target/postgres-lane.XXXXXX` (ignored by `/target/`). The key file
  is mode 0600 and owned by the user who runs it; CI's `chmod 644` is only for the container and is
  not copied.
  B: `$TMPDIR`.
- **D7 Where the equality check runs.**
  A (default, taken): a sh check in `task check` (no database needed, like
  `scripts/check-ess-pin.sh`). The "leaves nothing behind" assertion runs at the end of
  `check:postgres` itself: the directory is gone and the postmaster PID is not alive.
  B: a Rust test that reads `ci.yml`.
- **D8 Pool.** One option. The lane script starts a server and waits, so it runs without the
  `slot` prefix ("Scripts that do not build stay off slot"). The `cargo nextest` inside it takes
  slots through the shim. The cluster start stays outside `scripts/within.sh 120`.

## Scope

Derived 2026-11-28 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** the PostgreSQL lane's gate wiring: `Taskfile.yml` `check:postgres` and the
  `postgres` job of `.github/workflows/ci.yml` — cited (story Outcome)
- **Files:** `Taskfile.yml:180-197` (`check:postgres`: `requires: vars: [SPINDLE_TEST_DATABASE_URL,
  SPINDLE_TEST_DATABASE_CA]` at 182-183 refuses to start without a database, so it must become
  optional) — cited
- **Files:** `.github/workflows/ci.yml:188-214` (step "PostgreSQL 17 on loopback…": openssl CA and
  server certificate, `docker run … postgres:17 … -c ssl=on -c ssl_cert_file -c ssl_key_file
  -c max_locks_per_transaction=256 -c max_connections=300`, owner/owner, database `spindle`) — cited
- **Files:** `scripts/postgres-lane.sh`, new: POSIX sh that holds the server options, generates the
  CA and server certificate as CI does, runs `initdb`/`pg_ctl` in a directory under `target/`,
  exports `SPINDLE_TEST_DATABASE_URL`/`SPINDLE_TEST_DATABASE_CA`, runs the command and stops and
  removes the cluster on exit, INT and TERM — inferred (the name is a proposal)
- **Files:** a check that compares the options, beside `scripts/check-ess-pin.sh` (same pattern:
  sed over `.github/workflows/ci.yml` and a repository file), plus its `Taskfile.yml` task under
  `check` — inferred
- **Symbols:** `SPINDLE_TEST_DATABASE_URL`, `SPINDLE_TEST_DATABASE_CA` (read by about 20 test modules
  under `crates/spindle-{kernel,host,cli}/tests/`, none of which changes) — cited
- **Also likely:** `scripts/within.sh` (read, not changed: the 120 s bound keeps wrapping only the
  nextest run, and the cluster start stays outside it) — inferred
- **Documents:** `AGENTS.md:132-136` (the PostgreSQL paragraph: how the lane gets its database),
  `CHANGELOG.md` — cited (`AGENTS.md`), inferred (`CHANGELOG.md`)
- **Confidence:** high for the Taskfile and CI hunks (the story names them), medium for the new
  file names
- **Would collide with:** any unit that edits `check:postgres` in `Taskfile.yml` (its `TESTS`
  filter or `requires`), the `postgres` job of `ci.yml`, the ESS pin lines of `Taskfile.yml`/`ci.yml`
  (same files, other hunks), or `AGENTS.md`'s PostgreSQL paragraph. Not `.config/nextest.toml`, unless
  decision D7 below picks B.
- **Safety fact:** the tests need only what CI's container gives them: a superuser `owner` with
  password `owner`, a database literally named `spindle`
  (`crates/spindle-cli/tests/spindle-cli/adversary_postgres.rs:407`,
  `crates/spindle-kernel/tests/spindle-kernel/adversary_postgres_scan.rs:566` hard-code
  `GRANT TEMPORARY ON DATABASE spindle`), password login on loopback for roles the tests create
  (`as_role`), and TLS on the same port for the hosted cases (`sslmode=require`,
  `crates/spindle-kernel/src/postgres.rs:762`). A cluster that offers exactly these runs every
  case unchanged — step 2, unproven
- **Coupled values:** `max_connections=300` must cover the lane's concurrency as
  `.config/nextest.toml` sets it after `integration/2026-11-28-1` (it adds `test-threads = 8` to the
  default profile and three `profile.postgres.overrides`; the postgres profile has
  `test-threads = 12` and the heavy group `max-threads = 3`); not measured — inferred
