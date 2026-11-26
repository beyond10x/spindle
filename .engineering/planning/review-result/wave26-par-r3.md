---
format: aep.planning-md/3
id: review-result:wave26-par-r3
kind: review-result
status: active
title: Plan critic (parallel-safety), round 3, on wave 26
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

Round 1 and round 2 findings of my lane are all resolved. Two new unnamed collisions are open, both warnings and both resting on inferred surfaces.

- story:view-filter-parameter-literal — its Documents omit `website/docs/concepts/specification.md`, which is the one page that lists the view-filter leaves (`:139-155`, "A filter is read where each leaf does one of these"), and U2 adds leaves, so this story must edit that list. `story:related-selector-rows-quantifier` (U1) also edits `specification.md:65-68`, and neither body nor the wave page names the shared file. Inferred for U2's edit, from AGENTS.md:148 "A change to a command, a capability rule or a crate changes the site … in the same commit". The two edits are on separate hunks. Remedies: record the shared file with an ordering edge, or move one story's page text elsewhere. — `.engineering/planning/story/view-filter-parameter-literal.md:71`, `.engineering/planning/story/related-selector-rows-quantifier.md:52`
- story:view-filter-parameter-literal — its Documents and Shared files do not mention `crates/spindle-docs/src/status.rs` or the generated `website/docs/status.md` and `website/data/status.json`. The previous view-filter story added a status row (commit 7564bfefa, 7 lines in `status.rs`), and AGENTS.md:145-148 requires it. U3 and U4 edit `status.rs` rows and the generated pages, and the wave page lists `status.rs` for U3 and U4 only, so U2 (and possibly U1) would collide there unnamed. Inferred, from that precedent. Remedies: add U2 to the shared `status.rs` entry on the wave page and in its own Shared files, or state that the story adds no status row. — `.engineering/planning/story/view-filter-parameter-literal.md:114`, `.engineering/waves/wave-26.md:41`

**What I read:**
- 8 stories in full (`aep plan artifact show`), the wave page, and `review-result:wave26-par-r1` and `-r2`.
- `git grep` and `sed` on the tree for `status.rs`, `specification.md`, the `HostTarget::new` callers and the undecidable-message tests.
- A scan of the story bodies for personal, company, host or home-path details: none found.

**Surfaces and reads:**
- Surfaces: 8 cited, 0 inferred, 0 unplaced.
- Acceptance reads traced to a producer in the set: 3. Two are recorded by a `depends_on` edge: clock-and-mint-in-every-mode to run-fails-on-rejected-line, and scenario-without-components to scenario-act-without-outcome.
- The third is the U3 `--scenario` test reading U4's printed line. It is handled in prose, since the test asserts only the exit code and the `holds` substring.

**What I could not establish:**
- Whether U1 adds a `status.rs` row. The U2 finding rests on precedent only.
- Out of my lane, not counted in the verdict:
  - `story:guard-over-absent-row-field` says spindle-cli is not touched. `crates/spindle-cli/tests/spindle-cli/absent_optional_guard.rs:361` asserts the exact old refusal text "…is undecidable over the held instance and the input", so the message change may break a test in a crate the story does not list.
  - `story:view-filter-parameter-literal` still cites a "gap file" that is not in the repository (`:69`).
  - The release story names "the release-gates repository" (`:114` of its body).

```findings
- file: .engineering/planning/story/view-filter-parameter-literal.md
  line: 71
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Inferred surface: its Documents omit website/docs/concepts/specification.md (the view-filter leaf list at :139-155, which this story must extend), and story:related-selector-rows-quantifier (U1) edits the same file at :65-68; neither body nor the wave page names the shared file; remedies: record the shared file with an ordering edge, or move one story's page text elsewhere"
- file: .engineering/planning/story/view-filter-parameter-literal.md
  line: 114
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Inferred surface (precedent commit 7564bfefa and AGENTS.md:145-148): the story adds a status row but its Documents and Shared files omit crates/spindle-docs/src/status.rs and the generated website/docs/status.md and status.json, which U3 and U4 also edit and the wave page lists for U3 and U4 only; remedies: name U2 in the shared status.rs entry and its own Shared files, or state that no status row is added"
```
