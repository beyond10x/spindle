---
format: aep.planning-md/3
id: review-result:wave26-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 26
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
- reviews: story:view-filter-parameter-literal
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
- reviews: story:release-linux-glibc-floor-and-musl
revision: 1
---
needs-revision

- story:clock-and-mint-in-every-mode — its acceptance `a_scenario_runs_on_the_clock_flag` asserts that the act "prints `holds`" in `crates/spindle-cli/tests/spindle-cli/replay.rs`, but U4's `story:scenario-act-without-outcome` changes that printed line to carry the observed outcome. U3 merges first, so U4's implementor never sees U3's test, and a test that compares the whole line breaks at merge. Neither body says so (cited, both bodies). Remedies: the body states that the test asserts only the exit code and a `holds` substring, or the coordinator's merge step names this line as shared between U3 and U4 — `.engineering/planning/story/clock-and-mint-in-every-mode.md` `## Acceptance` (`a_scenario_runs_on_the_clock_flag`), `scenario-act-without-outcome.md` `## Outcome` (line "prints, on each timeline act's line, the outcome observed")
- story:release-linux-glibc-floor-and-musl — its `## Shared files` says `README.md` § Releases is "this story's alone in wave 26". But `story:run-fails-on-rejected-line` cites `README.md:56-64` under Documents, and `story:clock-and-mint-in-every-mode` lists `README.md` (run flags). U3 and U5 are separate units, so the file is shared (cited). The hunks differ: the seed paragraph at `README.md:56-64`, "Releases" at `:214-219`. Remedy: record `README.md` as shared with U3 on separate hunks, as the other shared files are — `.engineering/planning/story/release-linux-glibc-floor-and-musl.md` `## Shared files`, `.engineering/planning/story/run-fails-on-rejected-line.md` `## Scope` (Documents)
- `.engineering/waves/wave-26.md` — `## Surfaces` lists the files the coordinator resolves at merge but omits `crates/spindle-cli/src/replay.rs`, `crates/spindle-conformance/src/target.rs` and `crates/spindle-host/src/host.rs`, and the `README.md` above. The stories now name all of them, so the omission is only on the page. This is the part of round-1 finding 3 that is not fixed (cited) — `.engineering/waves/wave-26.md` `## Surfaces`

Round 1 status for my lane:
- **Resolved:** the `replay.rs` and `HostTarget` collision (both U4 bodies and the clock story now name it, `HostTarget::new` keeps its signature, and the separate constructor is stated).
- **Resolved:** the `host.rs` collision between U2 and U3. The hunks cited in both bodies match the tree (`Opening` at `:367`, `mint` at `:984`, doc list at `:1625-1645`).
- **Resolved:** the missing `depends_on` from `story:clock-and-mint-in-every-mode` to `story:run-fails-on-rejected-line`.
- **Resolved:** the leak in the release story (worktree path, branch, host and sibling commit hashes are gone).

What I read: all 8 stories in full with `aep plan artifact show`, the wave page, and the four round-1 review-results. I checked the cited hunks in `host.rs`, `target.rs`, `README.md` and the U2 test files by `git grep` and `sed`. Surfaces: 8 cited, 0 inferred, 0 unplaced. I traced 3 acceptance reads to a producer in the set, and 2 of them are recorded by a `depends_on` edge. The unrecorded one is the U3 scenario test reading U4's line format, in the first finding.

What I could not establish:
- Whether U3's "other CLI tests whose stdin lines fail and assert exit 0" includes a file U2 edits. This list is inferred in the story. `adversary_unserved.rs` and `view_field_filters.rs` hold no `failure` match that asserts a success exit, so I found no overlap.
- Whether the same-file merges the stories leave to the coordinator (`replay.rs`, `target.rs`, `host.rs`, the generated `cli.md` and `status.rs` pages, `CHANGELOG.md`) are acceptable across five parallel units. They are now named in the bodies, so the call is the operator's.
- Out of my lane, not counted in the verdict:
  - The consumer's field names (`<field>`, `<field>`) in `story:guard-over-absent-row-field`, and "a build node runs glibc 2.34" and "the release-gates repository" in the release story.
  - `story:view-filter-parameter-literal` still cites a "gap file" that is not in the repository.
  - The Outcome of `story:scenario-act-without-outcome` is now consistent with its Decided section. The Outcome of `story:clock-and-mint-in-every-mode` no longer promises periodic bindings.

```findings
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 40
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance a_scenario_runs_on_the_clock_flag asserts the act 'prints holds' in tests/spindle-cli/replay.rs, a line story:scenario-act-without-outcome (U4) changes to carry the observed outcome; U3 merges first and U4's implementor never sees the test, and neither body says so (cited, both bodies); remedies: assert only the exit code and a holds substring, or name the printed line as shared in both Shared files"
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 163
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Shared files calls README.md section Releases this story's alone, but story:run-fails-on-rejected-line cites README.md:56-64 and story:clock-and-mint-in-every-mode lists README.md (U3, cited); the file is shared on separate hunks and the body does not say so; remedies: record README.md as shared with U3, or move the U3 documentation change out of README.md"
- file: .engineering/waves/wave-26.md
  line: 38
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Surfaces section lists the files the coordinator resolves at merge but omits crates/spindle-cli/src/replay.rs, crates/spindle-conformance/src/target.rs, crates/spindle-host/src/host.rs and README.md, which the stories now name as shared between units; round-1 finding 3 asked for the wave page to list host.rs and this part is still open (cited)"
```
