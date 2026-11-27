---
format: aep.planning-md/3
id: review-result:wave30-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 30
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
needs-revision

story:decimal-identity-any-spelling — its new acceptance line `an_established_decimal_identity_is_found_whatever_its_spelling` calls `Host::establish`, but Scope gives it no test file. `spindle-kernel` has no `spindle-host` dev-dependency, so the test cannot go in the kernel's `decimal_identity.rs` where its sibling tests go. It must land in the `spindle-host` or `spindle-conformance` tests. If it lands as a new `spindle-host` module, it adds a line to `crates/spindle-host/tests/spindle-host/main.rs`, and the wave page lists only U1 and U8 for that file (inferred) — .engineering/planning/story/decimal-identity-any-spelling.md:60-61, :82-83, crates/spindle-kernel/Cargo.toml `[dev-dependencies]`, .engineering/waves/wave-30.md:81-82

**What I read:** all 12 stories (the files, plus the diff of 4547ef74), the wave page, `review-result:wave30-par-r1`, `aep plan artifact validate` (valid), `waves` and `relations`. In the tree: every place that builds `Undeclared`, `affects.rs:234-244`, `adversary_w26_u4.rs:505-535`, the kernel's dev-dependencies, the `spindle-host` test modules, and the callers of `establish(` in tests.

Round-1 findings:
- **1 (U5 and U4 on `execute.rs`): resolved.** U5's Scope names `:826`, `:1837` and `affects.rs:240` (create-on-held-identity-unavailable.md:59-62). The wave page lists them (wave-30.md:61-66) and merges U5 after U4 for that reason (:89-91).
- **2 (U1 on `host.rs`): resolved** on the wave page (wave-30.md:74). U1's body still names only wave 29 there, not U6 or U7. The wave page is where the plan records shared files, so I count this as resolved.
- **3 (U8's test file): resolved.** Acceptance 2 names `connect.rs`, Scope adds it and `main.rs`, and the wave page lists `spindle-host (U1, U8)`.
- **4 (U3 "doc only"): resolved** (wave-30.md:61-62), and `values.rs:433` is now listed (:69).

Where each story lands: 11 cited, 1 inferred (U10, `load.rs`), 0 unplaced, apart from U6's establish test (the finding above). I traced 3 acceptance reads to a producer in the set, and `depends_on` edges record the 2 real dependencies:
- map-value-order reads nested-element-quantifier's output; the edge is there.
- error-payload reads unknown-instance's output; the edge is there.
- U5's establish test against U6: no dependency, because it uses a string identity.

**What I could not establish:**
- **U5's new replay acceptance** (`..._prints_unavailable`) names no file. Its natural home is the cited `adversary_w26_u4.rs:532`, which already checks `diverges, undeclared` for a second create, so I raise no finding.
- **Overlaps with wave 29** (`host.rs`, `supported.rs`, `values.rs`, `target.rs`, `cli.rs`) are kept in sequence: wave 30 forks after wave 29 merges (wave-30.md:7, :85-87).
- **Public repository:** no person, organisation, host or home path in the 12 bodies.
- **Outside my lane:** `crates/spindle-telemetry/tests/spindle-telemetry/catalogue.rs:186-189` builds `Undeclared` too, but U5's new Scope line on the other constructors leaves it out. Nothing else in the set touches that file, so this is for the scope critic.

```findings
- file: .engineering/planning/story/decimal-identity-any-spelling.md
  line: 60
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: the new acceptance an_established_decimal_identity_is_found_whatever_its_spelling calls Host::establish, but Scope names no test file for it; spindle-kernel has no spindle-host dev-dependency, so it lands in spindle-host or spindle-conformance tests, and as a new spindle-host module it would add a line to crates/spindle-host/tests/spindle-host/main.rs, which the wave page lists for U1 and U8 only"
```
