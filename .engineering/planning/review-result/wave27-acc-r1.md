---
format: aep.planning-md/3
id: review-result:wave27-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 27
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

- story:stdin-run-fires-bindings — the Acceptance says a refused bound command leaves the run at exit 0, but "Decided when scoped" D4 A says a failed bound command counts as rejected and the run exits 1, so the test `a_bound_command_refused_is_printed_and_does_not_fail_the_line` asserts the opposite of what was decided; the acceptance also names no check for D2 (a refused binding stops the run before stdin) or D6 (`--drop-retries` accepted on stdin) — `.engineering/planning/story/stdin-run-fires-bindings.md:57`
- story:stdin-arrange-and-line-references — the D1 B decision ends mid-sentence ("... `/events/0/payload/<f>"}` into the"), so the reference form the acceptance tests cannot be told apart from a wrong one, and D3 A (a seeded mint avoids identities an arrange line wrote) has none of the five acceptance tests checking it — `.engineering/planning/story/stdin-arrange-and-line-references.md:103`
- story:seed-once-on-durable-state — D2 A ends mid-sentence ("a SIGKILL between commit and marker applies"), so the crash-recovery behaviour, an observable that the acceptance and the test list leave open, is not stated — `.engineering/planning/story/seed-once-on-durable-state.md:111`
- story:serve-loop-signal-exit-answers — acceptance 1 passes on either of two outcomes ("the reset is either answered 200 or leaves the state as it was") although D2 A decides one (a seeded reset cut by the signal answers 200), and D1 A ends mid-sentence ("...answered 503 before exit; acceptance 2") so how a queued request answered 503 relates to acceptance 2's "client reads the 400" is unstated — `.engineering/planning/story/serve-loop-signal-exit-answers.md:52`
- story:query-input-beside-body — acceptance 2 expects "the status and body ESS's binding gives" but D2 A decides a 400 with Spindle's `{"refused": ...}` body, so the status and body to assert are in conflict — `.engineering/planning/story/query-input-beside-body.md:45`
- story:query-input-beside-body — acceptance 3 keeps "a query field that is not a scalar" not served, yet D1 A serves `List<T>` query fields as repeated keys, so the acceptance does not say which non-scalar type must still be refused — `.engineering/planning/story/query-input-beside-body.md:47`
- story:caller-attributes-on-loopback — the acceptance still carries the settled-away first branch ("`a_loopback_request_proving_its_caller_runs_the_command` ... or, for the second branch, ..."), so it passes by either route although Scope settled the second; acceptance 2 is only "unchanged" with no check named in this section, and D1 A ends mid-sentence ("`run` prints one stderr line naming") — `.engineering/planning/story/caller-attributes-on-loopback.md:41`
- story:map-input-guard-facts — D1 A ends mid-sentence ("a Map binds `count` and its values by"), and the third acceptance bullet ("The served JSON route answers the interpreter's outcome with its status, not 501") names no test, though Scope names the CLI module `map_input_guard.rs` for it — `.engineering/planning/story/map-input-guard-facts.md:76`
- story:integer-identity-any-spelling — D1 A ends mid-sentence ("... after `check_input`; the"), so what the kernel exports for the host world to call (or its replacement) is not stated; D3 A (past 64 bits stays as written) has no bullet in the Acceptance — `.engineering/planning/story/integer-identity-any-spelling.md:73`
- story:wasm-lanes-within-budget — acceptance 4 and 5 already hold in the tree: `Taskfile.yml:27-32` runs the six lanes under `scripts/within.sh 120` and `Taskfile.yml:101`, `:109`, `:117` set `WASM_BINDGEN_TEST_TIMEOUT: '60'`, so those bullets read the same before and after (the "`'120'` today" claim is stale) and no `slot --timeout` appears in `Taskfile.yml`, so acceptance 6 is also already true — `.engineering/planning/story/wasm-lanes-within-budget.md:55`
- story:wasm-lanes-within-budget — D1 A ends mid-sentence ("... in a tree the shared cache has built; a"), so the conditions of the 120 s measurement are cut off — `.engineering/planning/story/wasm-lanes-within-budget.md:118`
- story:wasm-lanes-within-budget — the body names a host and a branch ("Measured by the slot session on the build node" and "branch a feature branch"), which the public-repository rule forbids — `.engineering/planning/story/wasm-lanes-within-budget.md:94`
- story:refused-bound-command-recorded — `a_bound_command_that_runs_adds_no_refusal_entry` is a bare test name with no statement of what the record holds, and the metrics count that D2 A decides (`spindle_bound_refusals_total`) has no acceptance bullet — `.engineering/planning/story/refused-bound-command-recorded.md:77`
- story:cross-component-related-reads — acceptance 1 to 3 still require the one-host refusal ("named not served by `admit` and `run` ... answers `unsupported`"), which D1 A reverses (a routing store reads the owner's store), so the acceptance states no read-from-owner outcome for the delivered branch and no split-run refusal; D2 A also ends mid-sentence ("refused by name in") — `.engineering/planning/story/cross-component-related-reads.md:77`

Read: all 11 stories in full with `aep plan artifact show` (the output was saved to a file and read in full), plus `.engineering/waves/wave-27.md`. I checked `Taskfile.yml` in the worktree and the raw story files for the truncated lines.

Could not establish:
- I did not run `aep plan artifact kinds` or `lifecycle`.
- I did not open the source files to check the cited `file:line` ranges beyond `Taskfile.yml`.
- Whether the mid-sentence "Decided when scoped" lines are cut in the stored body only, as I read them, or also in an earlier recorded revision.

Out of my lane:
- "the host operator" and "the operator" appear as roles in `refused-bound-command-recorded.md:116` and `cross-component-related-reads.md:192`. They are not a person or company, but the caller may prefer them neutral in a public repository.
- The "Dependencies" and "Shared files" claims between units are for the design and parallel-safety critics.

```findings
- file: .engineering/planning/story/stdin-run-fires-bindings.md
  line: 57
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance says a refused bound command leaves the run at exit 0 but D4 A says a failed bound command counts as rejected and the run exits 1, so the test asserts the opposite of the decision; D2 (router refusal stops the run) and D6 (--drop-retries accepted) have no acceptance check"
- file: .engineering/planning/story/stdin-arrange-and-line-references.md
  line: 103
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "D1 B ends mid-sentence so the reference form the acceptance tests is not stated, and D3 A (seeded mint avoids arrange-written identities) has no acceptance test"
- file: .engineering/planning/story/seed-once-on-durable-state.md
  line: 111
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "D2 A ends mid-sentence ('a SIGKILL between commit and marker applies') so the crash-recovery behaviour is not stated as an observable"
- file: .engineering/planning/story/serve-loop-signal-exit-answers.md
  line: 52
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "acceptance 1 passes on either of two outcomes (answered 200 or state unchanged) although D2 A decides one, and D1 A ends mid-sentence so the relation of a 503 for a queued request to acceptance 2's 400 is unstated"
- file: .engineering/planning/story/query-input-beside-body.md
  line: 45
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "acceptance 2 expects the status and body ESS's binding gives but D2 A decides a 400 with Spindle's {\"refused\": ...} body, so the expected body conflicts with the decision"
- file: .engineering/planning/story/query-input-beside-body.md
  line: 47
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "acceptance 3 keeps a non-scalar query field not served while D1 A serves List<T> query fields as repeated keys, so it does not say which non-scalar type must still be refused"
- file: .engineering/planning/story/caller-attributes-on-loopback.md
  line: 41
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance still carries the settled-away first branch with an 'or' so it passes by either route, acceptance 2 is only 'unchanged' with no check named, and D1 A ends mid-sentence"
- file: .engineering/planning/story/map-input-guard-facts.md
  line: 76
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A ends mid-sentence ('a Map binds count and its values by') and the served-route acceptance bullet names no test"
- file: .engineering/planning/story/integer-identity-any-spelling.md
  line: 73
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A ends mid-sentence ('... after check_input; the') and D3 A (past 64 bits stays as written) has no acceptance bullet"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 55
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: pre-existing
  message: "acceptance 4, 5 and 6 already hold in the tree (Taskfile.yml:27-32 run the six lanes under within.sh 120, :101/:109/:117 set WASM_BINDGEN_TEST_TIMEOUT 60, no slot --timeout), so they read the same before and after the work"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 118
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A ends mid-sentence ('... in a tree the shared cache has built; a') so the conditions of the 120 s measurement are cut off"
- file: .engineering/planning/story/wasm-lanes-within-budget.md
  line: 94
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the body names a host (the build node) and a branch (a feature branch, line 36) in a public repository"
- file: .engineering/planning/story/refused-bound-command-recorded.md
  line: 77
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "acceptance 2 is a bare test name with no statement of what the record holds, and the D2 A metrics count has no acceptance bullet"
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 77
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "acceptance 1 to 3 still require the one-host refusal by admit and run, which D1 A reverses, so no acceptance states the read-from-owner outcome or the split-run refusal; D2 A also ends mid-sentence"
```
