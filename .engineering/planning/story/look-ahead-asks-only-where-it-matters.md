---
format: aep.planning-md/3
id: story:look-ahead-asks-only-where-it-matters
kind: story
status: draft
title: The kernel asks a decider only where the answer changes the outcome
relations:
- informed_by: review-result:w17-u2-adversary-2
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/deployment.rs
revision: 6
---
## Outcome

The kernel asks a decider for an external cause only where the answer changes the outcome. A request
that ESS's interpreter answers the same under every forcing gets that answer with no decision on hand.

## Why

Pre-existing, found by review-result:w17-u2-adversary-2 (finding 2) and present at 9421ca79. When
every related row is present, the look-ahead in `crates/spindle-kernel/src/execute.rs` (around
:1041) still asks the decider for an external branch declared before the held-state check. It does
so even where ESS answers a present-related refusal (`desk-closed`, `home-closed`) or `wrong-state`
from every branch. A decider with no answer then turns the model's refusal into
`ExecuteError::Undecided`. That breaks the kernel's contract that a request the model already turns
away never reaches the decider (`execute.rs:593`, `crates/spindle-kernel/src/lib.rs:18`).

Reach: the test fixtures `adversary-related` (`LendBookChecked`) and `adversary-look-ahead`. Nothing
in `ess/` or `examples/` has this shape.

## Acceptance

- The four cases in `crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs` run
  un-ignored and pass. They are `adversary_no_decider_is_asked_for_a_request_the_model_answers_alone`
  and its `return_quiet_`, `lend_two_` and `lend_approved_` copies.
- `decide_admission_not_provided_asks_no_decider` (`deployment.rs`): Spindle's own
  `DecideAdmission` on an `Offered` deployment whose environment does not provide a binding answers
  `not-provided` with a decider that cannot answer.
- Every case of `adversary_related_guards` and `adversary_look_ahead` still agrees with the
  interpreter under every forcing.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (src/execute.rs, look-ahead) — cited
- **Files:** `crates/spindle-kernel/src/execute.rs:1580-1712` (`look_ahead`, `select_ahead`, `ahead_answer`) — cited
- **Files:** `crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs:336-363` (remove four `#[ignore]`) — cited
- **Symbols:** `Run::look_ahead` (the `if !ask` gate at :1586), `Run::select_ahead`, `Run::ahead_answer`, `enum Ahead` (:430) — cited
- **Symbols:** call sites `Run::select` (:787-798) and `Run::stored_reference` (:1519-1526), both pass `ask = !missing` — cited
- **Symbols:** doc text of `look_ahead` (:1561-1579) and `missing_reference` (:1474-1478), which describe today's asking rule — inferred, the docs to rewrite
- **Change shape:** always run the no-ask walk first. Where every branch it could select (`passed` plus the unforced selection) gives the same look-ahead answer (all `None`, or all the same `WrongState`/`Unknown` result), answer it without asking. Ask only where they differ, or where a guard past an external branch is `Unknown` (keep the :1598-1600 fallback). The `ask` parameter then goes away — inferred
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/deployment.rs` — inferred, one own-spec regression case, `decide_admission_not_provided_asks_no_decider`: an `Offered` deployment, `missing` of 1 or more, a binding the environment does not provide, a decider that cannot answer; expects `not-provided`
- **Tests:** the four un-ignored cases plus the one above; the module ran in 0.31 s in the review (8 cases), each well under 60 s; the kernel test step stays under 120 s — inferred
- **Documents:** none
- **Confidence:** high — the story names the functions and the tests, and the tree has them at the lines above
- **Would collide with:** any unit editing `crates/spindle-kernel/src/execute.rs`, above all `Run::unknown_instance`/`not_found` (:1782-1810, called from `look_ahead`) and `missing_reference`/`reference` (:1474-1513)
- **Safety fact:** every branch the kernel could select under any forcing is in `passed` plus the unforced selection (`select_ahead` :1636-1680), so where all of them answer alike, asking changes nothing; the change only removes decider questions, never adds one, and the existing tests that pin asks expect none (`deployment.rs:147`, `:240`, `adversary_admission.rs:169`, `:353`) — step 3, unproven
- **Safety fact:** own-spec reach: `DecideAdmission` (`ess/domains/deployment.yaml:232`) reads `environment_id` through a stored reference and declares `refused` (external) after `not-provided`/`missing-provided`; the current look-ahead asks `refused` before those refusals are read, so the host decider (`spindle-host/src/decisions.rs:234`) errors on a request ESS answers `not-provided` — step 3, unproven
