---
format: aep.planning-md/3
id: review-result:d620-parallel-r2
kind: review-result
status: active
title: D620 stories, parallel-safety critic, round 2
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
needs-revision

All four round-1 findings are fixed, but the pass turned up two new warnings. They are the only findings, and both warn that a body omits a collision. Neither is a blocker.

**Round-1 findings**
1. Fixed. `response-field-value-source.md:51` now names the `supported.rs`, `values.rs` and `execute.rs` sharers. `aep plan artifact waves` confirms none is in its wave 17: related-row-values is in 10, set-outcomes in 8, row-set-guards in 7, secondary-effects in 12, state-change-conditions in 11, and structured-response-values in 16.
2. Fixed. `execute.rs` is now in the scope, as inferred.
3. Fixed. `http-outcome-bodies.md:56` names `fixed-caller-per-grant`, `refusal-payload-complete`, `ingress-request-log` and `live-topology`. Waves puts `http-outcome-bodies` in 11, `ingress-request-log` in 12, `live-topology` in 17, `outcome-scoped-refusal` in 9, and the two merged stories in 6 and 10.
4. Fixed. `http-outcome-bodies.md:51-55` adds `depends_on story:outcome-scoped-refusal` and says the last acceptance holds under either timing. The refusal site is still at `surface.rs:857`, in `spindle-wave10`.

**New findings**
- `story:response-field-value-source` — its Order section lists the `supported.rs` sharers but omits `story:outcome-scoped-refusal`, which also claims `supported.rs` (inferred, scope field) and is not in the body. Waves separates the two (9 against 17), but only through scope, and the body does not name the pair. Remedy: name it in Order, or add an edge. — `.engineering/planning/story/response-field-value-source.md:51`
- `story:http-outcome-bodies` — both stories add integration tests under `crates/spindle-cli/tests/` (inferred from the acceptance, which runs routes and `b10x-spindle capabilities` through the CLI). Neither names a test file in its scope. Active `story:one-test-binary-per-crate` (cited) restructures that whole directory into one binary and says it "Leads wave 11" (`.engineering/planning/story/one-test-binary-per-crate.md`, Order section). Waves puts that story in wave 1 and `http-outcome-bodies` in wave 11, so the edge-free ordering is not what its own Order text says. The same applies to `response-field-value-source`. Remedies, without choosing: an ordering edge recording the shared directory, or splitting the surface so the new test file is named and registered after the restructure. — `.engineering/planning/story/http-outcome-bodies.md:10-16`

**What I read**
- Both target stories in full, round-1 `review-result:d620-parallel-r1`, and `aep plan artifact waves`, including the collision lines for both targets.
- The scope and relations of 17 other active stories.
- The statuses of 11 collision partners, which are all implemented.
- `surface.rs` and the `crates/spindle-*/tests` listings in `spindle-wave10`.
- Surfaces: 0 cited and 7 inferred (the scope fields of both stories are inferred); 0 unplaceable.
- Reader/producer reads traced to a producer in the set: 3 (the response read by `response-field-value-source`, the refusal site read by `http-outcome-bodies`, and the response member read by `http-outcome-bodies`). All three are recorded by a `depends_on` edge or by a merged or implemented producer.

**What I could not establish**
- Whether `one-test-binary-per-crate` has a real edge to either story. Its Order text and its wave 1 placement disagree.
- `story:ingress-error-object-bodies` is a draft that shares `surface.rs` and the same refusal site as `http-outcome-bodies`. It is outside the set, so I make no finding, but a scope critic should look at it.
- `kernel-delete-outcomes`, `fixed-caller-per-grant` and `refusal-payload-complete` are merged on wave/10 and await their store close, so I treated their collisions as resolved.

```findings
[
  {"file": ".engineering/planning/story/response-field-value-source.md", "line": 51, "category": "parallel-safety", "severity": "warning", "message": "Order lists the supported.rs sharers but omits story:outcome-scoped-refusal, which also claims supported.rs (inferred, scope field); waves separates them (wave 9 and wave 17) only through scope and the body does not name the pair"},
  {"file": ".engineering/planning/story/http-outcome-bodies.md", "line": 10, "category": "parallel-safety", "severity": "warning", "message": "inferred: both stories add CLI integration tests under crates/spindle-cli/tests/ without naming a test file, and active story:one-test-binary-per-crate (cited) restructures that directory into one binary; neither body mentions the other, and its Order says it leads wave 11 while waves puts it in wave 1. The same holds for story:response-field-value-source (its scope, line 10)"}
]
```
