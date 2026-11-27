---
format: aep.planning-md/3
id: review-result:wave30-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 30
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

story:create-on-held-identity-unavailable — D1 A adds a field to `ExecuteError::Undeclared`, so every place that builds that error has to change. That includes `execute.rs:826` at the end of `Run::select`, which U4 rewrites at `:787-798` when it drops the `ask` parameter, and `execute.rs:1837` in `Run::wrong_state`, next to `:1782-1810`, the part U4 says it would collide on. Neither this Scope nor the execute.rs line in the wave page's Surfaces section names these places: the Scope gives U5 only `:2076-2136`, and the Surfaces line gives U4 only `:1580-1712`. One of two fixes, not chosen here: an ordering edge from U5 to U4 on `execute.rs` with these lines as the reason, or keeping those two places unchanged (for example, building the error through a helper) (cited, `git grep -n "Undeclared *{"`) — .engineering/planning/story/create-on-held-identity-unavailable.md:54, .engineering/waves/wave-30.md:61-63

story:view-filter-nested-element-quantifier — its Scope has U1 editing `crates/spindle-host/src/host.rs:1974-1977`, the doc comment of the host's read method. U6 and U7 edit the same file, but the wave page's `host.rs` line lists only U6 and U7, and this body names neither of them (inferred) — .engineering/planning/story/view-filter-nested-element-quantifier.md:56, .engineering/waves/wave-30.md:70-71

story:peer-connect-test-steady — acceptance 2 (`a_connection_that_reached_itself_is_no_link`) names no test file. The story adds a `spindle-host` dev-dependency for it, so the test lands somewhere in `spindle-host`, which is not placed. If it is a new module, it adds a line to `crates/spindle-host/tests/spindle-host/main.rs`, as U1's `view_filter_map.rs` does, and the wave page's list of test `main.rs` files leaves out `spindle-host` (inferred) — .engineering/planning/story/peer-connect-test-steady.md:41,57, .engineering/planning/story/view-filter-map-value-order.md:79-80, .engineering/waves/wave-30.md:76-77

story:error-payload-generated-fields — the wave page says U3's change to `execute.rs` is "doc only". Under D1 A and D2 A this story changes code in `declared_error`, `error_of` and the `Unsourced::Minted` arm of `unsourced_error_fields` (`execute.rs:2382-2400`). Whoever merges the `execute.rs` units would judge U3's hunks wrong (cited) — .engineering/planning/story/error-payload-generated-fields.md:50,70, .engineering/waves/wave-30.md:61

**What I read:** all 12 stories (`aep plan artifact show`), the wave page, `aep plan artifact waves`, `relations` and `validate` (the store is valid), and the tree:
- every place that builds `Undeclared`, plus `execute.rs:780-835`, `:1775-1845` and `:2375-2400`;
- `adversary_w26_u4.rs:480-600`, `notice/sink.yaml` and `adversary_establish.rs:170-205`;
- every use of `Answered`, and `host.rs:1965-1980`;
- wave 29's Surfaces section (`git show plan/wave-29:.engineering/waves/wave-30.md` is not it; I read `plan/wave-29:.engineering/waves/wave-29.md`).

Where each story lands: 11 cited, 1 inferred (U10, `spindle-spec/tests/spindle-spec/load.rs`), 0 unplaced, except U8's acceptance-2 test file (third finding). I traced 3 acceptance reads to a producer in the set, and 2 `depends_on` edges record the 2 real dependencies:
- map-value-order on nested-element-quantifier, edge present;
- error-payload on unknown-instance, edge present;
- U5's establish test on U6's `Host::establish`: no dependency, because the test uses a string identity `parcel-1` and U6 changes only number identities.

**What I could not establish:**
- **Overlaps with wave 29.** Wave 29's page shows more than the brief listed. Its U1 owns all of `crates/spindle-host`, `spindle-cli/src/cli.rs`, `website/docs/reference/cli.md` and `spindle-conformance/src/target.rs`. So wave 30's U1, U2, U5, U6, U7, U8 and U9 overlap wave 29 too, and so do U3's `supported.rs` and `values.rs:433`. U9 also calls `lines::Bindings::new`, and wave 29 changes `lines.rs`. All of these are kept in sequence by `wave-30.md:7`, which forks `wave/30` after wave 29 merges, so none is a finding. The line numbers these stories cite in those files were taken before wave 29 and may move.
- **U3's fixture and U5's test.** U3 changes `notice/sink.yaml` and U5 changes `adversary_w26_u4.rs:532`. They meet only through that file, in different tests (`:494` and `:540`), so they do not collide.
- **Public repository.** No person, organisation, host or home path in the 12 bodies ("workstation" in the peer story names no host).
- **Outside my lane:** U4's hunk list on the wave page also leaves out `:430`, `:1474-1478` and `:1519-1526`. Nothing else edits those lines, so this is a design or scope matter.

```findings
- file: .engineering/planning/story/create-on-held-identity-unavailable.md
  line: 54
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "D1 A adds a field to ExecuteError::Undeclared, so every place that builds that error has to change, including execute.rs:826 at the end of Run::select (which U4 rewrites at :787-798) and execute.rs:1837 in Run::wrong_state (next to U4's named collision zone :1782-1810); neither this Scope nor the wave page's Surfaces section names these places; fix with an ordering edge to U4 recording the shared lines, or by keeping those two places unchanged"
- file: .engineering/planning/story/view-filter-nested-element-quantifier.md
  line: 56
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: U1 edits crates/spindle-host/src/host.rs:1974-1977, which U6 and U7 also edit, but the wave page's host.rs line lists only U6 and U7 and this body names neither"
- file: .engineering/planning/story/peer-connect-test-steady.md
  line: 41
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "inferred surface: acceptance 2 names no test file although the story adds a spindle-host dev-dependency for it; as a new module it would share crates/spindle-host/tests/spindle-host/main.rs with U1's view_filter_map.rs, and the wave page's list of test main.rs files leaves out spindle-host"
- file: .engineering/planning/story/error-payload-generated-fields.md
  line: 50
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the wave page says U3's execute.rs change is doc only, but under D1 A and D2 A this story changes code in declared_error, error_of and the Unsourced::Minted arm of unsourced_error_fields (execute.rs:2382-2400), so whoever merges the execute.rs units would judge U3's hunks wrong"
```
