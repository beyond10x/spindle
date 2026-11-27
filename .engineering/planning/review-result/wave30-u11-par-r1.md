---
format: aep.planning-md/3
id: review-result:wave30-u11-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
needs-revision

story:affects-over-input-list — the "Would collide with" line says `execute/affects.rs` is shared with no other wave-30 unit. U5 (story:create-on-held-identity-unavailable) cites `affects.rs:240` and adds a field to `ExecuteError::Undeclared`, and every `Undeclared` constructor in U11's rewritten `Run::affect` will have to set that field. The line should name U5 and say that U11 merges after it (cited, both bodies; the wave page lists the pair at `wave-30.md:72-73`) — .engineering/planning/story/affects-over-input-list.md:99, .engineering/planning/story/create-on-held-identity-unavailable.md:63

story:affects-over-input-list — this story and story:view-filter-nested-element-quantifier (U1) both edit `website/docs/concepts/specification.md`: U11 at `:124-125` (cited), U1 at `:168-173` (inferred). U11's "Would collide with" line does not name the file, and neither does the wave page's Surfaces section — .engineering/planning/story/affects-over-input-list.md:97, .engineering/planning/story/view-filter-nested-element-quantifier.md:58, .engineering/waves/wave-30.md:59-88

story:affects-over-input-list — U11's inferred doc comment at `crates/spindle-kernel/src/execute.rs:87` is missing from two places: its own "Would collide with" line, and the Surfaces entry for `execute.rs`, which U3, U4, U5 and U6 share. No other unit's hunk is near `:87` (inferred) — .engineering/planning/story/affects-over-input-list.md:94, .engineering/waves/wave-30.md:64-69

story:affects-over-input-list — the acceptance test `affects_over_an_input_list_agrees_with_the_interpreter` uses U6's single spelling of an identity (element `1.0` against a row held as `1`). The `depends_on` edge exists, but U11 is a separate unit that runs alongside U6, and both the story and the wave page only say it "merges after" U6. Neither says that U11 runs that test on a tree that already contains U6 before it reports green, as wave 29 says of its pin unit. Fix: say so, or move U11 into U6's unit — .engineering/planning/story/affects-over-input-list.md:66, :112, .engineering/waves/wave-30.md:52, :96-97, `git show plan/wave-29:.engineering/waves/wave-29.md` line 69

What I read: 13 stories (U11 in full, the other 12 at scope header and where they touch U11's files) and the wave pages for 30 and 29. Commands: `aep plan artifact show` (13 stories), `aep plan artifact waves`, `git show plan/wave-29:` (the wave page and 4 stories), `git grep`/`grep`/`sed` on `affects.rs`, `supported.rs`, `execute.rs`, `specification.md`, the CLI fixture `lot.yaml` and `outcome_scoped_refusal.rs`.
- Surfaces: 12 stories cited, 1 inferred only (story:related-via-nested-input), 0 not placed. U11 itself: 6 paths cited, 5 inferred.
- Acceptance reads: 1 traced to a producer in the set (U11 reads U6's spelling), and a `depends_on` edge records it. U11's interpreter parity reads the ESS 0.99.2 pin from wave 29, which is outside the set and comes first.

What I could not establish:
- Whether U11's rewrite of `select_affected` reaches lines 234-236. U5's change at `:240-243` is only a few lines away; it should still merge cleanly as long as at least one unchanged line separates the two changes.
- Whether U11 adds new direct `ExecuteError::Undeclared` constructors or keeps going through `undecided` (the body does not say).
- Outside my lane (acceptance): the story does not name the construct the moved stdin refusal test will use ("a construct the kernel still refuses and ESS accepts").
- Wave 29 changes `supported::value`, not `supported::affects`, and none of its stories touches `affects.rs`, `facts.rs` or `specification.md`. No collision with wave 29.

```findings
- file: .engineering/planning/story/affects-over-input-list.md
  line: 99
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Would collide with line says crates/spindle-kernel/src/execute/affects.rs is named by no other wave-30 surface, but story:create-on-held-identity-unavailable (U5) cites affects.rs:240 and adds a field to ExecuteError::Undeclared that every Undeclared constructor in U11's rewritten Run::affect must set, so the line should name U5 and the merge after it"
- file: .engineering/planning/story/affects-over-input-list.md
  line: 97
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "this story (cited, :124-125) and story:view-filter-nested-element-quantifier (U1, inferred, :168-173) both edit website/docs/concepts/specification.md, and neither U11's Would collide with line nor the wave page's Surfaces names the shared file"
- file: .engineering/planning/story/affects-over-input-list.md
  line: 94
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "U11's inferred doc comment at crates/spindle-kernel/src/execute.rs:87 is absent from its Would collide with line and from the wave page's Surfaces entry for execute.rs shared by U3, U4, U5 and U6 (surface inferred)"
- file: .engineering/planning/story/affects-over-input-list.md
  line: 66
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance affects_over_an_input_list_agrees_with_the_interpreter reads story:decimal-identity-any-spelling's one spelling (1.0 against a row held as 1) and the depends_on edge exists, but U11 runs as a concurrent unit and neither the story nor the wave page says it checks that case on a tree holding U6 before reporting green (only that it merges after U6), as wave 29 states for its pin unit; say so, or put U11 in U6's unit"
```
