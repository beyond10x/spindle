---
format: aep.planning-md/3
id: story:kernel-caller-attributes
kind: story
status: draft
title: Caller attributes
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/replay.rs
- confidence: inferred
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/src/facts.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/caller.rs
revision: 7
---
# Caller attributes

## Outcome

The kernel reads the attributes of the caller a command is sent as: `{caller: <attribute>}` as a
`sets:` or `payload:` source, `caller.<attribute>` as an operand of `==`/`!=` in `when:` and
`when_subject:`, and selects a branch the caller decides (`decided_by_caller`). The caller's
attributes are supplied per invocation, separate from the input: a `run` stdin line carries
`"caller": {...}`, and `run --scenario` passes a step's `caller` instead of answering
`unsupported`. Today `crates/spindle-kernel/src/supported.rs` refuses `decided_by_caller`
(`shape`) and the caller value source (`value`), and `crates/spindle-cli/src/replay.rs` refuses
every step sent as a caller.

Requested by a downstream specification probe (2026-11-05, 44 commands): 1 command is refused
first on a branch the caller decides, and 3 more read caller values behind another refusal.
Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/execute/caller.rs`
(the facts of one invocation); `docs/design/caller-values.md` at ess 0.86.0.

## Acceptance

Fixture: a notebook whose actor declares the attribute `owner_id`; `CreateNotebook` sets
`owner_id: {caller: owner_id}`, and `EditNotebook` is refused with `not-yours` when
`owner_id != caller.owner_id`.

- `a_caller_value_is_stored_and_published` — the created notebook's owner is the caller's.
- `a_branch_the_caller_decides_refuses_another_caller` — a second caller is refused with
  `not-yours`; the owner edits.
- `a_command_sent_without_its_caller_is_refused_before_anything_is_written`.
- `replay_passes_a_step_s_caller` — a scenario step with `caller` runs instead of answering
  `unsupported`.
- `caller_attributes_agree_with_the_interpreter` — for the same steps, outcomes, events and rows
  equal what ESS 0.86.0's interpreter produces.

Out of scope: how an HTTP caller proves its attributes. That is an authentication technical bound
per environment and needs a recorded decision (design §11); until then a route whose command reads
the caller stays unserved.

## Scope

- `crates/spindle-kernel/src/supported.rs` — cited: `decided_by_caller` in `shape`, the caller
  source in `value`.
- `crates/spindle-kernel/src/execute.rs` — inferred: caller facts beside the input.
- `crates/spindle-kernel/src/facts.rs` — inferred: `caller.` operands.
- `crates/spindle-cli/src/replay.rs` — cited: the refusal in `execute_command`.
- `crates/spindle-cli/src/cli.rs` — cited: the stdin line shape.
- `crates/spindle-kernel/tests/caller.rs` — inferred, new.

## Depends on

Nothing.
