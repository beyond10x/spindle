---
format: aep.planning-md/3
id: review-result:wave32-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 32 U1
relations:
- reviews: story:postgres-lane-runs-locally
revision: 1
---
I made the unit's checks fail: 10 of my cases are red, and all 10 come from this unit's own diff. The options check lets a CI server option through when it is written any other way than `-c name=value`. On the build node, a lane whose first port is already taken fails instead of retrying.

```
unit: story:postgres-lane-runs-locally, findings cover 78e1ff76c (tests committed on top as 6bc314a45, branch unit/w32-pg-local)
verdict: CONFIRMED
cases: executed 34→64, red 10
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths in my scratch directory (listed in part 6)
needs-coordinator: two task-wiring hunks are mine in Taskfile.yml, and the new tasks are not in `task check` yet (see part 1)
```

The red count is cases; the origin line counts findings, so 10 red cases make 3 of the 4 findings. The fourth is a judgement call with no case. `executed 34` is the implementor's own count: 27 lane cases plus 7 options cases.

**1. `git --no-pager diff --stat 78e1ff76c HEAD`**
```
 Taskfile.yml                            |  10 +
 scripts/postgres-lane.adversary-test.sh | 335 ++++++++++++++++++++++++++++++++
```
- `Taskfile.yml` is the one non-test file. It only adds two tasks that run my script (`check:postgres-options-adversary`, `check:postgres-lane-adversary`), which the brief allows.
- I did not add them to `task check`; whether to do that is your call.
- No implementation file was edited.
- The staged diff passed the public-repo grep with no output.

**2. Cases added** (`scripts/postgres-lane.adversary-test.sh`, POSIX sh)

`options` part: 9 cases, all on copies under `target/`, run first and alone. 8 are red; the control is green.
- **Red, `check-postgres-options.sh` passes CI's step with these appended to the docker line:**
  - `-c 'max_connections=100'`
  - `"-c" "max_connections=100"`
  - `-cmax_connections=100`
  - `-c \` with the value on the next line
  - `--max_connections=100`
  - `-N 100`
- **Red, two more gaps:**
  - The docker line drops `"${options[@]}"`, so the step still reads the options but never passes them.
  - The script adds `echo "max_locks_per_transaction=64"` after 256; postgres keeps the last value.
- **Green control:** the unchanged copy passes.
- **Captured red output (first run):** `postgres-lane-adversary: 8 of 9 case(s) failed`, wall time 9 s.
- **The bypass forms really change the server:** a docker probe of `postgres:17` read back max_connections 77, 78, 79 and 80 for `-cmax_connections=77`, `--max_connections=78`, `-N 79` and `-c 'max_connections=80'`.

`lane` part: 21 cases, on the build node's PostgreSQL 17.12, about 11 s. 19 are green and 2 are red.
- **Green:**
  - keeps-dir: `lane` fails on a copy of the script whose cleanup removes nothing.
  - keeps-postmaster: `lane` fails on a copy whose cleanup stops nothing.
  - SIGTERM to `run`.
  - Ctrl-C sent to `lane`'s whole process group (`setsid`).
  - `initdb` fails after making part of the data directory.
  - `pg_ctl start` reports failure after the server came up.
  - Empty `SPINDLE_TEST_DATABASE_URL`: `lane` starts its own cluster, and the command gets a full URL.
- **Red, both in case `second`:** a second lane whose first `pg_ctl start` is pointed at the port the first lane holds. Output:
  ```
  postgres-lane-adversary: FAIL: second: a lane whose first port another lane holds exited 1: ...
  LOG:  redirecting log output to logging collector process
  postgres-lane: pg_ctl start failed
  postgres-lane-adversary: 2 of 21 case(s) failed
  ```

Mutant proofs (copies under the node's `target/`):
- **M1, the unit's suite misses the D7 check:** with `lane`'s left-behind check removed, `postgres-lane.test.sh` still reports `27 cases passed`. My keeps-dir and keeps-postmaster cases go red on it.
- **M2, my `second` case is correct:** with `-c logging_collector=off` added to the flags, my lane cases report `21 cases passed`.

**3. Suite run** (after the cases existed): one `slot --timeout 5m --tokens 2` job on the build node, wall time 20 s.
- `check:the_postgres_options_are_one`: `7 cases passed`, exit 0.
- `check:postgres-lane-script`: `27 cases passed`, exit 0, 4 s.
- `check:postgres-options-adversary`: `8 of 9 case(s) failed`, exit 201.
- `check:postgres-lane-adversary`: `2 of 21 case(s) failed`, exit 201, 11 s.

**4. Findings**
- **The options check only catches `-c name=value` written with spaces** (`scripts/check-postgres-options.sh:28`).
  - Measured: 6 red cases.
  - What reaches it: anyone writing a server option into `ci.yml`. Quoting it, as in `-c 'x=y'`, is ordinary. postgres takes every one of these forms.
  - Fix: refuse any option-like token after `postgres` or after `"${options[@]}"` on the docker line, or require that line to end exactly in `sh "${options[@]}"`.
- **Reading the options is not the same as passing them** (`check-postgres-options.sh:33`).
  - The check only looks for the string `scripts/postgres-lane.sh options` in the step. CI can drop `"${options[@]}"` and still pass, then run on the defaults (100 connections), which is the drift the story was written against.
- **A value overridden later in the script passes** (`check-postgres-options.sh:42-46`).
  - `grep -qx` looks for the wanted value anywhere, not the value postgres ends up with. Severity is a note: it takes a deliberate second line.
- **The port-collision retry never fires on the build node, and a failed start says nothing useful** (`scripts/postgres-lane.sh:190`).
  - The node runs Amazon Linux 2023, whose `postgresql.conf.sample` sets `logging_collector = on`. The bind error goes to `data/log/`, so the `grep` on `server.log` finds nothing, and `pg_ctl start failed` follows on the first attempt. Cleanup then deletes the real log.
  - What reaches it: a port collision on the build node, where the documented workflow runs the lane. Ports are drawn from 20000-59999, and the node's ephemeral range is 32768-60999, so other loopback traffic can also hold the drawn port.
  - Fix: add `-c logging_collector=off` to the flags (shown by M2).
- **Judgement, not tested:** `initdb --locale=C` (`postgres-lane.sh:172`).
  - CI's `postgres:17` image initialises its cluster with `en_US.utf8`, so text ordering differs between the local lane and the forge. No test was found that depends on it. Severity is a note.

**5. Attacked and could not break**
- **Cleanup:** cleanup works after SIGTERM, after Ctrl-C to the group, and after `initdb` or `pg_ctl start` fails.
- **Two lanes at once:** each gets its own directory and socket.
- **Empty or set `SPINDLE_TEST_DATABASE_URL`:** both behave correctly.
- **Deviation 3, the CI runner path:**
  - From the runner-images readme and install script: `ubuntu-24.04` installs `postgresql-16` (16.16) from pgdg, which means `/usr/lib/postgresql/16/bin`. That holds, though nothing ran on the forge.
  - `env --default-signal` (deviation 5) needs GNU coreutils 8.31 or later. It works on the build node; that 24.04's coreutils is new enough is my assumption, not checked.
- **Deviation 1, the build node:** confirmed. The node has `initdb` 17.12 at `/usr/bin`.
- **SIGTERM to `lane`'s PID alone:** `lane` swallows it. Not raised: a probe showed `task` 3.54 does the same with a TERM to its own PID.
- **A second step named `PostgreSQL…`:** it is also checked. Renaming the step makes the check fail, which is the safe direction.

**6. Paths written outside the worktree** (all in `<local path>`):
- `taskprobe/probe.sh`, `taskprobe/Taskfile.yml`, `taskprobe/child.sh`, `taskprobe/log`, `taskprobe/task.out`
- `forms-probe.sh`, `forms-probe.out`
- `mutants.sh`, `mutants.out`
- `suite.sh`, `suite.out`
- `adv-options-1.out`, `adv-lane-1.out`
- `commit-msg.txt`

On the build node's mirror of the tree, the red runs left their outputs in `target/postgres-options-adversary/` and `target/postgres-lane-adversary/`. The probe's docker containers were removed. I did not take a session lease.

**7.**
```findings
- file: scripts/check-postgres-options.sh
  line: 28
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a server option in CI's PostgreSQL step passes the check when written as -c 'x=y', "-c" "x=y", -cx=y, -c continued on the next line, --x=y or -N n, and postgres:17 applies every one of these forms
- file: scripts/check-postgres-options.sh
  line: 33
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the check only looks for the string "scripts/postgres-lane.sh options" in the step, so a docker line that drops "${options[@]}" passes and CI runs on the default 100 connections
- file: scripts/check-postgres-options.sh
  line: 43
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: grep -qx accepts max_locks_per_transaction=256 when the script names a smaller value after it, and postgres keeps the last value
- file: scripts/postgres-lane.sh
  line: 190
  category: concurrency
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: on a host whose postgresql.conf turns logging_collector on (the build node, Amazon Linux 2023) the bind error never reaches server.log, so a taken port fails the lane on the first try without retrying and the real log is deleted by cleanup
- file: scripts/postgres-lane.sh
  line: 172
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: initdb --locale=C differs from CI's postgres:17 cluster (en_US.utf8), so text ordering in the local lane can differ from the forge
```
