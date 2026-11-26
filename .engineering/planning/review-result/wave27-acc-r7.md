---
format: aep.planning-md/3
id: review-result:wave27-acc-r7
kind: review-result
status: active
title: Plan critic (acceptance), round 7, on wave 27
relations:
- reviews: story:stdin-run-fires-bindings
- reviews: story:stdin-arrange-and-line-references
- reviews: story:seed-once-on-durable-state
- reviews: story:serve-loop-signal-exit-answers
- reviews: story:query-input-beside-body
- reviews: story:caller-attributes-on-loopback
- reviews: story:map-input-guard-facts
- reviews: story:integer-identity-any-spelling
- reviews: story:wasm-lanes-within-budget
- reviews: story:refused-bound-command-recorded
- reviews: story:cross-component-related-reads
revision: 1
---
needs-revision

- story:cross-component-related-reads — D1 A has the ingress `Surface` take the reached set so a served run serves the route, but no Acceptance bullet drives a served HTTP run (`run` with a network component) of a cross-component command, so the host tests and `admit` can pass while `surface::refuse` (`crates/spindle-ingress/src/surface.rs:1688`) still answers 501 "not served" — `.engineering/planning/story/cross-component-related-reads.md:77` (Acceptance: bullet 2 says only "served in one host"; D1 A at `:207-216`)
- story:seed-once-on-durable-state — the decided "writes `seed.json` at count 0 before it sends its first entry, so a run stopped before that entry completes restarts the seed from its first entry" has no bullet that fails when the marker is written only after the entry. The serve-loop bullet `a_reset_cut_at_its_first_entry_resumes_from_the_second` uses SIGTERM, which lets the entry finish, so the marker reads count 1 either way. Only a kill during the held first entry (SIGKILL) tells the two apart — `.engineering/planning/story/seed-once-on-durable-state.md:123` (against Acceptance `:52`; cited: `serve-loop-signal-exit-answers.md:66`)
- story:refused-bound-command-recorded — D1 A decides `caused_by` is empty for an envelope from another process, and no Acceptance bullet or test checks it. All five bullets name the causing command, so an implementation that fills it wrongly or fails there passes them — `.engineering/planning/story/refused-bound-command-recorded.md:128-129` (against Acceptance `:74`)
- story:refused-bound-command-recorded — D3 A decides the request log stays `spindle-requests/1`, and no Acceptance bullet reads the record's version string, so a version bump passes all five bullets — `.engineering/planning/story/refused-bound-command-recorded.md:133` (against Acceptance `:74`)

Earlier findings in my lane: the round 6 finding is resolved. `seed-once-on-durable-state` now has `a_run_stopped_mid_seed_resumes_after_the_last_entry_done` (Acceptance, last bullet). Rounds 1 to 5 stay resolved. I found nothing to change in the other eight stories. The four findings above are decided behaviours not yet checked in this pass.

What I read: 11 of 11 stories in full with `aep plan artifact show` (saved under `w27/acc7-*`), the wave page, and `review-result:wave27-acc-r1` to `-r6`. Run in `spindle-plan-27` at head `84668d689`. I checked in the tree that `surface::refuse` calls `spindle_kernel::takes` (`surface.rs:1688`) and that `takes_reaching` and any reached-set names do not exist yet. Story bodies hold no person, company, host or home path. "The build node", "the operator" and "the host operator" are generic roles.

Could not establish:
- Whether the six wasm lanes other than the 626 s Node lane and the 120.9 s service-worker lane already end under 120 s today. Each bullet names the bound, so I did not raise it.
- Whether the "Decided when scoped" items that only add documentation, such as the changelog line for no migration of `1.0` rows in `integer-identity-any-spelling` and the `docs/design/spindle.md` § 11 rows, need their own bullets. I read them as Scope deliverables and not findings.
- I did not run `aep plan artifact kinds` or `lifecycle` this pass.

Out of my lane: the Scope sections of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references` and `query-input-beside-body` count tests differently from their Acceptance sections (scope critic, noted since round 2).

```findings
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 77
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A has the ingress Surface take the reached set so a served run serves a cross-component route, but no Acceptance bullet drives a served HTTP run of such a command (bullet 2 says only 'served in one host'), so the host tests and admit can pass while surface::refuse still answers 501 'not served'"
- file: .engineering/planning/story/seed-once-on-durable-state.md
  line: 123
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the decided 'writes seed.json at count 0 before it sends its first entry, so a run stopped before that entry completes restarts the seed from its first entry' has no bullet that fails when the marker is written only after the entry; the serve-loop reset-cut bullet uses SIGTERM, which lets the entry finish, so only a kill during the held first entry tells the two apart"
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 128
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A decides caused_by is empty for an envelope from another process, and no Acceptance bullet or test checks it, so a wrong or failing value there passes all five bullets"
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 133
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D3 A decides the request log stays spindle-requests/1, and no Acceptance bullet reads the record's version string, so a version bump passes all five bullets"
```
