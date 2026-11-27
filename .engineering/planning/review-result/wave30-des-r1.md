---
format: aep.planning-md/3
id: review-result:wave30-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 30
relations:
- reviews: story:view-rows-absent-optional-fields
- reviews: story:view-filter-nested-element-quantifier
- reviews: story:view-filter-map-value-order
- reviews: story:error-payload-generated-fields
- reviews: story:unknown-instance-unsourced-unserved
- reviews: story:create-on-held-identity-unavailable
- reviews: story:decimal-identity-any-spelling
- reviews: story:look-ahead-asks-only-where-it-matters
- reviews: story:related-via-nested-input
- reviews: story:events-cursor-across-restart
- reviews: story:peer-connect-test-steady
- reviews: story:rally-example-reads-stdin
revision: 1
---
approve

**What I read:** all 12 stories (`aep plan artifact show`), the wave page `.engineering/waves/wave-30.md`, the wave 29 page (`git show plan/wave-29:.engineering/waves/wave-29.md`), `aep plan artifact relations`, `aep plan artifact graph --format dot` and `aep plan artifact validate`.
- **Edges:** 20 edges touch the set; only 2 are `depends_on`, and both stay inside the set. No edge points into the set from outside, so there can be no cycle that leaves the set and comes back.
- **Acceptance reads:** one acceptance reads what a sibling produces. `story:error-payload-generated-fields` `an_unsourced_error_field_draws_as_ess_draws` reads the refusal rule that `story:unknown-instance-unsourced-unserved` D1 A adds, and the `depends_on` edge records it.
- **The other edge:** `view-filter-map-value-order -> view-filter-nested-element-quantifier` exists because both change the same `Exists`/`Forall` arm. That is a shared surface, and each story can still be shown working on its own.
- **Code checked:** `Unsourced` (`crates/spindle-capability/src/mint.rs:186-202`) and `unsourced_error_fields` (`crates/spindle-kernel/src/execute.rs:2352-2420`). The two U3 stories change different arms of that function (`Unanswered` and `Minted`), so this is not a split abstraction.
- **No hidden U5/U6 dependency:** the U6 acceptance test (`adversary_w27_u5.rs:590`) weighs one scale and then reweighs it. It never creates a second row with the same identity, so it does not read the held-identity answer that U5 changes.
- **No horizontal slice and no chain:** each story can be shown working end to end on its own, and 8 of the 10 units have no ordering edge.
- **Personal details:** no person, organisation, host or home path in any story body.

`validate` prints `valid`, plus old warnings about review-results with no recorded outcome. None of those warnings is about this set.

**What I could not establish:**
- **Weak reason on the U3 edge (no finding):** the wave page says U3's two stories "rewrite the same `unsourced_error_fields`". But `story:unknown-instance-unsourced-unserved` Scope says it changes only that function's doc comment. The edge is still needed because of the acceptance read above, so the order holds.
- **Wave 29 overlaps (parallel-safety lane, not mine):** wave 30 starts after wave 29 merges, and every wave-30 scope was written against main plus wave 28, so line numbers will drift.
  - `story:error-payload-generated-fields` changes `crates/spindle-kernel/src/values.rs:433`, and wave 29 U2 changes `values.rs`. The wave 30 Surfaces section does not list `values.rs`.
  - `story:unknown-instance-unsourced-unserved` changes `supported.rs`, which wave 29 U2 also changes.
  - `story:rally-example-reads-stdin` changes `crates/spindle-cli/src/cli.rs` `Command::Run` and `website/docs/reference/cli.md`, which wave 29 U1 also changes.
  - `story:view-rows-absent-optional-fields` and `story:create-on-held-identity-unavailable` change `crates/spindle-conformance/src/target.rs`, which wave 29 U1 also changes.
  - The wave 30 units that touch `crates/spindle-host` (`view.rs`, `host.rs`, `host/component.rs`, `host/range.rs`) overlap wave 29 U1, which owns all of `spindle-host` in wave 29.
- **Not checked:** whether wave 29 U1 adds a flag to `run` that would conflict with `--stdin`. I did not read wave 29's story bodies.

```findings
[]
```
