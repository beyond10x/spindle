---
format: aep.planning-md/3
id: review-result:wave27-acc-r11
kind: review-result
status: active
title: Plan critic (acceptance), round 11, on wave 27
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
approve

No findings. The one finding open from round 10 is resolved, and every decided behaviour of the 11 stories now maps to a checkable Acceptance bullet.

Earlier findings in my lane:
- **`-r10` (`story:stdin-run-fires-bindings`):** resolved. Acceptance bullet 3, `a_rejected_bound_command_is_printed_and_fails_the_run`, now names the kernel-rejection path: a `failure` answer printed on the `caused_by` line, `C` counted rejected, every line read, exit 1.
- **`-r1` to `-r9`:** all still resolved. I re-checked their fixes against the current bodies.

This pass I mapped every "Decided when scoped" item, and each Decision (D1 to D6) of all 11 stories, to an Acceptance bullet:
- **`stdin-run-fires-bindings`:** D1 to D6 map to bullets 1 to 8.
- **`stdin-arrange-and-line-references`:** D1 B, D2 A and D3 A map to the arrange, reference, rejection and seeded-mint bullets.
- **`seed-once-on-durable-state`:**
  - The marker mode, resume, changed digest, empty-state exception, state with no marker and reset with a world map to bullets 1, 2 and 4 to 8.
  - The reset with no world, including the count-0 marker, maps to bullets 3 and 9.
  - D2 A is a stated limit, not a check.
- **`serve-loop-signal-exit-answers`:** D1 A and D2 A, the marker count at a reset, and the un-ignored `adversary_w26_u3` test map to bullets 1 to 6.
- **`query-input-beside-body`:** D1 A and D2 A map to bullets 4 and 2.
- **`caller-attributes-on-loopback`:** D1 A maps to bullet 1, and the 501 and exit-2 cases stay green in bullets 2 and 3.
- **`map-input-guard-facts`:** D1 A maps to bullets 1 to 4.
- **`integer-identity-any-spelling`:** D1 A and D3 A map to bullets 1 to 3. D2 A is a changelog statement.
- **`wasm-lanes-within-budget`:** D1 A maps to the measurement bullets and D2 A to the lane bounds.
- **`refused-bound-command-recorded`:** the place decision and D1 A to D3 A map to bullets 1 to 8.
- **`cross-component-related-reads`:** D1 A, D2 A, the stdin-passes-all rule and the additive widening map to bullets 1 to 8.

I also checked the code to see whether the edges I considered need a bullet:
- **Retries:** the retry loop in `Host::attempt` (`crates/spindle-host/src/host.rs:1876-1944`) reports once, after the last attempt. Bullet 5 of `stdin-run-fires-bindings` ("runs once and is not retried") is therefore checkable.
- **Foreign `affects:` on stdin:** `unsupported_outcomes` (`crates/spindle-cli/src/main.rs:906`) prints one stderr line per command and does not stop the run at start. So bullet 7 of `cross-component-related-reads`, which uses the same fixture as the foreign-`affects:` command, is consistent.

A grep of the story bodies for person, company, host, URL and home-path strings found nothing. "The build node" and "the host operator" are roles.

What I read: 11 of 11 ids, each in full with `aep plan artifact show`. They are `story:stdin-run-fires-bindings`, `story:stdin-arrange-and-line-references`, `story:seed-once-on-durable-state`, `story:serve-loop-signal-exit-answers`, `story:query-input-beside-body`, `story:caller-attributes-on-loopback`, `story:map-input-guard-facts`, `story:integer-identity-any-spelling`, `story:wasm-lanes-within-budget`, `story:refused-bound-command-recorded` and `story:cross-component-related-reads`. I also read the wave page, the procedure and `review-result:wave27-acc-r10`. I ran `aep plan artifact lifecycle story`, not `kinds`.

Could not establish:
- **Wasm lane times:** whether the six lanes end under 120 s today. Each bullet names the bound and the measurement.
- **Unmet bindings on stdin:** a binding whose input is unmet is a `Failed` entry with `attempts: 0` and no command sent. No decision says whether a stdin run prints it or counts it rejected under D4 A. I read this as an undecided edge for the implementor, not a decided behaviour left without a check.
- **Foreign `Scanned`:** whether it fails as "moved" in `cross-component-related-reads`. It is a safety fact for the implementor to prove.

Out of my lane:
- The Scope test lists of five stories, among them `stdin-run-fires-bindings` and `serve-loop-signal-exit-answers`, differ from their Acceptance names. The stories already say the Acceptance names replace them.
- Bullets 7 and 8 of `cross-component-related-reads` name no test location in Scope (scope critic).
- Stories sit in `draft` while the wave Exit says `active -> implemented`; the lifecycle needs `draft -> proposed -> active` first (not my lane).

```findings
[]
```
