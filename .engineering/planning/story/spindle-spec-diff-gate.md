---
format: aep.planning-md/3
id: story:spindle-spec-diff-gate
kind: story
status: draft
title: The gate classifies the specification's change since the last release
revision: 1
---
# The gate classifies the specification's change since the last release

## Outcome

`task check` and CI compare `ess/` with the specification of the last release tag through
`ess verify diff --compatibility --fail-on breaking-or-unknown`, and a breaking or unknown change
fails the gate until it is acknowledged in a committed `ess-diff-acknowledgements/1` document that
names each change id and both endpoint digests. Today nothing compares two revisions: a narrowing
can be released as if it were additive.

## Source

ESS hardening round 2026-11-23 (`docs/hardening/2026-11-23.md`, technique 7). `ess verify diff`
from `0.18.0` to `main` (`bbc50b00`): 0 changes, exit 0; the round's own change: 21 changes, 13
compatible, 8 unknown (`type/spindle.placement.Resources/invariants-changed`, two view fields and five
`outcome-error-payload-added`), exit 4. A planted removal of the `TargetKind` variant `Browser`
exited 4 naming `type/spindle.placement.TargetKind/variant-removed/Browser` (breaking for callers
and history).

## Acceptance

- `check:the_specification_change_since_the_last_release_is_acknowledged`: the task materialises
  `ess/` at the newest release tag, runs the diff with `--fail-on breaking-or-unknown` and the
  committed acknowledgements, and passes on `main`.
- A branch that removes one enum variant fails the task at exit 4, naming the change, and passes
  once the change is acknowledged.
- CI fetches the tags the task reads, so the local gate and the forge decide alike.

## Scope

- `Taskfile.yml`, `.github/workflows/ci.yml`, an acknowledgements file under `ess/`.
- `AGENTS.md` (Gate) and the release procedure: the acknowledgements are dropped when the next
  release is cut, since their endpoint digests no longer bind.
