---
format: aep.planning-md/3
id: review-result:wave27-acc-r5
kind: review-result
status: active
title: Plan critic (acceptance), round 5, on wave 27
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

- story:refused-bound-command-recorded — the Outcome and D1 A cover two cases, a declared refusing outcome (`RefusedCommand`) and a kernel rejection (`Failed`, "the refusal's outcome or error code"). Every Acceptance bullet drives only a declared refusing outcome, so the `Failed` path can be left unrecorded and all four bullets still pass — `.engineering/planning/story/refused-bound-command-recorded.md:12` ("When a binding's command is refused (a declared refusing outcome, or a kernel rejection), a served run records it"), Acceptance bullet 1 (`a_refused_bound_command_is_listed_with_its_cause`).
- story:seed-once-on-durable-state — three decided behaviours have no Acceptance bullet. D1 A says a reset that loads a world marks every seed entry done, so a restart sends nothing. A state that holds rows and no marker gets the seed applied as today, then the marker. A changed digest on a state that holds no row runs the seed from its first entry. The three bullets cover only the same-digest restart, the changed-digest refusal and the reset with no world, so each of these can be wrong while all three pass — `.engineering/planning/story/seed-once-on-durable-state.md`, section "Decided when scoped" (the D1 A line and the "A state that holds rows and no marker" line) against the three bullets in "Acceptance".

**Earlier findings.** Both round 4 findings are resolved:
- `serve-loop-signal-exit-answers` now has an Acceptance bullet naming `adversary_w26_u3::the_answers_in_hand_are_written_before_a_rejecting_seed_stops_the_run`: its `ignore` is removed, it passes, the taken grant reads 2xx and the queued one reads 503.
- `stdin-run-fires-bindings` now has `a_specification_without_components_fires_its_bindings_on_stdin`, which covers D3 A.

Rounds 1 to 3 stay resolved. The two findings above are new this round; the other nine stories I found nothing to change.

**What I read.** All 11 stories in full with `aep plan artifact show`, saved under `<local path>`. I also read `review-result:wave27-acc-r1` to `-r4`, the diff of `09c992a7a`, and the rubric and role files. I did not run `aep plan artifact kinds` or `lifecycle`. I checked story bodies for person, company, host and home-path strings and found none; "the host operator" and "the build node" are generic roles.

**Could not establish.**
- Whether the six wasm lanes other than the 626 s Node lane and the 120.9 s service-worker lane already end under 120 s. The other bullets carry the transition, so I did not raise it.
- The wave-26 ignore on the `adversary_w26_u3` test is not in this tree until wave 26 is merged.

**Out of my lane.** The Scope sections of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references` and `query-input-beside-body` count tests differently from their Acceptance sections. That is for the scope critic, noted in round 2.

```findings
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 12
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome and D1 A cover a declared refusing outcome and a kernel rejection (Failed), but every Acceptance bullet drives only a declared refusing outcome, so a record that misses the kernel-rejection path still passes all four"
- file: .engineering/planning/story/seed-once-on-durable-state.md
  line: 111
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A (a reset that loads a world marks every entry done), the rows-without-marker case (seed applied as today, then marker) and the changed digest on a state with no row (seed runs from its first entry) have no Acceptance bullet, so each can be wrong while the three bullets pass"
```
