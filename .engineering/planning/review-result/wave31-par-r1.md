---
format: aep.planning-md/3
id: review-result:wave31-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 31
relations:
- reviews: story:postgres-lane-kills-slow-tests
- reviews: story:spindle-image-published
- reviews: story:listener-tls
- reviews: story:verified-caller-token
- reviews: story:spec-change-over-stored-rows
- reviews: story:several-writers-one-postgres
- reviews: story:event-egress-webhook
- reviews: story:dead-letters-kept-and-replayed
revision: 1
---
needs-revision

story:several-writers-one-postgres — its acceptance `a_standby_posts_no_webhook_and_keeps_no_dead_letter` reads the `[egress]` binding that story:event-egress-webhook creates and the dead letters that story:dead-letters-kept-and-replayed keeps. Its only depends_on edge goes to story:spec-change-over-stored-rows, so the store has no edge to either producer. Add both edges, or move the line to the producers. — .engineering/planning/story/several-writers-one-postgres.md:106, .engineering/planning/story/event-egress-webhook.md:64, .engineering/planning/story/dead-letters-kept-and-replayed.md:77, `aep plan artifact graph`

story:several-writers-one-postgres — its D3 A needs two `listener.rs` changes that its Scope and the wave page leave out, while the page says each unit has its own hunk. Both were found by reading the code (inferred): (1) the standby's "every other request 503" goes in `answer` next to the `stopping` check at `:609`, beside story:verified-caller-token's hunk at `:597-606`. (2) Binding before the host exists means changing `bind_reading_on` and `serve` at `:366-400`, which require a `Reader`; story:listener-tls adds its constructor there. The Scope and the page list only "a standby flag beside `stopping` (`:154`, `:174`, `:494`)". — .engineering/planning/story/several-writers-one-postgres.md:175, .engineering/waves/wave-31.md:55, .engineering/waves/wave-31.md:78-80, crates/spindle-ingress/src/listener.rs:609

story:several-writers-one-postgres — its lease with a fencing token makes the `planned` status row "ownership leases with fencing ... no story yet" stale. Changing that row means editing `status.rs` and regenerating `website/data/status.json` and `website/docs/status.md`. story:verified-caller-token changes the same three files, and neither this Scope nor the wave Surfaces list them. — crates/spindle-docs/src/status.rs:533-538, .engineering/planning/story/several-writers-one-postgres.md:147

story:listener-tls — it ships a new capability. The repository rule (AGENTS.md:149) and every past feature commit (e.g. `git show --stat 1f06b18d`) add a `status.rs` row and regenerate `status.json` and `status.md`, including its "N shipped, M planned" line. story:verified-caller-token changes those files too, and this Scope does not list them (inferred). — .engineering/planning/story/listener-tls.md:100, AGENTS.md:149

story:spec-change-over-stored-rows — the same: a new capability rule means a `status.rs` row and regenerated `status.json` and `status.md`, shared with story:verified-caller-token. Its Scope omits them (inferred). — .engineering/planning/story/spec-change-over-stored-rows.md:138, AGENTS.md:149

story:event-egress-webhook — the same: a new capability means a `status.rs` row and regenerated `status.json` and `status.md`, shared with story:verified-caller-token. Its Scope omits them (inferred). — .engineering/planning/story/event-egress-webhook.md:113, AGENTS.md:149

story:dead-letters-kept-and-replayed — the same: a new capability means a `status.rs` row and regenerated `status.json` and `status.md`, shared with story:verified-caller-token. Its Scope omits them (inferred). — .engineering/planning/story/dead-letters-kept-and-replayed.md:126, AGENTS.md:149

**What I read.** All 8 stories (`aep plan artifact show`), the wave-31 and wave-30 pages, and the scopes of story:served-clock-steps, story:events-cursor-across-restart and story:rally-example-reads-stdin. Commands: `aep plan artifact graph`, `aep plan artifact waves`, `aep plan artifact relations`, then reads of `Taskfile.yml`, `listener.rs`, `serve.rs`, `status.rs`, `AGENTS.md` and `git show --stat` on 4 feature commits.
- Surfaces: all 8 stories have at least one cited surface, none rests only on inferred surfaces, and none is unplaceable.
- Acceptance reads: I traced 2 to a producer in the set (U6 to U7, U6 to U8). No depends_on edge records either.

**What I could not establish:**
- Wave-30 collisions: wave 30 U12 (story:served-clock-steps) changes `Console` and `Console::ALL` in `console.rs` and `answer` in `router.rs`, where U8 adds its two routes. The wave page names U12's change to those files (`wave-31.md:108-110`) but not U8's hunks there. Wave 31 forks after wave 30 merges, so nothing collides while both run; only the line numbers move. Not a finding.
- `host.rs` and the kernel's `lib.rs`: wave 30 U1, U5, U6 and U7 also change these. The wave page names only U9 and U12; story:spec-change-over-stored-rows names the rest. The hunks are apart from wave 31's, and the order is the same.
- `AGENTS.md`: the same rule asks U3 to U8 to change it, and only U2 lists it. Which sections they would touch could not be established.
- PostgreSQL lane: U5 to U8 have their 60 s kill there only once U1 lands. Their acceptance lines do not read that, and the merge order puts U1 first. The new PostgreSQL tests all match the `postgres::` filter, so no Taskfile hunk of theirs meets U1's.
- Public repository: no person, host or home path in the 8 bodies or the wave page. `ghcr.io/beyond10x/spindle` is the project's own image and `gcr.io/distroless` a public base image.
- Outside my lane: whether 120 s is enough once U6's SIGSTOP and failover process tests sit in the PostgreSQL lane (test budget).

```findings
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 106
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance a_standby_posts_no_webhook_and_keeps_no_dead_letter reads the [egress] binding story:event-egress-webhook creates and the dead letters story:dead-letters-kept-and-replayed keeps, and no depends_on edge runs from it to either (only to story:spec-change-over-stored-rows); add both edges or move the line to the producers"
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 175
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: its D3 A puts the standby 503 in listener.rs answer beside the stopping check at :609, next to story:verified-caller-token's :597-606, and makes the Reader-taking bind path (bind_reading_on and serve, :366-400) work before the host exists, where story:listener-tls adds its constructor; its Scope and the wave page list only a standby flag beside stopping (:154, :174, :494) and claim own hunks"
- file: crates/spindle-docs/src/status.rs
  line: 533
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:several-writers-one-postgres's lease with fencing makes the planned status row 'ownership leases with fencing ... no story yet' stale, so it changes status.rs and the generated status.json and status.md, the same files story:verified-caller-token changes; neither its Scope nor the wave Surfaces list them"
- file: .engineering/planning/story/listener-tls.md
  line: 100
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: a new capability adds a status.rs row and regenerates status.json and status.md including the N shipped line (AGENTS.md:149, every feature commit), shared with story:verified-caller-token, and this Scope does not list them"
- file: .engineering/planning/story/spec-change-over-stored-rows.md
  line: 138
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: a new capability rule adds a status.rs row and regenerates status.json and status.md, shared with story:verified-caller-token, and this Scope does not list them"
- file: .engineering/planning/story/event-egress-webhook.md
  line: 113
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: a new capability adds a status.rs row and regenerates status.json and status.md, shared with story:verified-caller-token, and this Scope does not list them"
- file: .engineering/planning/story/dead-letters-kept-and-replayed.md
  line: 126
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: a new capability adds a status.rs row and regenerates status.json and status.md, shared with story:verified-caller-token, and this Scope does not list them"
```
