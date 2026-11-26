---
format: aep.planning-md/3
id: review-result:wave26-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 26
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

- story:clock-and-mint-in-every-mode — its `Shared files` omit `crates/spindle-cli/src/replay.rs` (cited: `:173-205`, opens the host through `HostTarget::new` at `:187`) and `crates/spindle-cli/tests/spindle-cli/replay.rs` (inferred, `Also likely`). U4 also edits both (`replay.rs:44-89`, `:100-153`, `:173-206` plus its replay tests). The body also says `HostTarget::new` takes a clock and a mint but only calls that "separate functions". A signature change breaks every caller: `replay.rs:82`, `replay.rs:187`, `conform.rs:122` and about 30 call sites across `crates/spindle-conformance/tests`. Remedies: an ordering edge between U3 and U4 that records this, or keep `HostTarget::new` unchanged and add a separate constructor so the two share no hunk. — .engineering/planning/story/clock-and-mint-in-every-mode.md `## Scope` (Files `replay.rs:173-205`) and `## Shared files`; `git grep -n "HostTarget::new" crates`
- story:scenario-act-without-outcome — `Shared files` name `target.rs` as "separate functions" and `cli.rs` and `status.rs` hunks with U3, but not `replay.rs` or its tests (cited: `replay.rs:100-153`, `Replays::run` `:173-206`). U3 edits `Replays::run` and the `HostTarget::new` call at `:187`. The new `HostTarget` outcome record (a struct field beside the command count) sits in the same constructor hunk U3 changes. `story:scenario-without-components` has the same gap on `replay.rs:82`, which sits inside its own `:44-89`. Remedies: an ordering edge between U3 and U4, or split the surface so they share neither `replay.rs` nor the `HostTarget` struct and `new`. — .engineering/planning/story/scenario-act-without-outcome.md `## Shared files`, `## Scope` (Primary surface, Also likely `target.rs`)
- story:view-filter-parameter-literal — its `Shared files` call `host.rs:1625-1645` "this story's alone", but U3's `story:clock-and-mint-in-every-mode` edits `crates/spindle-host/src/host.rs` (`:367-371`, `:984`, `:1195-1236`, cited). The hunks differ, so this is a same-file collision, not a same-line one. Neither body names the other, and the wave page's `## Surfaces` does not list `host.rs`. Both also add test modules to `crates/spindle-host/tests/spindle-host/main.rs` only if U3 tests the reset in that crate (inferred). Remedies: record the shared file with `host.rs` as the reason, or have U3's run seed live outside `host.rs`. — .engineering/planning/story/view-filter-parameter-literal.md `## Shared files`, and `## Scope` (Also likely `host.rs:1625-1645`)
- story:clock-and-mint-in-every-mode — its second acceptance line ("a later line with an earlier `at` is rejected") reads the rejection and exit-1 behaviour that `story:run-fails-on-rejected-line` creates. Its `Decided when scoped` says so, but no `depends_on` edge runs from this story to that one; the order is in prose only. The fix is to add the edge (or merge the two). — .engineering/planning/story/clock-and-mint-in-every-mode.md `## Acceptance` (second bullet), `## Decided when scoped` (second bullet); `aep plan artifact graph` shows no edge between the two
- story:release-linux-glibc-floor-and-musl — the story is for a public repository and holds a local worktree path fragment (`a local worktree path a local branch name (`an unmerged branch`) and sibling-repository commit hashes. It also says "the binary installed on this host". These identify the local machine and private repositories. Replace them with a generic description. — .engineering/planning/story/release-linux-glibc-floor-and-musl.md `## Scope` (Pattern to copy, ESS bullet and the composite-action sub-bullet), `## Joined 2026-11-26`

What I read: 8 stories (`aep plan artifact show` for each), the wave page, `aep plan artifact graph`, `relations` and `waves`, and the tree with `git grep` and `sed`. Of the 8, 8 are placed by cited surfaces, 0 by inferred surfaces and 0 are unplaced. I traced 1 acceptance read to a producer in the set, and 0 of those have a `depends_on` edge. The only graph edges among the set are `serves vision:spindle`, plus one `informed_by` on the view-filter story.

What I could not establish:
- Whether U3's tests land in the spindle-host test `main.rs`; the story names only spindle-cli tests, and I treated that as unknown.
- Whether U4 changes the fixtures under `fixtures/replay` that U3's `observe.rs` and `conform.rs` read. `observe.rs` is on U3's list of stdin tests that assert exit 0; U4 edits the replay spec fixtures. I could not tell if that matters, so I left it out of the findings.
- Out of my lane: the `guard-over-absent-row-field` Scope has a garbled sentence ("source (`d7833ee`)crates/"). Its first two acceptance lines are expected to pass at the start, which is the acceptance critic's question. The consumer field names (`claimed_by`, `phone_number_id`) in that story may identify the consumer; the brief asks me to flag that and I did not judge it a finding.

```findings
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 11
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "this and U4's story:scenario-act-without-outcome and story:scenario-without-components both land on crates/spindle-cli/src/replay.rs and its tests (cited, both bodies), and this story's HostTarget::new signature change breaks every caller (replay.rs:82, :187, conform.rs:122, about 30 conformance test call sites); neither body names the collision; remedies: an ordering edge recording the shared files, or a separate constructor so the two share no hunk"
- file: .engineering/planning/story/scenario-act-without-outcome.md
  line: 66
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "Shared files omit replay.rs (Replays::run :173-206, tests) and the HostTarget struct and constructor hunk that U3's story:clock-and-mint-in-every-mode also changes (cited, both bodies), calling target.rs only 'separate functions'; story:scenario-without-components has the same gap on replay.rs:82; remedies: an ordering edge between U3 and U4, or split the surface so they share neither"
- file: .engineering/planning/story/view-filter-parameter-literal.md
  line: 109
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "claims host.rs:1625-1645 as this story's alone, but U3's story:clock-and-mint-in-every-mode edits crates/spindle-host/src/host.rs (:367-371, :984, :1195-1236; cited, both bodies); different hunks of one file, named in neither body nor the wave page; remedies: record the shared file with an ordering edge, or move U3's run seed out of host.rs"
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 24
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance that an earlier 'at' is rejected (exit 1 per Decided when scoped) reads behaviour story:run-fails-on-rejected-line creates, and no depends_on edge runs from this story to that one; add the edge or merge the two"
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 104
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "public-repository story body holds a local worktree path fragment (a local worktree path a local branch name (an unmerged branch), sibling-repository commit hashes and 'the binary installed on this host'; replace with a generic description"
```
