---
format: aep.planning-md/3
id: review-result:wave32-adversary-U1-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 32 U1
relations:
- reviews: story:postgres-lane-runs-locally
revision: 1
---
Round 2: the implementor's fixes hold, but the options check can still be bypassed in two ways, each with a red case committed (34d0212a2). Both are in `check-postgres-options.sh` and both come from this unit's diff.

```
unit: story:postgres-lane-runs-locally, findings cover 90ed1019e (round-2 cases committed on top as 34d0212a2, branch unit/w32-pg-local)
verdict: CONFIRMED
cases: executed 75→77, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 more paths in my scratch directory (listed in part 6)
needs-coordinator: `task check` is now red, because it runs check:postgres-options-adversary, which holds these two cases
```

`git diff 6bc314a45 90ed1019e -- scripts/postgres-lane.adversary-test.sh` is empty: none of my round-1 cases was weakened. All 10 round-1 red cases are now green, including `second` (port-collision retry). The locale note is fixed as well.

**1. `git --no-pager diff --stat 90ed1019e HEAD`**
```
 scripts/postgres-lane.adversary-test.sh | 47 +++++++++++++++++++++++++++++++++
```
This is a test file only.

**2. Cases added** (options part of the adversary test). I ran them alone first: `postgres-lane-adversary: 2 of 11 case(s) failed`.
- **second-job:** a new `postgres-13:` job at the end of `ci.yml`. It runs `docker run ... postgres:13 -c max_connections=100` and then `task check:postgres`. The check passes it.
- **service-container:** the `postgres` job gets a `services: db: image: postgres:17` container on `127.0.0.1:5433`, which runs on the image defaults (no TLS, 100 connections). The `SPINDLE_TEST_DATABASE_URL` written into the job's environment names port 5433. The checked `docker run` still starts, but nothing uses it. The check passes it.
- Both copies pass actionlint 1.7.7 (run through docker), so they are valid workflows someone could commit.

**3. Suite run**: one slot job on the build node, wall time 20 s.
- `check:the_postgres_options_are_one`: `13 cases passed`.
- `check:postgres-lane-script`: `32 cases passed`, 4 s.
- `check:postgres-options-adversary`: `2 of 11 case(s) failed`, exit 201.
- `check:postgres-lane-adversary`: `21 cases passed`, 8 s.

**4. Findings**
- **Only the job named `postgres` is read** (`check-postgres-options.sh:47`). A second job that starts PostgreSQL with its own options is not inspected. What reaches it: a version job such as PostgreSQL 13, the floor `AGENTS.md` names.
- **The check never ties the tests' URL to the container it checked** (`check-postgres-options.sh:64`). It pins the `docker run` line, but the URL written into the job's environment can point at another server, such as a `services:` container, which is GitHub's usual way to run PostgreSQL. A fix: pin the two lines that write `SPINDLE_TEST_DATABASE_URL` and `SPINDLE_TEST_DATABASE_CA`, and refuse `services:` and any other job that runs `check:postgres`.
- **Judgement, no case: false alarms.** A YAML or shell comment, or heredoc text, in the postgres job that mentions `docker run` or `options` makes the check refuse a correct workflow. It fails safe, so I did not add a red case to the gate for it.

**5. Attacked and could not break**
- **Retry loop:** a real port collision with a second lane is retried and succeeds. A failed start now prints the end of the server log.
- **Locale probe:**
  - Both spellings, `utf8` and `UTF-8`, are matched.
  - Without `en_US` it falls back to `C.utf8` with one warning, and a missing `locale` command falls back to `C`.
  - `lc_messages` follows `--locale`, so the English bind message the retry looks for is never translated.
- **Lines the check refuses:** `options+=`, `docker create` / `exec` / `compose`, `psql`, `POSTGRES_INITDB_ARGS`, and `-c` written in any form.
- **Not pursued:**
  - `run: >` (folded): a value on its own line after the pinned `docker run` would get through. But a working folded step needs a blank line between every command, and its `\` continuations break, so I judged it not reachable.
  - A `log_destination=syslog` sample config: no distribution I know ships one.
  - An `initdb` that rejects a locale `locale -a` lists: I could only build this, not show anyone reaches it.

**6. Paths written outside the worktree** (all under `<local path>`):
- `r2-options-1.out`, `r2-suite.out`
- `commit-msg-2.txt`
- `lint-copies.sh`
- `lint-copies/`, which holds three workflow copies: `unchanged`, `second-job` and `service-container`, each at `.github/workflows/ci.yml`.

On the build node, the red run left its outputs in the tree's `target/postgres-options-adversary/`.

**7.**
```findings
- file: scripts/check-postgres-options.sh
  line: 47
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the check reads only the job named postgres, so a second workflow job that starts postgres:13 with a literal -c max_connections=100 and runs task check:postgres passes
- file: scripts/check-postgres-options.sh
  line: 64
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the check pins the docker run line but not the database the tests reach, so a services container on image defaults with SPINDLE_TEST_DATABASE_URL pointed at it passes
- file: scripts/check-postgres-options.sh
  line: 77
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a comment or heredoc line in the postgres job that names docker run or options makes the check refuse a correct workflow
```
