---
format: aep.planning-md/3
id: review-result:wave30-u11-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
approve

All four round-1 findings are fixed in the head commit 32764572, and the revision adds no new collision.

**Round-1 findings:**
1. **U5 shares `execute/affects.rs`:** fixed. The story's "Would collide with" line now names U5 and says U11 merges after it (`affects-over-input-list.md:115`). The wave page says the same (`wave-30.md:73-75` and `:99`). In the tree, `undecided` at `affects.rs:239-243` is the only constructor of `ExecuteError::Undeclared`. U11's run-time refusal at `:189-197` is a `ExecuteError::Unsupported`, so U5's one-line change sits outside U11's `select_affected` code.
2. **`specification.md` shared with U1:** fixed. Both the story (`:115`) and the wave page Surfaces (`wave-30.md:79`) name it. U11 edits `:124-125` and U1 edits `:168-173`; I checked both ranges in the tree.
3. **The doc comment at `execute.rs:87`:** fixed. Both the story (`:115`) and the wave page entry for `execute.rs` (`wave-30.md:68-69`) list it. In the tree it is the doc comment of the `execute` function, far from the U3, U4, U5 and U6 code.
4. **U11 reading U6's spelling while running beside it:** fixed. The acceptance (`:72-74`) and the wave page (`wave-30.md:101-102`) both say the coordinator merges `wave/30` holding U5 and U6 into U11's branch before U11's last run, and that U11 reports green only on that tree. The `depends_on` edge to U6 is there.

**New in this revision:**
- **New fixture `affects-each-network`:** no file of that name exists, and no other wave-30 or wave-29 unit creates one.
- **HTTP case reading `/api/history`:** that endpoint is served from `spindle-ingress` `console.rs`/`router.rs`. No wave-30 story changes them; U7 says `console.rs` stays unchanged.
- **Moved refusal test using `compensates: true`:** the kernel refuses it at `supported.rs:590-591`. No other wave-30 story mentions `compensates`, so nothing lifts that refusal while U11 relies on it.
- **U9 changing the network branch of `run`:** U9 keeps a network spec serving HTTP when `--stdin` is not given, and U11 merges before U9.
- **Wave 29:** its kernel changes are limited to `supported::value` and `values.rs`. None of its stories touches `affects.rs`, `facts.rs`, `specification.md` or `outcome_scoped_refusal.rs`.
- **Personal details:** none of the persons, organisations, hosts or home paths the brief lists appear in U11's body.

**What I read:** U11 in full, U5 in full, the other eleven stories by grep for U11's surfaces, wave-30.md, the wave 29 page and its stories, and `review-result:wave30-u11-par-r1`. Commands: `aep plan artifact show`, `aep plan artifact graph`, `git show HEAD`, `git diff HEAD~1 HEAD`, `git show plan/wave-29:…`, `git grep`/`sed` on `affects.rs`, `execute.rs:80-92`, `supported.rs:587-636`, `specification.md`, `outcome_scoped_refusal.rs:519-572` and `secondary_effects.rs:392`.
- **Surfaces:** 12 of the 13 stories cited, 1 inferred only (story:related-via-nested-input, as in round 1), 0 not placed. U11 itself has 6 paths cited and 6 inferred.
- **Acceptance reads:** 1 traced to a producer in the set (U11 reads U6's spelling), and a `depends_on` edge records it. The ESS 0.99.2 pin comes from wave 29, outside the set, and merges first.

**What I could not establish:**
- Where `http_serves_an_affects_entry_over_an_input_list` lands. Scope `:112` still names the old combined test, `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http`, in `outcome_scoped_refusal.rs`. If the HTTP test goes into a new module instead, it adds a line to the `spindle-cli` test `main.rs`, which U9 also edits (inferred). That stale name belongs to the scope and acceptance lanes, not mine.
- U11 has no `depends_on` edge to U5. Its acceptance reads no state U5 writes, so the merge order written in the story and on the wave page is enough for my lane.

```findings
[]
```
