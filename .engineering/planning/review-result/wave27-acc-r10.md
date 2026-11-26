---
format: aep.planning-md/3
id: review-result:wave27-acc-r10
kind: review-result
status: active
title: Plan critic (acceptance), round 10, on wave 27
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

- story:stdin-run-fires-bindings — D4 A decides "A failed bound command counts as rejected; the run exits 1", but the only bullet that checks printing and the exit status (`a_refused_bound_command_is_printed_and_fails_the_run`) drives a declared refusing outcome. The kernel-rejection path is the `failure` member that the stdin loop counts at `crates/spindle-cli/src/main.rs:776-791`. Its only bullet, `drop_retries_is_accepted_on_stdin` ("a bound command that fails runs once and is not retried"), checks neither the printed line nor the exit status. An implementation that prints a kernel-rejected `C` but exits 0 passes every bullet. Name in bullet 2 or 4 that a bound command the kernel rejects is printed on its `caused_by` line, counted rejected, and the run exits 1 — `.engineering/planning/story/stdin-run-fires-bindings.md:30-36` (Acceptance) against `:103` (D4 A)

Earlier findings in my lane: both round 9 findings are resolved, and rounds 1 to 8 stay resolved.
- `serve-loop-signal-exit-answers` bullet 2 is now `a_refused_answer_handed_before_close_is_written`, a deterministic `spindle-ingress` test. It no longer depends on a race with SIGTERM.
- `refused-bound-command-recorded` bullet 1 now says `GET /api/events` lists no entry for the refused `C`.
- I re-read `-r1` to `-r9` and checked each fix against the current bodies.

This pass I mapped every "Decided when scoped" item and decided option of all 11 stories to an Acceptance bullet. The finding above is the only decided behaviour left without a checkable bullet.

What I read: 11 of 11 stories (`story:stdin-run-fires-bindings`, `story:stdin-arrange-and-line-references`, `story:seed-once-on-durable-state`, `story:serve-loop-signal-exit-answers`, `story:query-input-beside-body`, `story:caller-attributes-on-loopback`, `story:map-input-guard-facts`, `story:integer-identity-any-spelling`, `story:wasm-lanes-within-budget`, `story:refused-bound-command-recorded`, `story:cross-component-related-reads`), each in full with `aep plan artifact show`. I also read the wave page and `review-result:wave27-acc-r1` to `-r9`. I checked `crates/spindle-cli/src/main.rs:741-791` at head `d82587e4f`. A grep of the story bodies for person, company, host, URL and home-path strings found nothing. "A consumer", "the build node" and "the host operator" are roles.

Could not establish:
- I did not run `aep plan artifact kinds` or `lifecycle` this pass.
- Whether the six wasm lanes end under 120 s today. Each bullet names the bound and the measurement.
- Whether foreign `Scanned` passed through unchanged fails as "moved" (`cross-component-related-reads`, Safety fact to prove). I read it as the implementor's proof. Bullet 3 "selects the outcome" implies the write commits.
- I read the Scope-only behaviours of `query-input-beside-body` as Scope, not decided: an unlisted query key under `refuse` or `ignore`, a scalar key given twice, and the routes without `request.query` that keep their bytes. The same goes for the `alloc` feature in `wasm-lanes-within-budget`, the SIGKILL double-apply in `seed-once-on-durable-state` D2 A, and the additive widening in `cross-component-related-reads`.

Out of my lane: the Scope test lists of `stdin-run-fires-bindings`, `stdin-arrange-and-line-references`, `query-input-beside-body`, `serve-loop-signal-exit-answers` and `refused-bound-command-recorded` differ from their Acceptance names (scope critic). The stories already state that the Acceptance names replace them.

```findings
- file: .engineering/planning/story/stdin-run-fires-bindings.md
  line: 30
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D4 A decides a failed bound command counts as rejected and the run exits 1, but the one bullet that checks print and exit drives only a declared refusing outcome, and the kernel-rejection path (a `failure` member, main.rs:776-791) is covered only by drop_retries_is_accepted_on_stdin, which names neither a printed line nor an exit status; an implementation that prints a kernel-rejected C but exits 0 passes every bullet"
```
