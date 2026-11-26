---
format: aep.planning-md/3
id: review-result:wave27-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 27
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

- story:serve-loop-signal-exit-answers — acceptance `a_signal_before_a_resets_first_entry_seeds_again_on_restart` has the signal arrive "while a reset's first seed entry is held" and expects the marker to read count 0. D2 A has the entry under way finish, and the same story writes `seed.json` "at the count of entries done, written after each". The test therefore reads count 1 and resumes after entry 1, so the acceptance and the decision cannot both pass. Name the moment: either the signal arrives before the first entry is sent and the marker is count 0, or the signal arrives with the entry held and the restart resumes from entry 2. — `.engineering/planning/story/serve-loop-signal-exit-answers.md:63`

Round 2 finding (`serve-loop-signal-exit-answers`, acceptance 2 against D1 A) is resolved:
- D1 A now says a reset refused before the signal "reads its 400 (acceptance 2)".
- The "test names in the Acceptance section replace the names the Scope section lists" line settles the test-name mismatch.

The round 1 findings stay resolved, and I raised no new ones on the other ten stories.

What I read: all 11 stories in full with `aep plan artifact show` (saved to the scratch directory), the wave page, and `review-result:wave27-acc-r1` and `-r2`. I also checked `Taskfile.yml`, `Cargo.toml` profiles, `.cargo/config.toml`, `adversary_w25_reseed_r2.rs`, the `holding_desk` helper and the `Play` command in `cli.rs`. A grep of the story bodies for person, company, host and home-path strings found nothing.

Could not establish:
- Whether the six wasm lanes other than the 626 s Node lane and the 120.9 s service-worker lane already end under 120 s. The story's other bullets still carry the transition, so I did not raise it.
- `story:cross-component-related-reads` acceptance 4 names a `play --component` run, but only `admit` and the host test are listed under Tests. The acceptance is checkable as written, so I did not raise it.
- `story:wasm-lanes-within-budget` acceptance 5 cites "the slot story" without an id. It is a report requirement and checkable, so I did not raise it.

Out of my lane: the wave page's Surfaces section says U1 `:79-100` for the `cli.rs` `run` help. That is for the parallel-safety critic.

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 63
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "acceptance a_signal_before_a_resets_first_entry_seeds_again_on_restart sends SIGTERM while the first entry is held and expects marker count 0, but D2 A finishes the entry under way and the marker is written after each entry, so the test reads count 1 and resumes after entry 1; the acceptance and the decision cannot both hold"
```
