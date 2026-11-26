---
format: aep.planning-md/3
id: story:unknown-instance-after-exists-false-guard
kind: story
status: archived
title: 'A declared exists: false guard answers before the unknown-instance refusal'
relations:
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "archived", at: "2026-11-26T18:41:12Z", actor: "human:operator", revision: 3}
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

## Not a gap

Archived 2026-11-26 when scoped: not a gap. ESS 0.99.1 answers the addressed record's existence
before every `exists: false` guard over a row the input names (ess-domain
`command/precedence.rs:104-113`, `:483-487`; ess-conformance `interpret/execute.rs:741-756`,
`:796-839`), so the reproduction answers `undeclared` in ESS as in Spindle
(`crates/spindle-kernel/src/execute.rs:637-700`). The order quoted in the Outcome comes from an ESS
validation hint (`command/related_guard.rs:782-785`) that ESS records as stated the wrong way round.
`crates/spindle-kernel/tests/spindle-kernel/adversary_related_guards.rs:304` already pins parity for
this case. A specification that wants the refusal declares `unknown_instance:` with that error.
