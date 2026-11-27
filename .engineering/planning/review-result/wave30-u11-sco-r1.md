---
format: aep.planning-md/3
id: review-result:wave30-u11-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
needs-revision

story:affects-over-input-list — The Outcome says "a `moves:` or `deletes:` entry acts only on a held row", and D2 A takes "moves and deletes of held rows only", but no Acceptance line covers `deletes:`. The `moves:` test and the "these cases" interpreter-parity test both leave it out, so the delete half of the promise has quietly dropped out of the checks. Fix: add a deletes test such as `an_affects_entry_over_an_input_list_deletes_only_held_rows`, or name `deletes:` among the parity cases. — .engineering/planning/story/affects-over-input-list.md:44 (promise), :62-67 (Acceptance, no deletes)

What I read: 1 story in full (`aep plan artifact show story:affects-over-input-list`, and the file .engineering/planning/story/affects-over-input-list.md). Also the U11 parts of .engineering/waves/wave-30.md (Selection, Units, Surfaces, merge order) and `aep plan artifact graph`. I grepped .engineering/planning/story/ for other claims on `each:`, `affects.rs` and the refusal fixture, and found none in wave 30 or wave 29. Other files read: `supported.rs:600-636`, the fixture `lot.yaml` for `swept`, `website/docs/concepts/specification.md:118-130`, and the ESS interpreter `set_effects.rs` `elements` at checkout 8b2da1d. Promises taken from the parent (Consumer gap :79-81 plus Outcome :39-52): 13. Traced to an Acceptance or Scope line: 12. The one left is `deletes:` on held rows only. Nothing in the story reaches beyond the parent. D1 moves an existing refusal test so it keeps running, and the docs change follows from the refusal being removed. No other artifact claims the same outcome.

What I could not establish:
- Whether, after waves 29 and 30, any construct is left that the kernel refuses and ESS accepts, which the moved refusal test (D1, :72-75) needs. This is for the acceptance or design critic.
- In the ESS interpreter, a `moves:` element whose held row sits outside the move's `from` states is skipped (`set_effects.rs` `elements`, NotFromHere arm). The Outcome does not say this; only the parity test covers it. This is for the acceptance critic, and it does not affect my verdict.
- The existing refusal of an `affects:` entry over another component's entity (story:cross-component-related-reads) has to keep applying on the new `each:` path. U11 does not mention it. This is for the design critic.
- U11's possible doc-comment change in `execute.rs` (Scope, "Also likely") is missing from the wave page's `execute.rs` Surfaces bullet (.engineering/waves/wave-30.md:64-69). This is for the parallel-safety critic.

```findings
- file: .engineering/planning/story/affects-over-input-list.md
  line: 44
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome promises \"a `moves:` or `deletes:` entry acts only on a held row\" and D2 A takes \"moves and deletes of held rows only\", but no Acceptance line claims the `deletes:` case (the moves test and the parity test's \"these cases\" both omit it), so half of that promise is silently narrowed; add a deletes test or name `deletes:` among the parity cases"
```
