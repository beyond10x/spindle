---
format: aep.planning-md/3
id: review-result:wave27-sco-r3
kind: review-result
status: active
title: Plan critic (scope), round 3, on wave 27
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

- story:caller-attributes-on-loopback — Outcome/body heading — the front-matter title was changed, but the body's first heading still reads "# A loopback served run serves a command that reads its caller". The story delivers only a stderr line, and a run without either setting still answers 501, so the heading keeps the round-2 promise it was meant to drop. Change the heading to the new title — .engineering/planning/story/caller-attributes-on-loopback.md:27
- story:cross-component-related-reads — Joined/Acceptance — the Joined section quotes the consumer's runs as "(stdin or one served host)" and the stdin run goes through `unsupported_outcomes` (`main.rs:722`, Scope). "Decided when scoped" D1 A names only `admit` (all components or the one `--component`) and `component` (one). It says nothing about what the stdin run passes, and no Acceptance line runs a cross-component command on stdin. The consumer's gate drives Spindle on stdin, so the stdin half of its report is claimed by no acceptance. Add a stdin acceptance or a Decided line saying stdin passes all components — .engineering/planning/story/cross-component-related-reads.md:107

What I read: the order and `JOINED-OPEN.md` § Wave 27 first, then all 11 stories, the wave page and my two earlier records. There is still no epic. I extracted 12 promises and traced 11 whole to a story. The twelfth, the consumer's cross-component report, is only partly traced (the stdin half). The 12:
- seed-once
- stdin arrange and references
- stdin bindings
- wasm lanes
- the slot session's 3 cache changes
- map-input guard
- integer identity
- serve-loop exit
- cross-component reads
- query beside body
- caller attributes
- refused-bound recording

Earlier findings in my lane: the round-1 findings stay resolved. Both round-2 findings are partly resolved:
- The value read is now in the Outcome and has its own acceptance, `a_value_over_another_components_entity_is_read_from_its_owner`. That finding is closed.
- The caller story's front-matter title is fixed, but its body heading is not (first finding above).

Everything else holds:
- No two stories claim one outcome. `stdin-run-fires-bindings` prints a refused bound command; `refused-bound-command-recorded` records it in the request record.
- Round 2's new decisions stay inside the order: the reset's marker at count 0 (seed-once, serve-loop), `a_request_queued_at_the_signal_reads_503`, and the same-bindings test with two fixtures.
- The named omissions are honest: the caller header trusted without the secret, split-run reads, periodic bindings, PostgreSQL.
- No person, company, host or home path appears in any story body or in the wave page. I grepped for home paths, names, addresses and URLs. The only hits were version numbers and `127.0.0.1`.

Out of my lane:
- Acceptance: `stdin-arrange-and-line-references` Scope's Tests line lists five test names and omits `a_seeded_mint_avoids_an_arranged_identity`, which Acceptance has.
- Acceptance: the serve-loop Scope names `a_reset_taken_after_a_signal_empties_the_state_only_where_it_answers`, which Acceptance replaces. "Decided when scoped" says the Acceptance names win.

```findings
- file: .engineering/planning/story/caller-attributes-on-loopback.md
  line: 27
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the body heading still reads 'A loopback served run serves a command that reads its caller' after the title was changed to what the story delivers; the story only names the settings on stderr and a run without them still answers 501, so the heading keeps the round-2 promise it was meant to drop"
- file: .engineering/planning/story/cross-component-related-reads.md
  line: 107
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "Joined quotes the consumer's runs as 'stdin or one served host', but D1 A names only admit and component as passing the reached set, and no Acceptance line runs a cross-component command on stdin (main.rs:722); the stdin half of the report is claimed by nothing"
```
