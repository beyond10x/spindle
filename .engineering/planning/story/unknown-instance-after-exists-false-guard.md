---
format: aep.planning-md/3
id: story:unknown-instance-after-exists-false-guard
kind: story
status: draft
title: 'A declared exists: false guard answers before the unknown-instance refusal'
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A command whose outcome updates the instance an input names, and which declares an earlier outcome
guarded by `when_related: {via: input.<id>, exists: false}`, answers that declared outcome when no
row has the identity. Today Spindle looks up the update's subject before it evaluates the guard and
answers `undeclared: … no <entity> has the identity …, and the command declares neither
unknown_instance: nor wrong_state:`. ESS compiles such a specification without a warning, and from
`ess/27` an unknown addressed identity answers after an `exists: false` over the input's row.

## Acceptance

- `an_exists_false_guard_answers_before_the_unknown_instance_refusal`: a `Debit(account_id)` with
  outcomes `no-such-account` (`when_related: {via: input.account_id, exists: false}`) and `debited`
  (`updates: Account, instance: account_id`), sent with an identity no row has, answers
  `no-such-account` and its declared error; nothing is written. The answer equals
  `ess_conformance::interpret` on the same specification and input.
- `a_known_identity_still_takes_the_update`: the same command with a known identity answers
  `debited`.
- `an_unknown_identity_without_a_guard_is_still_refused`: with no `exists: false` guard and no
  `unknown_instance:`, the answer stays `undeclared`, as today.

## Not covered

- `unknown_instance:` and `wrong_state:` outcomes: unchanged.

## Origin

A consumer's simulation, 2026-11-26, on 0.21.0, with a standalone reproduction.
