---
format: aep.planning-md/3
id: review-result:d620-parallel-r1
kind: review-result
status: active
title: D620 stories, parallel-safety critic, round 1
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
needs-revision

Findings:

story:response-field-value-source — its scope lines `crates/spindle-kernel/src/supported.rs` and `crates/spindle-kernel/src/values.rs` (inferred, scope field) are also claimed by active `story:kernel-related-row-values` (cited, the `other =>` arm of `value`, plus `values.rs`) and `story:kernel-set-outcomes` (`supported.rs` cited, `values.rs` inferred). Both rewrite the same `value` match in `supported.rs` that this story edits, and neither body names the other. `supported.rs` is also claimed by `story:kernel-delete-outcomes`, `kernel-row-set-guards`, `kernel-secondary-effects` and `state-change-conditions`. Remedies, without choosing: a `depends_on` edge recording the shared file, or splitting the surface. — .engineering/planning/story/response-field-value-source.md:9-18

story:response-field-value-source — the scope omits `crates/spindle-kernel/src/execute.rs`, the file that holds the response-field refusal and the response mint (`execute.rs:1316`, inferred). Active `story:structured-response-values` and `kernel-set-outcomes` claim `execute.rs`. The real surface is wider than declared, so the collision above is understated. — .engineering/planning/story/response-field-value-source.md:9

story:http-outcome-bodies — its scope lines `crates/spindle-ingress/src/answer.rs` and `crates/spindle-ingress/src/surface.rs` (inferred) are also claimed by active `story:fixed-caller-per-grant` (cited, both), `story:ingress-request-log` (cited, both, plus `lib.rs` and `listener.rs`), `story:outcome-scoped-refusal` (inferred, both) and `story:refusal-payload-complete` (`answer.rs`, cited). `story:live-topology` also claims `surface.rs`. No body names this story. Remedies, without choosing: an ordering edge recording the shared file, or splitting the surface. — .engineering/planning/story/http-outcome-bodies.md:7-16

story:http-outcome-bodies — it and `story:outcome-scoped-refusal` both rewrite the one refusal site, `surface.rs:851-858`, which the body cites. Its third acceptance line refuses an unserved construct "by name when the surface is built". `outcome-scoped-refusal` makes unsupported outcomes refuse only when chosen. The two disagree on when that refusal fires, and the body does not say which wins (cited, both bodies). — .engineering/planning/story/http-outcome-bodies.md:25-31

Neither story is the pair's own collision: the two stories share no file with each other (kernel against ingress), so they can run together. Both are in `waves` output only as `story:http-outcome-bodies` in wave 1; `story:response-field-value-source` did not appear in any wave, though it has a `depends_on` edge.

What I read:
- 2 target stories in full.
- Scope and edges of all 22 active stories, via `aep plan artifact list --status active` and `show`.
- `aep plan artifact waves` and `graph`.
- Bodies of `outcome-scoped-refusal`, `fixed-caller-per-grant`, `kernel-related-row-values` and the draft `ingress-error-object-bodies`.
- Code in spindle-wave10: `surface.rs:845-860` (the refusal site) and the kernel `execute.rs`/`supported.rs` response-field refusal sites.
- A grep of every active story body for either target's id found no mention.
- Surfaces: 0 cited, 2 inferred (both stories' scopes are marked inferred), 0 unplaceable. Both bodies cite paths in prose, and the code confirms them.
- Reader/producer reads traced to a producer in the set: 0 (the two stories do not feed each other). Edges recording them: n/a.

What I could not establish:
- Whether `response-field-value-source` actually needs `execute.rs` (inferred from the refusal strings only).
- Why `response-field-value-source` is absent from `waves`.
- The draft `story:ingress-error-object-bodies` (`surface.rs`, error-object bodies) overlaps `http-outcome-bodies`' error-object kind. It is outside the set, so I make no finding. The design or scope critic should judge it.
- Whether the two target stories should be one wave. That is the operator's call.

```findings
- file: .engineering/planning/story/response-field-value-source.md
  line: 9
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "scope supported.rs and values.rs (inferred) is also claimed by active story:kernel-related-row-values and story:kernel-set-outcomes (supported.rs also by kernel-delete-outcomes, kernel-row-set-guards, kernel-secondary-effects, state-change-conditions); no body names the overlap; add an ordering edge recording the shared file or split the surface"
- file: .engineering/planning/story/response-field-value-source.md
  line: 9
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "scope omits crates/spindle-kernel/src/execute.rs, where the response-field refusal and response mint live (inferred from execute.rs:1316), which active structured-response-values and kernel-set-outcomes claim; the collision is understated"
- file: .engineering/planning/story/http-outcome-bodies.md
  line: 7
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "scope answer.rs and surface.rs (inferred) is also claimed by active fixed-caller-per-grant, ingress-request-log, outcome-scoped-refusal, refusal-payload-complete (answer.rs) and live-topology (surface.rs); no body names the overlap; add an ordering edge recording the shared file or split the surface"
- file: .engineering/planning/story/http-outcome-bodies.md
  line: 25
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "both this story and active outcome-scoped-refusal rewrite the refusal at surface.rs:851-858, and the third acceptance (refuse an unserved construct by name when the surface is built) conflicts with outcome-scoped-refusal (refuse only when chosen); the body does not say which governs"
```
