---
format: aep.planning-md/3
id: story:planning-only-prs-skip-tests
kind: story
status: implemented
title: A pull request that changes only planning or prose skips the build and test jobs
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: .github/workflows/ci.yml
- confidence: cited
  path: .github/workflows/pages.yml
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: scripts
- confidence: cited
  path: scripts/changed-paths.sh
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:32Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"test_result":1,"review_outcome":20}}}
---
# A pull request that changes only planning or prose skips the build and test jobs

## Outcome

A pull request whose changed files are all planning or prose files (`.engineering/`, `*.md`
outside `website/`) passes its required checks (`Gate`, `macOS`, `PostgreSQL`, `Build
documentation`) without building or testing: each required job runs, finds only such paths, and
exits 0. A change under `website/` and planning or prose files only still runs the documentation
check and the site build, and skips the build and test steps of the code jobs.

One POSIX sh script, `scripts/changed-paths.sh`, holds the path list and classifies a list of
changed paths as `planning`, `docs` or `code`; every job of `.github/workflows/ci.yml` and
`.github/workflows/pages.yml` calls it, and no workflow repeats the list. The ruleset is unchanged.

Today every pull request runs the full gate: PR 61 changed one file under `.engineering/` and its
`CI` run took 18 minutes (2026-11-22T15:25:58Z to 15:44:15Z).

## Acceptance

- `task check:changed_paths_are_classified` runs the script's test cases: `.engineering/` and
  `README.md` paths only answer `planning`; adding one `website/` file answers `docs`; adding any
  other path (one `.rs` file, `Taskfile.yml`, a workflow file) answers `code`.
- `scripts/changed-paths.sh` is the only file that holds the path list: `.github/workflows/ci.yml`
  and `.github/workflows/pages.yml` call it and hold no path pattern of their own.
- On a planning-only pull request, `Gate`, `macOS` and `PostgreSQL` (`ci.yml`) each report success
  within 2 minutes with no build or test step run; the run's job log shows it.
- On a planning-only pull request, `Build documentation` (`pages.yml`) reports success without
  building the site; the job log shows it.
- On a pull request that changes one `website/docs/` page, `Build documentation` builds the site
  and runs `docs-check`.
- On a pull request that changes one `website/docs/` page, `Gate`, `macOS` and `PostgreSQL` report
  success with no build or test step run.
- A pull request with one changed `.rs` file runs every job as today.

## Origin

`~/beyond10x/AGENTS.md` § "Planning-only pull requests skip the tests" (Timo, 2026-11-21, "ROLL
the 5", proposal 3): built as a story ahead of the next feature.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `.github/workflows/` (CI configuration, no crate) — cited
- **Files:** `.github/workflows/ci.yml` — cited: jobs `Gate` (`check`, line 19) and `macOS` (`macos`, line 163) that the story names
- **Files:** `.github/workflows/pages.yml` — cited: job `Build documentation` (`build`, line 17), which must still run on `website/` changes and skip the site build otherwise
- **Symbols:** job ids `check`, `macos`, `postgres` in `ci.yml`; `build` in `pages.yml`; one path list (`.engineering/`, `*.md` outside `website/`) — cited
- **Also likely:** `scripts/` — inferred: a POSIX sh script that classifies a list of changed paths, with its test cases for both sides; `scripts/check-ess-pin.sh` sets the pattern
- **Also likely:** `Taskfile.yml` — inferred: a `check:` task that runs those test cases inside `task check`, like `check:the_ess_pin_is_one_release` (line 34)
- **Read only:** `.github/workflows/shared-gates.yml` — inferred: the `Security and privacy` check comes from the pinned reusable workflow `beyond10x/gates/.github/workflows/common.yml@95d64e3c` (line 18); this repository cannot make it skip
- **Documents:** `AGENTS.md` § documentation workflows (line 153 describes what `pages.yml` runs) — inferred
- **Confidence:** medium. The workflow files and job names are cited; where the classifying script and its tests live is inferred.
- **Would collide with:** any unit that edits the `check` (`Gate`) or `macos` job of `ci.yml`, the `build` job of `pages.yml`, or the `check` task list in `Taskfile.yml`
- **Safety:** every required job still starts on every pull request and only its steps are skipped, so no required check stays pending; `ci.yml:3-6` and `pages.yml:3-7` have no `paths:` filter today, and a workflow-level `paths:` filter would leave a required check pending — inferred

## Not covered

- `Security and privacy` comes from the reusable workflow of the `beyond10x/gates` repository
  (`.github/workflows/shared-gates.yml`); this story leaves it as it runs today.

## Shared files

- `.github/workflows/ci.yml` (job `Gate`) and `Taskfile.yml` (task `check`): shared with
  `story:test-step-within-budget` and `story:wasm-lanes-within-budget`, built one after another on
  one branch in that order (`depends_on` edges on the two).
