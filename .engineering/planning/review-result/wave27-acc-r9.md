---
format: aep.planning-md/3
id: review-result:wave27-acc-r9
kind: review-result
status: active
title: Plan critic (acceptance), round 9, on wave 27
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

- story:serve-loop-signal-exit-answers — Bullet 2 "the same with a reset refused 400: the client reads the 400" names no sequence that puts the refusal before the signal. "The same" cannot mean a held first entry, because a refused reset (`console::reset` answers 400, `crates/spindle-cli/src/serve.rs:494-516`) reaches no seed entry. A SIGTERM sent before the loop takes the reset makes it a queued request, which bullet 3 and D1 A answer 503, so a run can fail bullet 2 on a race. Name how the refused reset is taken before SIGTERM, or move this check to the deterministic `spindle-ingress` close test — `.engineering/planning/story/serve-loop-signal-exit-answers.md:56` (against D1 A at `:79-81`, bullet 3 at `:58`)
- story:refused-bound-command-recorded — The decided "Place, A … the event log stays what ESS commits" (B, an entry in `GET /api/events`, is rejected) has no Acceptance bullet. An implementation that also writes the refusal into the event log passes all eight bullets. Add to bullet 1 or 2 that `GET /api/events` lists nothing for the refused bound command — `.engineering/planning/story/refused-bound-command-recorded.md:71` and `:132-133` (against Acceptance `:76-98`)

Earlier findings: both round 8 findings are resolved.
- `serve-loop-signal-exit-answers` bullet 1 now names the sequence (start-up seed done, reset taken, its own first entry held, then SIGTERM). Bullet 3 says a queued reset is not taken and the state is not emptied.
- `refused-bound-command-recorded` splits bullet 6 into a host test (`a_refusal_from_another_process_has_no_cause`) and an ingress test (`a_refusal_without_a_cause_has_no_cause_member`).
- Rounds 1 to 7 stay resolved. I re-read the findings of `-r1` to `-r7` and checked each fix against the current bodies.

This pass, I mapped every decided option and every "Decided when scoped" item of all 11 stories to an Acceptance bullet. The two findings above are the only decided behaviours without a checkable bullet.

What I read: 11 of 11 stories in full with `aep plan artifact show`, saved under `<local path>`. I also read the wave page, the brief, the rubric and role files, `review-result:wave27-acc-r1` to `-r8`, and the serve loop at `crates/spindle-cli/src/serve.rs:415-516` and `crates/spindle-cli/src/component.rs` at head `701f3034b`. I confirmed `play --component` exists in `cli.rs`. A grep of the story bodies for person, company, host, home path and URL found nothing; "the build node", "the host operator" and "a consumer" are roles.

Could not establish:
- Whether each wasm lane ends under 120 s today. Each bullet names the bound and the measurement.
- Whether foreign `Scanned` passed through unchanged fails as "moved" (`cross-component-related-reads`, `Safety fact (to prove)`). I read it as the implementor's proof.
- The documentation deliverables, the D2 `alloc` feature of `wasm-lanes-within-budget`, D1 A of `integer-identity-any-spelling` (the world walk moving to the kernel, with the wave 25 world tests as the guard) and D2 A of `seed-once-on-durable-state` (SIGKILL applies one entry twice, which is not observable on purpose). I read these as Scope.
- I did not run `aep plan artifact kinds` or `lifecycle` this pass.

Out of my lane: the Scope test lists of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references`, `query-input-beside-body` and `refused-bound-command-recorded` differ from their Acceptance names (scope critic).

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 56
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "bullet 2 'the same with a reset refused 400: the client reads the 400' names no sequence that puts the refusal before the signal; a refused reset reaches no seed entry, so 'the same' gives no ordering, and a SIGTERM that wins the race makes the reset a queued request answered 503 under bullet 3 and D1 A; name how the refused reset is taken before SIGTERM or move the check to the ingress close test"
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 71
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the decided Place A 'the event log stays the events ESS commits' (option B rejected) has no Acceptance bullet, so an implementation that also writes the refusal into GET /api/events passes all eight bullets; state that the event log lists nothing for the refused bound command"
```
