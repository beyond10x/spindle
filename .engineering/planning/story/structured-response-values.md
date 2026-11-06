---
format: aep.planning-md/3
id: story:structured-response-values
kind: story
status: active
title: Response fields of every type ESS's interpreter chooses
relations:
- serves: vision:spindle
- depends_on: story:kernel-command-responses
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/mint.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T10:10:59Z", actor: "agent:b10x-chef-orchestrator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-06T10:10:59Z", actor: "agent:b10x-chef-orchestrator", revision: 4}
---
# Response fields of every type ESS's interpreter chooses

## Outcome

A returning outcome whose response declares a field of a record (struct), union, map, enum-free
list or constrained newtype answers it with a value of that type, as ESS's interpreter chooses one
(ESS 0.87.0 `crates/verify/ess-conformance/src/interpret/response.rs`, `base`: a struct's members
each by the same rules, a union's first variant, one map entry, one list element, a newtype's
invariant literal where one satisfies it). Today `crates/spindle-kernel/src/supported.rs` refuses
such a field by name ("which the runtime does not choose"), so the command answers 501 over HTTP.

## Acceptance

- `a_record_response_field_answers_a_record` — a neutral fixture whose response holds a record of
  `Optional<Integer>` members: the answer passes ESS's `direct_response::Observation::compare`.
- `union_map_and_list_response_fields_answer_their_type` — one fixture per kind.
- `a_constrained_newtype_response_field_satisfies_its_invariants`.
- A type ESS's interpreter does not choose either (an enum field, per `base`) is still refused by
  name before the command runs.

## Origin

A consumer serving a specification as a UI test backend (2026-11-06, on main 62b97a9): a settings
update returns a record-typed `item`, answered 501. Follows `story:kernel-command-responses`.
