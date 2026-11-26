---
format: aep.planning-md/3
id: review-result:wave27-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 27
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

- story:serve-loop-signal-exit-answers — acceptance 2 says "the client reads the 400" but D1 A rewrites it to "400 or 503, never EOF", so the text a test is written from names one outcome while the decision allows two. The acceptance should state the 400-or-503 outcome itself, and the Scope's test name for acceptance 1 (`a_reset_taken_after_a_signal_empties_the_state_only_where_it_answers`) should be aligned with the acceptance's `a_reset_taken_after_a_signal_is_answered_200` — `.engineering/planning/story/serve-loop-signal-exit-answers.md:56`

Round 1 findings, 14 in all:
- **Resolved (13):** the cut-off "Decided when scoped" lines are now complete in all 11 stories. The stale or opposite acceptance bullets are fixed in `stdin-run-fires-bindings`, `query-input-beside-body`, `caller-attributes-on-loopback`, `cross-component-related-reads` and `refused-bound-command-recorded`. The missing decision checks are now covered in `stdin-arrange-and-line-references`, `integer-identity-any-spelling` and `refused-bound-command-recorded`. The "already holds" bullets of `wasm-lanes-within-budget` are rewritten as a measured 626 s → ≤120 s transition, a new lint check, and a bullet marked "Kept as they hold since wave 25".
- **Partly resolved (1):** the serve-loop acceptance-1 fix landed (200 now decided). Acceptance 2 still conflicts with D1 A (the finding above).

Read: 11 stories in full with `aep plan artifact show` (output saved to files and read in full), plus `round-1 review-result:wave27-acc-r1` and `.engineering/waves/wave-27.md`. I checked `Taskfile.yml`, the test names `ingress.rs:457` and `trusted_caller.rs:263`, and a grep of every story body for person, company, host and home-path strings.

Could not establish:
- Whether `check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store` and `check:the_browser_store_keeps_state_in_indexeddb` already end under 120 s today. If they do, those bullets of `wasm-lanes-within-budget` hold before the work, but the story's other bullets still carry the transition, so I did not raise it.
- The public-repo scan found no person, company, host name or home path. The wasm story still says "the build node" and "workstation pool" as generic roles, which I read as not identifying. I did not re-raise round 1's blocker on this.
- `caller-attributes-on-loopback` acceptance 2 names `a_loopback_request_proving_its_caller_runs_the_command`, but the existing test at `trusted_caller.rs:263` is `a_command_reading_the_caller_runs_for_the_caller_the_request_proves`. Both are checkable, so this is wording and not a finding.

Out of my lane (scope critic): several Scope sections still count the old number of tests. `stdin-run-fires-bindings` says "the four named acceptance tests" against six listed, `stdin-arrange-and-line-references` says "five" against six, and `query-input-beside-body` says "three" against four.

```findings
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 56
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "acceptance 2 says the client reads the 400 but D1 A rewrites it to '400 or 503, never EOF', so the acceptance text names one outcome while the decision allows two; the Scope test name for acceptance 1 also differs from the acceptance's"
```
