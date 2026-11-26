---
format: aep.planning-md/3
id: review-result:wave26-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 26
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
Now the stories and their edges.
needs-revision

- story:release-linux-glibc-floor-and-musl — the body of this story in a public repository names the company ("every tool of the organisation", Outcome), a host ("the binary installed on this host", Joined), a local worktree path and unmerged branch (`a local worktree path `an unmerged branch`, Scope), and the a private sibling repository with its script paths; the revision generalises these (e.g. "every Rust tool of the organisation", "a sibling repository's `scripts/package.sh`") and drops the worktree path and branch name — `.engineering/planning/story/release-linux-glibc-floor-and-musl.md:20,43,100,107,109,111`
- story:scenario-without-components — acceptance 3 compares the outcome a scenario act answers with the stdin outcome, and the printed outcome on each act's line is what `story:scenario-act-without-outcome` adds, so the reader depends on the producer and no `depends_on` edge says so; add `depends_on story:scenario-act-without-outcome` (it is the U4 order already, held only in prose) — `.engineering/planning/story/scenario-without-components.md:25`, `.engineering/planning/story/scenario-act-without-outcome.md:87`
- story:clock-and-mint-in-every-mode — an earlier `at` "rejects the line" and its Decided section ties that to `story:run-fails-on-rejected-line` making the run exit 1, so the rejection behaviour is that story's and no edge records it; add `depends_on story:run-fails-on-rejected-line` (it is the U3 order already, held only in prose) — `.engineering/planning/story/clock-and-mint-in-every-mode.md:33,67-68`

**What you read.** I read 9 artifacts: the 8 stories, in full, and the wave page. I also read `story:release-binary-portable-libc`, the archived story that the release story takes over. I ran `aep plan artifact show` on each story, plus `relations`, `graph` and `validate`. `validate` reported only old unassessed review-results, which are not findings. I walked 45 edges in the whole-store graph, including edges outside the set. The set has no `depends_on` edges, so there is no cycle and no chain. Each story only `serves vision:spindle`, and `story:view-filter-parameter-literal` is also `informed_by story:view-filter-text-contains`. I traced 5 acceptance reads that depend on a producer inside the set. Two of them are the findings above, and no edge records either. The other three are ordered inside a unit by prose only. Of those, the two U1 stories read no state from each other, and one U3 acceptance asks for a line to be rejected with no stated read of the other story's output.

**What I could not establish.**
- U1: `story:guard-over-absent-row-field` after `story:related-selector-rows-quantifier` shares `execute.rs`, `lib.rs` and the kernel `main.rs` and reads no state the first story produces. Whether to record that order with an edge or split the shared surface is the parallel-safety critic's call, not a defect of shape.
- The glibc and musl floor, the zigbuild and objdump checks, and whether `cargo auditable` wraps `cargo zigbuild` are claims about a workflow I did not run.
- Out of my lane, for the acceptance critic: `story:scenario-act-without-outcome` Outcome still says Spindle exits 1, while Decided says exit 0, and Decided overrides it. `story:clock-and-mint-in-every-mode` Outcome still promises periodic bindings firing, which Decided moved to Not covered. Acceptance 1 and 2 of `story:guard-over-absent-row-field` are expected to pass at the start, so the story proves nothing beyond its third line.
- Out of my lane, for the parallel-safety critic: U3 and U4 share `target.rs`, `cli.rs`, `cli.md` and `status.rs`, and U5 may share `main.rs` with U3.

```findings
- file: .engineering/planning/story/release-linux-glibc-floor-and-musl.md
  line: 20
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the body in a public repository names the company (line 20), a host (line 43), a local worktree path and unmerged branch (lines 100, 107) and the a private sibling repositorywith its script paths (lines 109, 111); the revision generalises them"
- file: .engineering/planning/story/scenario-without-components.md
  line: 25
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "acceptance 3 reads the outcome printed on a scenario act line, which story:scenario-act-without-outcome adds (scenario-act-without-outcome.md:87), and no depends_on edge runs from this story to it; add depends_on story:scenario-act-without-outcome"
- file: .engineering/planning/story/clock-and-mint-in-every-mode.md
  line: 33
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the rejected line for an earlier at (lines 33, 67-68) takes its meaning and exit status from story:run-fails-on-rejected-line, and no depends_on edge records it; add depends_on story:run-fails-on-rejected-line"
```
