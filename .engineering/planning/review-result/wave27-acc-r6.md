---
format: aep.planning-md/3
id: review-result:wave27-acc-r6
kind: review-result
status: active
title: Plan critic (acceptance), round 6, on wave 27
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

- story:seed-once-on-durable-state — the decided mid-seed resume has no Acceptance bullet: "It is written after each entry, so a run stopped mid-seed resumes after the last entry done" and the listed test `a_run_stopped_mid_seed_resumes_after_the_last_entry_done` are not among the seven bullets, so a restart that replays the seed from its first entry after a stop mid-seed passes all of them. The serve-loop story's reset-cut bullet covers only a reset's seed, and this story's bullets 1 to 3 start from a seed that ran to the end — `.engineering/planning/story/seed-once-on-durable-state.md:105` (section "Decided when scoped") and `:95`, against the Acceptance bullets at `:52-67`.

**Earlier findings in my lane.** Both round 5 findings are resolved by the fix at `de4e00daa`:
- `refused-bound-command-recorded` now has `a_rejected_bound_command_is_listed_with_its_kind`, which drives the kernel-rejection (`Failed`) path.
- `seed-once-on-durable-state` now has `a_restart_after_a_reset_with_a_world_sends_no_seed_entry`, `a_state_with_rows_and_no_marker_is_seeded_then_marked` and `a_changed_seed_on_a_state_with_no_row_runs_from_its_first_entry`.

I took rounds 1 to 4 from the round 5 record's statement that they stay resolved. I did not re-read `-r1` to `-r4`. The finding above is new this round. It is the same kind of gap as round 5, and I missed it then. I found nothing to change in the other ten stories. The residual ones I considered, such as the "also likely" tests of `query-input-beside-body` and the SIGKILL double-apply of D2 A, are not decided behaviours or are not observable, so they are not findings.

**What I read.** 11 of 11 stories, each in full with `aep plan artifact show` (saved under `<local path>`), on `plan/wave-27` at head `203ee1d17`. I also read `review-result:wave27-acc-r5`, the brief, and the critic rubric and role files. Story bodies hold no person, company, host or home path. "The build node" and "the operator" are generic roles.

**Could not establish.**
- Whether the six wasm lanes other than the 626 s Node lane and the 120.9 s service-worker lane already end under 120 s. The bullets name the transition for each, so I did not raise it.
- Whether `adversary_w26_u3` carries an `ignore` in this tree. Wave 26 is stated to be merged, but I did not read the test file.
- I did not run `aep plan artifact kinds` or `lifecycle`.

**Out of my lane.** The Scope sections of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references` and `query-input-beside-body` count tests differently from their Acceptance sections. The scope critic should look at that, as noted in round 2.

```findings
- file: .engineering/planning/story/seed-once-on-durable-state.md
  line: 105
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the decided mid-seed resume (the marker is written after each entry, so a run stopped mid-seed resumes after the last entry done; test a_run_stopped_mid_seed_resumes_after_the_last_entry_done at line 95) has no Acceptance bullet, so a restart that replays the seed from its first entry after a stop mid-seed passes all seven bullets"
```
