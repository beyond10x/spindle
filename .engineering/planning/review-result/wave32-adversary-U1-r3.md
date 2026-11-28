---
format: aep.planning-md/3
id: review-result:wave32-adversary-U1-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 32 U1
relations:
- reviews: story:postgres-lane-runs-locally
revision: 1
---
Round 3: I found no bypass that an ordinary workflow edit reaches, so the verdict is REFUTED. None of my cases changed, and all 11 options cases now pass at 0194edbbd.

```
unit: story:postgres-lane-runs-locally, findings cover 0194edbbd (branch unit/w32-pg-local, my last commit 34d0212a2 beneath it)
verdict: REFUTED
cases: executed 82→82, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 path (listed in part 6)
needs-coordinator: none
```

**1. `git --no-pager diff --stat 0194edbbd HEAD`**: empty. I added no files and made no commits in this round, and the working tree is clean.

`git diff 34d0212a2 0194edbbd -- scripts/postgres-lane.adversary-test.sh Taskfile.yml` is empty too, so none of my cases or their tasks was touched.

**2. Cases added**: none. I found no ordinary bypass to write one for.

**3. Suite run**: one slot job on the build node, wall time 19 s, exit 0.
- `check:the_postgres_options_are_one`: `18 cases passed`, 1 s.
- `check:postgres-lane-script`: `32 cases passed`, 5 s.
- `check:postgres-options-adversary`: `11 cases passed`, under 1 s. Both round-2 cases, second-job and service-container, are now green.
- `check:postgres-lane-adversary`: `21 cases passed`, 9 s.

**4. Findings**: none new. The round-2 comment false alarm (`check-postgres-options.sh:77`) still stands as a note; it fails safe.

**5. Attacked and could not break**
- **Another job running the PostgreSQL tests directly:** a job that runs `cargo nextest` itself still needs `SPINDLE_TEST_DATABASE_URL`, which the check refuses outside the postgres job.
- **Taskfile route:** adding `check:postgres` to Taskfile's `check` makes the Gate job start its own cluster with the script's options. That is no option drift.
- **Inside the postgres job:**
  - a job-level `env:` URL;
  - a matrix image tag;
  - a `docker restart` or `docker cp` step.

  Each is refused, either by the pinned lines or by the ban on other docker commands.
- **A local composite action** (`.github/actions/*/action.yml`), which the check does not read, could still start PostgreSQL for another job. I did not count it as ordinary, because the repository has no `.github/actions` and no workflow uses a local action today. It would take a new mechanism.
- **Not pursued again**, as already judged unreachable: `run: >` folding and the obfuscated forms.

**6. Paths written outside the worktree** (under `<local path>`):
- `r3-suite.out`

**7.**
```findings
[]
```
