---
format: aep.planning-md/3
id: review-result:wave29-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 29 U2
relations:
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
revision: 1
---
```
unit: story:payload-field-through-conversion, story:value-source-through-two-rows; findings cover unit/w29-values at 9eecee45 (the code under attack is unchanged from ac56456f)
verdict: CONFIRMED
cases: executed 71→81 (spindle-conformance integration binary), red 6
origin: introduced 0 / pre-existing 3 / undecided 0
wrote-outside-worktree: 1 directory, adv-scratch/ (listed in part 6)
needs-coordinator: none
```

The unit's own code is correct in every case I could build. All 6 red cases come from Spindle code outside this unit's diff. Each one has a one-hop counterpart that is red the same way when my module runs against a copy of the base commit (97dedd3b).

**1. Diff stat** (`git --no-pager diff --stat ac56456f..HEAD`). Every path is a test file or a test fixture:
```
 .../fixtures/adversary_w29_u2/domains/plant.yaml   | 260 +++++++++++++
 .../fixtures/adversary_w29_u2/domains/yard.yaml    |  43 +++
 .../tests/fixtures/adversary_w29_u2/system.yaml    |  49 +++
 .../tests/spindle-conformance/adversary_w29_u2.rs  | 409 +++++++++++++++++++++
 .../tests/spindle-conformance/main.rs              |   1 +
```
The commit is `9eecee45` on the unit branch, made through `b10x-gates bot`. The scan for private names found nothing.

**2. Cases added**
They are in `<local path>`. The fixture `tests/fixtures/adversary_w29_u2` is the unit's related-chain model with two of the unit's workarounds undone: no `presence:` on the instance fields, and the tag bounded with `value.count <= 8` instead of `>= 4`. It adds `PinInstance`, which reads the identity of the row two hops away, and `NoteInstance`, a one-hop read through an `Optional` input.

Run alone with `cargo test --locked -p spindle-conformance --test spindle-conformance adversary_w29_u2`: 4 passed, 6 failed, 0.05 s. Every case is under 1 s.

- **Red: the view row after an absent `Optional` reference** (`a_chain_through_an_absent_optional_reference_leaves_the_view_row_as_ess_does`, :260). The host leaves the fields out; ESS's interpreter answers them as `null`:
  `left: ... rows: {"forge.plant.Instances": [Object {"instance_id": String("ins-1")}]}`
  `right: ... [Object {"formation_id": Null, "formation_state": Null, "instance_id": String("ins-1"), "slots": Null}]`
- **Red: the synthesised suite for chained reads** (`every_synthesised_scenario_of_a_chained_read_runs_on_the_host_as_on_the_interpreter`, :229):
  `forge.plant.CreateInstance/outcome/created: host Failed, interpreter Some(Passed)`, expected `"forge.plant.Instances holds a row where formation_state = null, ..., slots = null"`.
- **Red: a missing far row** (`a_chain_whose_far_row_is_absent_is_answered_as_ess_answers_it`, :322). Both `PinInstance` and `BadgeWorkload` get the wrong kind of refusal:
  `left: ... Refused("unavailable"), Refused("unavailable")` / `right: ... Refused("unsupported"), Refused("unsupported")`.
- **Red, one-hop counterparts (paths the diff does not change):**
  - `a_one_hop_read_through_an_absent_optional_input_leaves_the_view_row_as_ess_does` (:280) shows the same `null` difference.
  - `every_synthesised_scenario_of_a_one_hop_optional_read_runs_on_the_host_as_on_the_interpreter`: `NoteInstance/outcome/noted: host Failed, interpreter Some(Passed)`.
  - `a_one_hop_read_of_an_absent_row_is_answered_as_ess_answers_it` (:341): `Refused("unavailable")` against `Refused("unsupported")`.
- **Green, kept.** On the base, all four were refused as `unsupported`, so they hold the unit's behaviour:
  - `a_chain_read_of_the_far_row_s_identity_is_the_far_address`: the instance holds `frm-1`, not the input address or the other reference. This catches a mutant in the identity arm at `values.rs` `Sources::related`, which none of the unit's tests reach.
  - `a_chained_code_over_the_tag_s_upper_bound_is_refused_as_ess_refuses_it`
  - `an_input_through_a_conversion_its_target_refuses_is_answered_as_ess_answers_it`
  - `every_synthesised_scenario_of_the_conversion_fixture_runs_as_on_the_interpreter`

**3. Suite run** (after the cases existed)
- `cargo test --locked -p spindle-conformance --no-fail-fast` exited 101: `test result: FAILED. 75 passed; 6 failed; 1 ignored; ... finished in 8.30s`. The 6 failures are exactly the cases above.
- Your implementor's run before mine: `ok. 71 passed; 0 failed; 1 ignored`.
- `cargo clippy --locked -p spindle-conformance --all-targets -- -D warnings` exited 0.
- `cargo fmt -p spindle-conformance -- --check` exited 0.
- To get the origin, I ran the same module on a `git archive` copy of 97dedd3b in my scratch directory, exit 101. The one-hop cases fail the same way there, so these defects are pre-existing.

**4. Findings**

| # | file:line | verdict | origin | what was measured / what reaches it |
|---|---|---|---|---|
| 1 | crates/spindle-host/src/view.rs:1325 | CONFIRMED | pre-existing | A view field the row does not hold is left out unless it declares `presence: null_when_absent`, where ESS's interpreter answers `null`. So ESS's synthesised creation scenario fails on the host through a chain (:229) and through one hop (NoteInstance). **What reaches it:** a consumer's model without `presence:` that creates a row through an absent `Optional` reference or input. Your implementor's point (c): the `presence:` lines in the unit's fixture hide this. |
| 2 | crates/spindle-conformance/src/target.rs:384 | CONFIRMED | pre-existing | When the kernel finds no value (`no_value`), the host answers `unavailable`; ESS answers `unsupported`, because it counts that as a capability gap. This reproduces through one hop on the base. **What reaches it:** a request naming a row nobody holds, through `run` or `conform`. The kernel-level comparison (point (d)) is correct; the difference is only at the host target. |
| 3 | crates/spindle-kernel/tests/spindle-kernel/related_chain_values.rs:241 | CONFIRMED | pre-existing | The unit's comparison with ESS drops `null` fields from ESS's view rows, so it cannot see finding 1. **Line 241 is introduced by this unit**; it is tagged pre-existing only because the defect it hides is. |
| 4 | crates/spindle-kernel/tests/fixtures/related-chain/domains/plant.yaml | INFEASIBLE | n/a (ESS) | With `value.count <= 8`, ESS's interpreter itself errors on its own synthesised `BadgeWorkload/outcome/issued` scenario, and the host matches it. This explains your implementor's `>= 4` workaround (point (c)); the fault is on the ESS side, not Spindle's. |

On your implementor's other points, checked against ESS 0.99.1 (`determined.rs`):
- **(a)** ESS's interpreter (`checked_read`) treats a required field that is absent on a read without a conversion as a bad request; the kernel answers it as absent. I found no way to reach that case: the compiler wraps a read through an `Optional` hop as `Optional`, and the store always holds required fields. Not a finding.
- **(b)** Not reachable. At the pin, ESS refuses `{related: …}` inside `affects:` sets when the model is validated ("a source reading one row or the caller is not admitted over a set of rows in this cut", `set_effects.rs:1856`).
- **(d)** Matching on the text "to the implementation" is right: it is the wording ESS prints for a value it cannot determine (`execute.rs:355`).

**5. Attacked, could not break**
- A chain of three rows: ESS refuses it when the model is validated ("names exactly two references").
- A row owned by another component, at the first row or the last: `foreign_read` checks each one.
- A conversion on the far field, with a value inside and outside the target's bound.
- An input through a conversion whose target refuses its value: the host matches ESS.
- The `{response: …}` conversion refusal, including inside a struct (`value()` recurses into struct members).
- The refusal text changed, but nothing in the repository reads it.
- Not tested: the arm in `checked` where an absent value is allowed because the source type is `Optional`. It needs a conversion declared between two `Optional` types.

**6. Paths written outside the worktree**
- `<local path>`, which holds:
  - `base/` (a 42 MB copy of 97dedd3b plus my test files)
  - `red1.log`, `red2.log`, `suite.log`, `clippy.log`, `base-run.log`
  - `head.rs`, `middle.rs`, `tail.rs`, `commit.txt`
- One shell wait I started on `base-run.log` may still be running in the background; it ends by itself within 10 minutes.

**7. Findings block**
```findings
- file: crates/spindle-host/src/view.rs
  line: 1325
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: a view field the row does not hold is left out unless it declares null_when_absent while ESS's interpreter answers null, so the synthesised chained (and one-hop) creation through an absent Optional reference fails conform on the host and passes on the interpreter
- file: crates/spindle-conformance/src/target.rs
  line: 384
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a kernel no_value is answered unavailable by the host target where ESS's interpreter answers unsupported, for a chained and a one-hop read of a row none carries
- file: crates/spindle-kernel/tests/spindle-kernel/related_chain_values.rs
  line: 241
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: the unit's interpreter comparison drops null fields from ESS's view rows, which hides the null-versus-absent difference of the first finding
- file: crates/spindle-kernel/tests/fixtures/related-chain/domains/plant.yaml
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: undecided
  message: with an upper-bounded conversion target read through a chain, ESS's interpreter errors on its own synthesised BadgeWorkload scenario, which is why the fixture uses value.count >= 4; an ESS-side cause, not Spindle's
```
