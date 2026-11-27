---
format: aep.planning-md/3
id: review-result:wave30-par-r3
kind: review-result
status: active
title: Plan critic (parallel-safety), round 3, on wave 30
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

**What I read:** all 12 stories (their files and the head commit 36e4e55a), the wave page, `review-result:wave30-par-r1` and `review-result:wave30-par-r2`, and the output of `aep plan artifact validate` (valid). In the tree I ran `git grep -n "Undeclared *{"` and read the `spindle-host` and `spindle-kernel` test module lists, `adversary_world_integers.rs`, `adversary_establish.rs:140-205` and `host.rs:1265-1282`.

Earlier findings:
- **Round 1, all 4 findings:** still resolved.
- **Round 2 (U6's `Host::establish` test had no file):** resolved. The test now goes in the existing module `crates/spindle-host/tests/spindle-host/adversary_world_integers.rs` (decimal-identity-any-spelling.md:83), so it adds no `main.rs` line. No other unit touches that module. `Host::establish` is public (`host.rs:1270`), so the test needs no new dev-dependency and stays off U8's `Cargo.toml`.
- **Round 2 out-of-lane note:** closed. U5's Scope now names `catalogue.rs:186-189`. With it, U5 names every place that builds `ExecuteError::Undeclared`: `execute.rs:826`, `:1837`, `:2108` and `:2126`, `affects.rs:240` and `catalogue.rs:186`. Every other place only matches the error with `..`.

Where each story lands: 11 cited, 1 inferred (U10, `load.rs`), 0 unplaced.

Acceptance reads: 3 traced to a producer in the set, of which 2 are real dependencies. `depends_on` edges record both:
- map-value-order on nested-element-quantifier;
- error-payload on unknown-instance;
- U5's establish test against U6: no dependency, because the test uses the string identity `parcel-1`.

Every file that two units share is listed on the wave page, and the merge order for `execute.rs` (U4, U3, U5, U6) is stated there.

**What I could not establish:**
- **U2 and U5 on one helper (inferred, optional):** U2 has an optional cleanup of `adversary_establish.rs:160`, the `list` helper that U5's un-ignored test calls. U2's frontmatter does not claim that file. The helper removes `null` on both sides, so U2's `query_view` change leaves U5's test as it is. If U2 does the cleanup, it lands in a separate hunk from U5's `:174-205`.
- **U5's replay acceptance** still names no file. Its likely home is `adversary_w26_u4.rs`, as in round 2.
- **Wave page over-lists `spindle-kernel` `main.rs` for U3.** U3's kernel tests go in modules that already exist (`error_fields.rs`, `generated.rs`, `mint.rs`). This is harmless.
- **Overlaps with wave 29** stay in sequence, because `wave/30` forks after wave 29 merges (wave-30.md:7).
- **Public repository:** the 12 bodies and the wave page name no person, organisation, host or home path.

```findings
[]
```
