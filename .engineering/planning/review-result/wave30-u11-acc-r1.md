---
format: aep.planning-md/3
id: review-result:wave30-u11-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
needs-revision
story:affects-over-input-list — the Acceptance has no test for a `deletes:` entry over an input list, though the Outcome and D2 serve one ("moves and deletes of held rows only"), so every named test passes with deletes never built — .engineering/planning/story/affects-over-input-list.md:109
story:affects-over-input-list — the test `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http` names no fixture that serves `swept` over HTTP: the only fixture with `swept` (`outcome-scoped-refusal-effects`) is `reached_by: in_process`, and `run` serves HTTP only for a `reached_by: network` component, so the HTTP half cannot be written as stated — .engineering/planning/story/affects-over-input-list.md:70
story:affects-over-input-list — in the same bullet, "each act on the rows" names no row state the test reads (for example tag `t1` resting in `Swept`), so a test that checks only the answered outcome name passes it — .engineering/planning/story/affects-over-input-list.md:70

**What I read:** the 1 story I was given (`aep plan artifact show story:affects-over-input-list`) and the U11 parts of `.engineering/waves/wave-30.md`: the units table, Surfaces and merge order. I checked them against the tree: `supported.rs:602-636`, `execute/affects.rs:170-240`, `outcome_scoped_refusal.rs:1-75,150-210,395-610`, the fixture files of `outcome-scoped-refusal-effects`, and the `run`/`admit` docs in `cli.rs`. I also read the pinned ESS source at commit d7833ee: `set_effects.rs` (`apply`, `carry_out`, `elements`, `ElementFacts`) and `ir.rs` `ResolvedEach`, including `repeated`.

**What I could not establish:**
- Whether ESS's interpreter treats the `Decimal` key `1.0` as the same row as `1`. The parity bullet holds either way, because it compares against the interpreter.
- Out of my lane (scope): the story's status is `draft`, while the wave's other stories were moved to `active`.
- Out of my lane (scope): no acceptance line covers the change to `website/docs/concepts/specification.md:124`, which names `each:` as refused.
- Out of my lane, minor: the Scope cites the `SWEPT` constant at `:522`; it sits at `:524`.
- What I confirmed: the claimed ESS behaviour matches the code (create in `initial`, move or remove only held rows, skip the subject, element filter, ess/35 `repeated`). A construct the kernel still refuses and ESS accepts exists for D1: `compensates: true`, used by the fixture `adversary-outcome-scoped-refusal-compensates`. The story body names no person, organisation, host or home path.

```findings
- file: .engineering/planning/story/affects-over-input-list.md
  line: 109
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the Acceptance has no test for a `deletes:` entry over an input list, though the Outcome and D2 serve one (\"moves and deletes of held rows only\"), so every named test passes with deletes never built"
- file: .engineering/planning/story/affects-over-input-list.md
  line: 70
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the test `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http` names no fixture that serves `swept` over HTTP: the only fixture with `swept` (`outcome-scoped-refusal-effects`) is `reached_by: in_process`, and `run` serves HTTP only for a `reached_by: network` component, so the HTTP half cannot be written as stated"
- file: .engineering/planning/story/affects-over-input-list.md
  line: 70
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "in the same bullet, \"each act on the rows\" names no row state the test reads (for example tag `t1` resting in `Swept`), so a test that checks only the answered outcome name passes it"
```
