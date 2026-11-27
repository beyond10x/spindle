---
format: aep.planning-md/3
id: review-result:wave31-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U1
relations:
- reviews: story:postgres-lane-kills-slow-tests
revision: 1
---
unit: story:postgres-lane-kills-slow-tests, findings cover 7fac9081 (implementor head 1e787b6e plus my test commit)
verdict: CONFIRMED
cases: executed 95→96 (lane), 25→32 (gate sh test), red 1
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 19 files in adv-scratch, plus 2 directories under the worktree's `target/` on the build node (listed in part 6)
needs-coordinator: the real CI `PostgreSQL` job was not run. A run at CI's 4 test threads on the build node stands in for it.

The main problem: under nextest, the three adversary modules' schema setup is no longer safe on a fresh database. CI starts a fresh database every run, so the lane can go red at random. The new case shows it in 8 s.

## 1. diff --stat (1e787b6e..HEAD)
```
 .../tests/spindle-kernel/adversary_w31_u1.rs       | 147 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   2 +
 scripts/gate-scripts.adversary-test.sh             |  40 ++++++
```
All three are test files. No implementation file was touched. Committed as 7fac9081.

## 2. Cases added
- **`adversary_w31_u1::adv_w31_the_once_modules_pass_one_process_per_case_on_postgres`** (spindle-kernel, ignored). The lane selects it because its name contains `on_postgres`.
  - It runs cases of `adversary_postgres`, `adversary_postgres_scan` and `adversary_postgres_named` the way nextest does: each case is its own process of the test binary, 6 side by side per case, 4 rounds, a fresh database each round. It asserts every process passes.
  - It is **red now**. Output of the case run alone:
    ```
    5 of 54 case processes failed on a fresh database, each a process of its own as under nextest:
    round 1: adversary_postgres::adv_two_stores_on_one_prefix_create_one_identity_exactly_once: exit status: 101
          ... panicked at crates/spindle-kernel/tests/spindle-kernel/adversary_postgres.rs:59:37:
          CREATE SCHEMA IF NOT EXISTS adv1_w19: db error
    round 1: adversary_postgres_scan::adv2_a_migrate_beside_a_serving_store_delays_nothing: exit status: 101
          ... adversary_postgres_scan.rs:47:37:
          CREATE SCHEMA IF NOT EXISTS adv2_w19: db error
    FAIL [   5.597s] (1/1) ... adversary_w31_u1::adv_w31_the_once_modules_pass_one_process_per_case_on_postgres
    ```
  - With 4 rounds it reported `4 of 72 case processes failed` in 8.1 s alone. It takes 28 s inside the lane: over 5 s, under 60 s.
  - It needs a role that may create databases. CI's `owner` role is the container's superuser, so it can.
- **`scripts/gate-scripts.adversary-test.sh`**: 7 new cases, green now. They check that:
  - `ci.yml` pins one nextest version and one digest, at least twice;
  - the postgres job installs nextest before `task check:postgres`;
  - `check:postgres` uses `--profile postgres`;
  - `--no-run` comes before `within.sh 120 {{.NEXTEST}}`;
  - the ten slowest are read from the postgres profile;
  - the only `slow-timeout` is the default profile's 30 s × 2.

  I broke each one on a copy and the matching case went red: postgres job nextest 0.9.142, the install step removed, `within.sh` removed, `NEXTEST_PROFILE: default`, and a `[profile.postgres] slow-timeout` of 300 s.

## 3. Suite, run after the cases existed
- Lane run (`task check:postgres` on a fresh PostgreSQL 17.12 with TLS):
  - `Summary [  54.287s] 96 tests run: 95 passed (2 slow), 1 failed`
  - `within: 65.365 s of 120 s, the command failed with status 100`
  - `task check:postgres exit 201`
- The red case's line in that run: `20 of 108 case processes failed`. That run still used 6 rounds; it is 4 now.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0. `cargo fmt -p spindle-kernel --check`: exit 0.
- `within.sh 120 cargo nextest run --locked -p spindle-kernel`: `444 tests run: 444 passed, 52 skipped`, `within: 10.996 s of 120 s`.
- `sh scripts/gate-scripts.adversary-test.sh`: `32 cases passed`.

## 4. Findings
1. **The schema setup races under nextest.** Files: `adversary_postgres.rs:67-70`, `adversary_postgres_scan.rs:60-63`, `adversary_postgres_named.rs:58-61`.
   - Measured: `std::sync::Once` only holds inside one process. With nextest running one process per test, `CREATE SCHEMA IF NOT EXISTS` fails with `duplicate key value violates unique constraint "pg_namespace_nspname_index"`.
     - Raw psql test (8 sessions starting at once, 30 rounds): 14 of 240 sessions refused.
     - The three modules alone under nextest on fresh databases: 7 of 10 rounds red.
   - What reaches it: the lane, with CI's fresh container on every run. In 6 full-lane runs the cases never collided, because cli and host tests start first and spread the kernel cases out. So it is an intermittent red, not a constant one.
   - Introduced: at the base the lane ran `cargo test`, one process per binary, so the `Once` held.
   - Fix to name: make the schema creation safe across processes (an advisory lock, or retry on the unique violation), or create the schemas in a setup step before the run.
2. **The timed run still compiles.** File: `Taskfile.yml:197`.
   - Measured: in every run (5 of 5) the `within.sh 120` command itself prints `Compiling spindle-control` and `Compiling spindle-cli`, 8.7 to 10.7 s.
   - Cause: `crates/spindle-control/build.rs:15` and `crates/spindle-cli/build.rs:14` use `rerun-if-changed` on `ui/dist`, which is missing, so they always rerun.
   - What reaches it: CI's postgres job builds no `ui/`, so this happens on every run. The story's acceptance says the compile time is not part of the run.
3. **One host test is close to the 60 s kill.** File: `crates/spindle-host/tests/spindle-host/postgres.rs:91` (`a_reader_counts_the_rows_before_a_reset_or_after_it_never_between`).
   - Measured: 29.1 s alone and 33.8 s and 53.5 s in 32-thread lanes. At 4 threads it was killed once (`TIMEOUT [  60.051s]`, `within: 101.542 s of 120 s`, task exit 201) and passed once (`within: 59.311 s`).
   - `adv2_a_scan_answers_over_a_million_events` also went from 14.5 s to 32.9 s under load.
   - Introduced: the kill exists only since this change. The test was this slow at the base too.

## 5. Attacked, not broken
- **Selection:** the old seven commands and the new nextest filter select the same 95 tests, with no duplicates (`diff` IDENTICAL). Each of the seven listings exited 0.
- **Kill at 60 s:** a test over 60 s shows `TIMEOUT`, nextest exits 100 and the task fails (201). The postgres profile inherits the 30 s × 2 timeout.
- **CI nextest version:** both jobs use 0.9.143 with the same digest, which is also the host's version.
- **Shortened `adv3_drawn_command_sequences_scan_as_sqlite_does`:** a scan that drops the last page is still caught, because there are scans after version 1,000 (every 20th group, near page edges, and after the last). 2,400 groups cross no more page boundaries than 1,500 do. Not run against a mutated build.
- **Shortened scan-cost test:** a mutant whose cost is a constant plus a linear term has the same ratio at 10,000 and at 50,000, so dropping 50,000 loses nothing there. Realistic quadratic mutants already exceed 2× at 10,000. Not run against a mutated build.
  - Separate, pre-existing, and not in the findings: the normal ratio is 1.7 against a threshold of 2.0, so that test has little margin under load.

## 6. Paths written outside the worktree
- `<local path>`: `parity.sh`, `parity.log`, `harness.sh`, `race.sh`, `race.log`, `raceB.sh`, `raceB.log`, `ci4.sh`, `ci4.log`, `ci4b.sh`, `ci4b.log`, `red.sh`, `red.log`, `red2.log`, `suite.sh`, `suite.log`, `gate.sh`, `gate.log`, `commit-msg.txt`
- On the build node, under the worktree's `target/`: `target/adv-w31/` (`ci4b.out`, `suite.out`) and `target/adv-w31-parity/`

## 7. Findings block
```findings
- file: crates/spindle-kernel/tests/spindle-kernel/adversary_postgres.rs
  line: 67
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: under nextest each case is its own process, so the Once around CREATE SCHEMA IF NOT EXISTS in adversary_postgres, adversary_postgres_scan and adversary_postgres_named no longer serialises it, and on CI's fresh database the cases race into a pg_namespace unique violation (red case adversary_w31_u1, 7 of 10 module-only rounds red)
- file: Taskfile.yml
  line: 197
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the run timed by within.sh 120 recompiles spindle-control and spindle-cli (8.7-10.7 s, every run) because their build scripts rerun while ui/dist is missing, and CI's PostgreSQL job builds no ui, so compile time is part of the bounded run
- file: crates/spindle-host/tests/spindle-host/postgres.rs
  line: 91
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a_reader_counts_the_rows_before_a_reset_or_after_it_never_between takes 29 s alone and 34-54 s in the lane, and at CI's 4 test threads it was killed at 60 s in 1 of 2 runs, failing the lane
```
