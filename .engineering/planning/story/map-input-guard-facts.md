---
format: aep.planning-md/3
id: story:map-input-guard-facts
kind: story
status: draft
title: A guard over a present Map input is decided as ESS decides it
relations:
- serves: vision:spindle
- informed_by: story:guard-over-absent-optional-input
revision: 1
---
# A guard over a present Map input is decided as ESS decides it

## Outcome

A guard that reads a present `Map` input with `.count` or a quantifier (`exists`, `all`) selects
the outcome ESS 0.99.1's interpreter selects. Today `Facts::bind`
(`crates/spindle-kernel/src/facts.rs:160`) binds a Map's members by key, with no `<path>.count` and
no `<path>.<index>` facts; ESS binds a map like a list, its count plus its values in key order
(`ess-conformance/src/input.rs:1816-1818`). The kernel refuses `undecidable` and the served JSON
route answers 501 (`crates/spindle-ingress/src/answer.rs:531`) where ESS records or refuses.

Found 2026-11-26 by the adversary of wave 25 U4 (review-result:wave25-adversary-U4-r1); red on the
base 16bba3d5f, so pre-existing.

## Acceptance

- `a_map_count_guard_answers_as_the_interpreter`: `Optional<Map<String, Integer>>` with the guard
  `scores.count > 0`, for `{}` and `{"a": 7}`, selects the interpreter's outcome.
- `a_map_exists_guard_answers_as_the_interpreter`: `exists: {in: scores, as: s, that: s > 5}` over
  `{"a": 1}` selects the interpreter's outcome.
- The served JSON route answers the interpreter's outcome with its status, not 501.
