---
format: aep.planning-md/3
id: story:planning-only-prs-skip-tests
kind: story
status: proposed
title: A pull request that changes only planning or prose skips the build and test jobs
relations:
- serves: vision:spindle
revision: 2
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 2}
---
# A pull request that changes only planning or prose skips the build and test jobs

## Outcome

A pull request whose changed files are all planning or prose files (`.engineering/`, `*.md`
outside `website/`) passes its required checks (`Gate`, `macOS`, `Build documentation`, the
shared `Security and privacy` check) without building or testing: each required job runs, finds
only such paths, and exits 0. A change under `website/` still runs the documentation check and
skips the code jobs. One path list, in the workflow. The ruleset is unchanged.

Today every pull request runs the full gate: PR 61 changed one file under `.engineering/` and its
`CI` run took 18 minutes (2026-11-22T15:25:58Z to 15:44:15Z).

## Acceptance

- A test case for each side of the path list: a change set of only `.engineering/` and
  `README.md` paths is classified planning-only; one that adds any other path, or a file under
  `website/`, is not.
- A planning-only pull request's `Gate` and `macOS` jobs end within 2 minutes with exit 0, and
  `Build documentation` skips the site build.
- A pull request with one changed `.rs` file runs every job as today.

## Origin

`~/beyond10x/AGENTS.md` § "Planning-only pull requests skip the tests" (Timo, 2026-11-21, "ROLL
the 5", proposal 3): built as a story ahead of the next feature.
