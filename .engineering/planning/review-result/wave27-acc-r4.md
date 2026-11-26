---
format: aep.planning-md/3
id: review-result:wave27-acc-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on wave 27
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

- story:serve-loop-signal-exit-answers — "Decided when scoped" says this story removes the `ignore` of `adversary_w26_u3::the_answers_in_hand_are_written_before_a_rejecting_seed_stops_the_run` and rewrites it to "the request taken reads 2xx, the one still queued reads 503", but no Acceptance bullet names that test, so nothing observable says the ignored test now runs and passes — `.engineering/planning/story/serve-loop-signal-exit-answers.md:99`
- story:stdin-run-fires-bindings — D3 A ("a spec without components routes as one implied component") has no Acceptance bullet, and the named fixture is described only as "no `reached_by: network`", so a stdin run of a component-less specification firing its bindings is never checked, although D2 and D6 got bullets — `.engineering/planning/story/stdin-run-fires-bindings.md:105`

**Earlier findings.** Round 3's finding is resolved. In `story:serve-loop-signal-exit-answers`, `a_reset_cut_at_its_first_entry_resumes_from_the_second` (`.engineering/planning/story/serve-loop-signal-exit-answers.md:~63`) now has the first entry held, the marker at count 0 before it, the entry finishing, and the restart resuming from entry 2. That agrees with D2 A and with `seed.json` being written after each entry. Rounds 1 and 2 stay resolved.

**What I read.** All 11 stories in full with `aep plan artifact show` (saved under the scratch directory `w27/acc4`), the wave page, `review-result:wave27-acc-r1` to `-r3`, and the diff stat of `b066eba8f`. I did not run `aep plan artifact kinds` or `lifecycle`; the `kinds` output was printed, though. I checked in the tree that `play --component` exists in `cli.rs`. I also grepped the story bodies for person, company, host and home-path strings and found none. "The build node", "workstation pool" and "the host operator" are generic roles.

**Could not establish.**
- The ignore on the wave-26 test is on `wave/26` (commit `972760785`) but not in `plan/wave-27`'s tree. The story's "ignored since wave 26" is therefore true only once that branch is merged. I did not raise this.
- Whether the six wasm lanes other than the 626 s Node lane and the 120.9 s service-worker lane already end under 120 s today. The story's other bullets carry the transition, so I did not raise it.

**Out of my lane.** The Scope sections of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references` and `query-input-beside-body` still count tests differently from their Acceptance sections (scope critic, noted in round 2).

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 99
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Decided when scoped has the story remove the ignore of adversary_w26_u3::the_answers_in_hand_are_written_before_a_rejecting_seed_stops_the_run and rewrite it to 'the request taken reads 2xx, the one still queued reads 503', but no Acceptance bullet names that test, so the ignored-to-passing transition is not observable from the Acceptance section"
- file: .engineering/planning/story/stdin-run-fires-bindings.md
  line: 105
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D3 A (a spec without components routes as one implied component) has no Acceptance bullet and the fixture is described only as 'no reached_by: network', so a stdin run of a component-less specification firing its bindings is never checked, although D2 and D6 have bullets"
```
