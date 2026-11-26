---
format: aep.planning-md/3
id: review-result:wave27-acc-r8
kind: review-result
status: active
title: Plan critic (acceptance), round 8, on wave 27
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

- story:serve-loop-signal-exit-answers — Acceptance bullet 1 does not say whose seed entry is held, and its two readings contradict bullet 3 and D1 A. Bullet 1 reads "a seeded reset taken while a seed entry is held, then SIGTERM: the client reads 200 once the entry under way is done, … a restart … holds the rows the seed applied before the signal". Its name is `a_reset_taken_after_a_signal_is_answered_200`. The bug in the Outcome is a reset queued behind the held start-up seed entry and taken by `listener.next` after the signal. Bullet 3 and D1 A answer a request still queued at the signal 503. A reset that was taken before the signal and whose own first entry is held is bullet 5's scenario, answered 200 under D2 A. Bullet 1 can mean either, so a run can pass it or bullet 3 with opposite answers for the same queued reset. Name the sequence in bullet 1 (the reset exchange was taken before SIGTERM and its own seed entry is held). For the other sequence, bullet 3 should say a queued reset is not run and the durable state is not emptied — `.engineering/planning/story/serve-loop-signal-exit-answers.md:37-47` (against D1 A and D2 A at `:77-81`; loop at `crates/spindle-cli/src/serve.rs:428-450`)
- story:refused-bound-command-recorded — Acceptance bullet 6 has no test that can check its second half. It names `crates/spindle-host`, where `RefusedCommand.caused_by` can be read, and in the same sentence asserts "the request record's entry has no `caused_by` member". The request record exists only in `spindle-ingress`. Scope lists a different host test for D1, `a_delivered_refusal_names_the_command_that_published_its_event`, and no test for this bullet. State where the record half is read, either an ingress test or a split bullet, so that one test can fail it — `.engineering/planning/story/refused-bound-command-recorded.md:63-65` (against Scope `:92`)

Earlier findings: I checked that every finding of rounds 1 to 7 in my lane is recorded as fixed on its stories. The four round 7 findings are resolved. `cross-component-related-reads` has `a_served_run_answers_a_cross_component_guard`. `seed-once-on-durable-state` has `a_kill_during_a_resets_first_entry_restarts_the_seed`, with its serve-loop counterpart. `refused-bound-command-recorded` has `a_refusal_from_another_process_has_no_cause` and `the_request_log_stays_version_1`. I found nothing to change in the other nine stories, whose decided options each map to a bullet. I checked every decided option and every "Decided when scoped" item of each story against its Acceptance in this pass. The only unchecked items are documentation deliverables, such as the changelog lines, the § 11 rows and the no-migration note, and the `alloc` feature of D2 on `wasm-lanes-within-budget`, which the lane times observe. I read them as Scope.

What I read: all 11 stories in full with `aep plan artifact show`, saved under `w27/acc8/`. I also read the wave page, `review-result:wave27-acc-r7` in full and the outcomes of `-r1` to `-r6`, and the serve loop at `crates/spindle-cli/src/serve.rs:415-470` at head `fb142c367`. Story bodies hold no person, company, host or home path.

Could not establish:
- Whether each of the six wasm lanes ends under 120 s today. Each bullet names the bound and the measurement.
- Whether the `Safety fact (to prove)` of `cross-component-related-reads`, foreign `Scanned` passed through unchanged failing as "moved", is exercised. Bullet 3's "selects the outcome" does not say the row-set guard's outcome commits a write. I read this as the implementor's proof and did not raise it.
- I did not run `aep plan artifact kinds` or `lifecycle` this pass.

Out of my lane: the Scope test lists of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references`, `query-input-beside-body` and `refused-bound-command-recorded` differ from their Acceptance names (scope critic).

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 37
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "bullet 1 'a seeded reset taken while a seed entry is held, then SIGTERM: the client reads 200' does not say whose entry is held or whether the reset was taken before the signal; a reset queued behind the held start-up entry is 503 under bullet 3 and D1 A, so the same run can pass bullet 1 or bullet 3 with opposite answers; name the sequence, and have bullet 3 assert a queued reset is not run and the state is not emptied"
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 63
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "bullet 6 is placed in crates/spindle-host but also asserts 'the request record's entry has no caused_by member', which only spindle-ingress can read; Scope lists no test for it (only a_delivered_refusal_names_the_command_that_published_its_event), so no named test can fail the record half"
```
