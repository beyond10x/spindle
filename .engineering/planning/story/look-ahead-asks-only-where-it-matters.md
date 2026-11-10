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
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs
revision: 2
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
- Every case of `adversary_related_guards` and `adversary_look_ahead` still agrees with the
  interpreter under every forcing.

## Scope

- `crates/spindle-kernel/src/execute.rs` (`select_ahead`, `ahead_answer`, the look-ahead in
  `select` and `stored_reference`)
- `crates/spindle-kernel/tests/spindle-kernel/adversary_look_ahead.rs`
