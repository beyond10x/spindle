---
format: aep.planning-md/3
id: review-result:w18-u2-adversary-1
kind: review-result
status: active
title: Wave 18 U2 adversary pass 1
relations:
- reviews: story:kernel-existing-instance
revision: 1
---
needs-revision

```
unit: wave 18 U2 story:kernel-existing-instance, commit fe90c61e on f9c99a67, tree spindle-w18-existing plus uncommitted test files
verdict: NEEDS-CHANGE
cases: executed 255→259, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
```

When the creation's identity comes from a field nested inside a struct input (`input.wrap.id`), a repeated add of an existing id returns an error the specification never declares. ESS answers its declared duplicate `ItemExists`. The kernel writes nothing in that case.

Cases added in `crates/spindle-kernel/tests/spindle-kernel/adversary_existing_instance.rs`, each comparing the kernel with ESS 0.90.0's interpreter, answers and stored rows:

- `a_held_identity_named_by_a_nested_input_member_answers_the_duplicate`: red.
  `left: Err("Undeclared { command: \"stock.items.AddWrapped\", reason: \"a `stock.items.Item` already has the identity \\\"item-1\\\", and `creates:` never replaces an instance\" }")`
  `right: Ok(("duplicate", Some(("stock.items.ItemExists", {"id": String("item-1")})), []))`
- `a_new_identity_named_by_a_nested_input_member_is_added`: green.
- `an_item_held_in_its_terminal_state_answers_the_duplicate`: green.
- `a_removed_item_is_added_again`: green.

Suite: `cargo test -p spindle-kernel`: `test result: FAILED. 258 passed; 1 failed; 4 ignored`, exit 101.

Finding: `crates/spindle-kernel/src/execute.rs:833` `Run::existing` reads the identity with `self.input.get(field)`. ESS reads it with `Context::get`, `ess_compiler::ir::read_input`, which follows dotted paths (`ir.rs:1463`). For `input.wrap.id` the kernel finds nothing, skips the existing check, and the creation hits the stored row and answers `Undeclared`. Named fix: `crate::values::read_input(self.input, field)` (`values.rs:413`). Introduced: on the base the command was refused as unsupported.

Attacked and could not break: the `Run::truth` claim (`precedence::place` puts `ExistingInstance` only in the RelatedRow or Existence phase, and `select_ahead` skips both); precedence against guards and input refusals; an existing row in its terminal state; a removed row added again; several creations at one address; generated or omitted identities. The different-addresses refusal narrows ESS, which answers when no `when_related:` branch exists; the CHANGELOG states it. The left-out identity-type check was not tested.

```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 833
  category: boundary
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: Run::existing reads the creation identity with a flat Map::get instead of the dotted-path read_input ESS uses, so a held identity named by a struct input member (input.wrap.id) answers Undeclared instead of the declared existing_instance error
```
