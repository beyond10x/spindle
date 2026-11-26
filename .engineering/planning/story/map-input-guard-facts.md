---
format: aep.planning-md/3
id: story:map-input-guard-facts
kind: story
status: implemented
title: A guard over a present Map input is decided as ESS decides it
relations:
- serves: vision:spindle
- informed_by: story:guard-over-absent-optional-input
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/map_input_guard.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-kernel/src/facts.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/map-input-guard
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/map_input_guard.rs
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T21:00:28Z", actor: "human:operator", revision: 14, decided_on: {"recorded":{"review_outcome":20}}}
- {from: "proposed", to: "active", at: "2026-11-26T21:00:28Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"review_outcome":20}}}
- {from: "active", to: "implemented", at: "2026-11-26T23:02:26Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"test_result":1,"review_outcome":21}}}
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
  `scores.count > 0`, for `{}`, `{"a": 7}` and the key left out, selects the interpreter's outcome.
- `a_map_exists_guard_answers_as_the_interpreter`: `exists: {in: scores, as: s, that: s > 5}` over
  `{"a": 1}` and `{"a": 1, "b": 9}` selects the interpreter's outcome.
- `a_map_all_guard_answers_as_the_interpreter`: the same with `all`.
- `a_map_guard_is_served_with_the_interpreters_status` (`spindle-cli`): the served JSON route answers
  the interpreter's outcome with its status, not 501.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (the fact binding of `Facts`) — cited
- **Files:** `crates/spindle-kernel/src/facts.rs:160-183` (`Facts::bind`): the `Value::Object` arm at `:176` binds every object by member key, Map and Struct alike, and binds no `<path>.count` or `<path>.<index>` facts for a Map — cited
- **Files:** `crates/spindle-kernel/src/facts.rs:90-157` (`Facts::with_caller`, `Facts::row`, `Facts::values`): the three entry points that call `bind`. Each holds the declaring `ResolvedField`s (`fields`, `input`, `caller`), so a typed bind can look up a field's `type_ref` by name — cited
- **Symbols:** `Facts::bind`, `Facts::values`, `Facts::row`, `Facts::with_caller`, `ResolvedTypeRef::Map` — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:531`: the 501 comes from `other => Answer::unfinished(..)` for `ExecuteError::Undecidable`. It needs no change: once the kernel decides the guard, the route never reaches it — cited (the 501 site), inferred (no change needed)
- **Also likely:** `crates/spindle-kernel/tests/spindle-kernel/map_input_guard.rs`, a new module with its `mod` line in `crates/spindle-kernel/tests/spindle-kernel/main.rs`. Built like `absent_optional_guard.rs`: kernel against `ess_conformance::interpret` from an empty store — inferred
- **Also likely:** `crates/spindle-kernel/tests/fixtures/map-input-guard/` (`system.yaml`, `domains/`): one component reached over the network, with commands guarded by `scores.count > 0`, `exists: {in: scores, as: s, that: s > 5}` and `all`, over `Optional<Map<String, Integer>>` — inferred
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/map_input_guard.rs` with its `mod` line in `crates/spindle-cli/tests/spindle-cli/main.rs`, the served-route half. Built like `absent_optional_guard.rs` there, reading the kernel fixture and `examples/environments/laptop.toml` — inferred
- **Also likely:** `CHANGELOG.md` `[Unreleased]` `### Fixed`, one bullet — inferred
- **Documents:** the module doc of `crates/spindle-kernel/src/facts.rs:10-13` says "an object member at `<path>.<member>`", and it should also say that a Map binds as a list does — inferred
- **ESS 0.99.1:** `ess-conformance/src/input.rs:1812-1820` (`project_value`): a Map publishes `<path>.count` and its values at `<path>.<index>` in key order through `project_list` (`:1943-1960`), and no path reaches a key. The same projection backs the interpreter's row facts (`interpret/execute/history/facts.rs:157-158` via `input::project_observed`) and its caller facts (`interpret/execute/caller.rs:31`, `input::bind`). So ESS binds a Map like a list in input, row and caller facts — cited
- **Key order:** ESS's `Node::Map` is a `BTreeMap` (`ess-primitives/src/node.rs:30`). Spindle's `serde_json::Map` is also sorted, because `serde_json` is built without `preserve_order` (no `indexmap` among its dependencies in `Cargo.lock`). Index `n` therefore names the same value on both sides — cited (lockfile), inferred (no feature unification adds it later)
- **Confidence:** high — the story and the adversary finding name `facts.rs:160`, and the ESS rule it must match was read at the pinned rev
- **Would collide with:** any unit that changes `spindle-kernel`'s fact binding (`facts.rs`, in particular `bind`, `values`, `row` and `with_caller`); the module lists of the `spindle-kernel` and `spindle-cli` integration-test binaries; and `CHANGELOG.md` `[Unreleased]`
- **Safety fact:** the change only adds facts for a path whose declared type is `Map`. A Struct, List, scalar or undeclared path binds exactly as today. Every guard, selector, `when_subject:` row and struct invariant reads facts only through `Facts::{values,row,with_caller}` (`execute.rs:69,887,970,981,1001,1008,1071,1078,1091,1362,1661`, `execute/affects.rs:198-213`, `execute/set.rs:129,148`, `typed.rs:188`). So the effect is limited to Map-typed values, and there it moves toward ESS. Proven to step 2 (`git grep -n 'Facts::values\|Facts::row\|\.with_caller('`), unproven

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `Facts::bind` follows the declared type; a Map binds `count` and its values by
  index in key order, as ESS 0.99.1.
