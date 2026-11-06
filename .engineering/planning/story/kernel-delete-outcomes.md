---
format: aep.planning-md/3
id: story:kernel-delete-outcomes
kind: story
status: active
title: The kernel runs outcomes that delete their subject
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/tests/deletes.rs
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T09:08:51Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-06T09:08:51Z", actor: "human:timo", revision: 3}
---
# The kernel runs outcomes that delete their subject

## Outcome

An outcome that declares `deletes:` removes its subject row as ESS's interpreter does: the row
leaves every view, a later command on that identity answers as an unknown instance, and the
outcome's events are published. Today `crates/spindle-kernel/src/supported.rs` refuses `deletes:`,
so such a command is unsupported in process and answers 501 over HTTP, including a DELETE route.

## Acceptance

- `a_deleting_outcome_removes_its_subject` — a neutral fixture whose `RemoveBook` deletes the
  book: the view no longer lists it, and a second `RemoveBook` answers the unknown-instance refusal.
- `deletes_agree_with_the_interpreter` — for the same steps, rows and outcomes equal ESS's
  interpreter.
- `a_delete_route_runs_a_deleting_outcome` — `DELETE /books/{id}` answers the bound status and
  writes the deletion.

## Origin

Pre-existing. Found by the kernel-command-responses unit (wave 8); the ingress-delete-routes unit
tested DELETE with a lifecycle move because the kernel refuses `deletes:`.
