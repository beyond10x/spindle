---
format: aep.planning-md/3
id: review-result:panel-w15-drafts-r1
kind: review-result
status: active
title: Five drafts, plan-critic panel, round 1
relations:
- reviews: story:kernel-row-set-commit-conflict
- reviews: story:route-literal-segment-precedence
- reviews: story:kernel-creation-takes-unknown-row
- reviews: story:kernel-guard-reads-now
- reviews: story:links-second-hello-race
revision: 1
---
needs-revision

Plan-critic panel, round 1, on five draft stories: story:kernel-row-set-commit-conflict, story:route-literal-segment-precedence, story:kernel-creation-takes-unknown-row, story:kernel-guard-reads-now, story:links-second-hello-race. Four lanes, one agent each.

LANE: acceptance — needs-revision (all five)
- kernel-row-set-commit-conflict: the test asserted one active reservation only, which a surfaced error also satisfies; the single-row `when_related:` sentence had no test.
- route-literal-segment-precedence: the 405 test passed before the work (both routes on GET).
- kernel-creation-takes-unknown-row: no test for the `declared_not_found` fallback; steps unnamed.
- kernel-guard-reads-now: one guard covered; selection, invariant and the admission-refusal branch untested.
- links-second-hello-race: the 20-run check is no transition, and the second order contradicted `a_second_hello_on_an_open_link_is_refused_naming_the_peer`.

LANE: design — needs-revision (row-set, links); approve (route, creation, guard-now)
- kernel-row-set-commit-conflict conditioned the multi-row commit of story:kernel-secondary-effects and story:kernel-set-outcomes with no edge or coverage statement; its cross-tab claim belonged to story:browser-store-stale-write-conflict.
- links-second-hello-race: same contradiction with the open-link test.

LANE: parallel-safety — needs-revision (all five)
- No Order sections naming the shared `execute.rs`, `store.rs`, `surface.rs`; scopes missed `contract.rs`, the contract runners, `delete.rs`, a conditional `supported.rs`; links-second-hello-race had no Scope and was unassessed.

LANE: scope — needs-revision (row-set, route); approve (creation, guard-now, links)
- kernel-row-set-commit-conflict reached beyond its origin (single-row read, a re-decide loop); the route story's 405 test had no Outcome sentence.

```findings
- file: .engineering/planning/story/kernel-row-set-commit-conflict.md
  line: 37
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance did not tell a refused conflict from an error, and the row-set story's commit coverage, scope and origin reach needed revision"
- file: .engineering/planning/story/route-literal-segment-precedence.md
  line: 40
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the 405 acceptance passed before the work and had no Outcome sentence; surface.rs sharing with live-topology unstated"
- file: .engineering/planning/story/kernel-creation-takes-unknown-row.md
  line: 35
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "no test for the declared_not_found fallback, unnamed steps, delete.rs and execute.rs sharing unstated"
- file: .engineering/planning/story/kernel-guard-reads-now.md
  line: 31
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "untested constructs and an either/or admission branch; execute.rs sharing unstated"
- file: .engineering/planning/story/links-second-hello-race.md
  line: 35
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the 20-run check was no transition, the second order contradicted the open-link test, and no Scope"
```
