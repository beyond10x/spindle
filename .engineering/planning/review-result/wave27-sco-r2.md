---
format: aep.planning-md/3
id: review-result:wave27-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 27
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

- story:cross-component-related-reads — the consumer's actual hit is a value read (`{related: ...}` in an outcome) in one host, and the Outcome (`:49`) and every Acceptance line name only guards, row sets and `affects:`. The Joined section promises "read the value, the guard's row and the row set from the owner's store when both components run in one host". Nothing in Acceptance claims the value read, though Scope flips the two value tests to `…_is_read_from_its_owner`. Add a value-read acceptance and name values in the Outcome — .engineering/planning/story/cross-component-related-reads.md:103
- story:caller-attributes-on-loopback — the round-1 gap is fixed in the Outcome, which now says "Not delivered" for trusting the header without the secret. The title still reads "A loopback served run serves a command that reads its caller", which promises the consumer's route is served. The story only changes the stderr text, and a run without either setting still answers 501. Retitle to what is delivered (the refusal names how to serve it) — .engineering/planning/story/caller-attributes-on-loopback.md:24

What I read: no epic parent, so I took the promises from `JOINED-OPEN.md` § Wave 27 plus `story:refused-bound-command-recorded`. I read all 11 stories whole (`aep plan artifact show` for each), `.engineering/waves/wave-27.md`, `review-result:wave27-sco-r1`, `w27/decisions.md`, and `cross_component_reads.rs`, and grepped the story bodies for persons, hosts and paths. I extracted 12 promises and traced 12 to a story:
- seed-once
- stdin arrange and references
- stdin bindings
- wasm lanes, with the slot session's three cache changes
- map, integer and serve-loop
- cross-component reads
- query beside body
- caller attribute
- refused-bound recording
- the order's "resolve all missing spindle features"

Two of the 12 are only partly claimed, and they are the two findings above.

Round-1 findings in my lane: all 11 are resolved.
- The cut "Decided when scoped" texts are whole in the stored bodies of serve-loop, stdin-arrange, map, integer, seed-once, caller and wasm.
- The stale "Not covered" in cross-component now reads "kept refused by name" for the split run only.
- D2 of cross-component is complete.
- The wasm body says "the build node", with no host name, person or home path.
- No person, company, host or home path appears in any story body. `b10x-spindle` is the binary's name.
- The three slot cache changes are claimed: the runner in `.cargo/config.toml`, one profile directory (the Scope's "no change expected" is recorded, and the new lint holds it), and `serde_json` `alloc` (D2).
- The two wasm-lanes claims the same way: the story says the lanes were already one profile and ran 120.9 s on the service-worker lane.
- No two stories claim one outcome. `stdin-run-fires-bindings` prints a refused bound command and `refused-bound-command-recorded` records it in the served request record. They are different surfaces.
- The named omissions are honest: the ADR-amending first branch of caller-attributes, the split run, periodic bindings, and PostgreSQL.

What I could not establish:
- Out of my lane, acceptance: `story:stdin-arrange-and-line-references` Scope says "five acceptance tests" and Acceptance lists six. `story:serve-loop-signal-exit-answers` Scope names `a_reset_taken_after_a_signal_empties_the_state_only_where_it_answers` and Acceptance names `a_reset_taken_after_a_signal_is_answered_200`.
- Out of my lane, design: `story:refused-bound-command-recorded` edits ESS domain YAML, generated projections and the UI. I traced it to the consumer's report and the host operator's confirmation, so I did not count it as reach.

```findings
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 103
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Joined section promises to read the value, the guard's row and the row set from the owner's store in one host, but the Outcome and every Acceptance line name only guards, row sets and affects, so the consumer's value read, which Scope's flipped tests cover, is claimed by no acceptance"
- file: .engineering/planning/story/caller-attributes-on-loopback.md
  line: 24
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the title promises a loopback run serves a caller-reading command, but the Outcome delivers only a stderr line naming the two settings and a run without either still answers 501; retitle to the delivered change (part of the round-1 finding is fixed in the Outcome)"
```
