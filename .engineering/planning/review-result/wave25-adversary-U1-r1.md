---
format: aep.planning-md/3
id: review-result:wave25-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U1
relations:
- reviews: story:planning-only-prs-skip-tests
- reviews: story:test-step-within-budget
- reviews: story:wasm-lanes-within-budget
revision: 1
---
needs-revision

```
unit: story:planning-only-prs-skip-tests, story:test-step-within-budget, story:wasm-lanes-within-budget, head 1ff804b65
verdict: NEEDS-CHANGE
cases: executed 1361→1386, red 2
origin: introduced 4 / pre-existing 0 / undecided 0
```

Red on 1ff804b65 (`scripts/gate-scripts.adversary-test.sh`): a rename of `docs/design/spindle.md`
to `docs/archive/`, and a rename of `crates/a/src/lib.rs` into `.engineering/attic/`, are both
classified `planning`, so CI would skip every step.

Mutants the unit's own checks missed: dropping the guard that keeps `docs` over `planning`, and a
`within.sh` that drops a failing child's status.

Attacked and not broken: spaces, leading dashes, non-ASCII paths, an empty list, a non-merge HEAD,
other events; `docs/design/*.md` is the only Markdown outside `website/` a crate reads; every
required job still starts; `within.sh` on a signal, a missing command and bad bounds;
`.config/nextest.toml` without overrides; `opt-level = 1` for dependencies only; the broken-pipe
tolerance still asserts on the child's output.

Fixed by the coordinator (86975817b): `--no-renames`; the 25 cases run in
`check:changed_paths_are_classified`; `check:within_fails_over_its_bound` checks a status of 3; a
planning-only pull request runs `aep plan artifact validate`.

```findings
- file: scripts/changed-paths.sh
  line: 58
  category: boundary
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "renames are listed by their new path only, so a move of a test-read document or a source file into .engineering/ is classified planning"
- file: scripts/changed-paths.sh
  line: 47
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "the self-test never lists a planning path after a docs path"
- file: Taskfile.yml
  line: 46
  category: mutant
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "check:within_fails_over_its_bound never checks that within.sh passes on a failing child's status"
- file: .github/workflows/ci.yml
  line: 130
  category: judgement
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: "a planning-only pull request skipped aep plan artifact validate"
```
