---
format: aep.planning-md/3
id: story:payload-field-through-conversion
kind: story
status: draft
title: An outcome field filled through a declared conversion runs
relations:
- serves: vision:spindle
- informed_by: story:binding-mappings-across-conversions
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/conform.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/fixtures/payload-conversion
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/main.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/payload_conversions.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: inferred
  path: crates/spindle-kernel/src/values.rs
revision: 8
---
## Outcome

An outcome whose payload or `sets:` field is filled through a declared conversion (a component's
`conversions:` entry, such as a stamped instant into a `Timestamp` field) runs in `run` (stdin and
served) and `conform`, as ESS 0.99.1's interpreter runs it: the value is passed on unchanged, and a
value read from a row is checked against the conversion's source and target types and refused as
ESS refuses it. Today the specification is admitted, and a request that selects such an outcome is
refused with "unsupported: the conversion `<because text>` into `<field>`"
(`crates/spindle-kernel/src/supported.rs:839-843`, refused whenever `field.conversion` is set).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: six outcomes on its first path are refused
this way, four of them needed by its walking skeleton (an assignment stamped `assigned_at`, a
session opened and a heartbeat stamped `last_seen_at`, a lease renewed). `story:binding-mappings-across-conversions`
covers binding mappings only.

## Acceptance

- `a_payload_field_through_a_conversion_is_written_as_ess_converts_it`: a neutral fixture with a
  `conversions:` entry; an outcome whose event field and `sets:` field are filled through it from an
  input answers, and the event and the row hold the value as sent, as ESS's interpreter
  (`ess_conformance::interpret`) gives them. It is refused on 0.22.0.
- `a_row_read_the_conversion_target_refuses_is_refused_as_ess_refuses_it`: a subject field read
  through a conversion into a target whose constraint the value breaks is refused as ESS refuses
  it, and nothing is written.
- `a_response_field_through_a_conversion_stays_unsupported`: a `{response: ...}` field with a
  conversion is still refused by name, as ESS does not interpret it.
- `conform_runs_an_outcome_with_a_conversion`: `conform` on the fixture executes every scenario
  that selects the outcome with the conversion (none is skipped as `unsupported`; on 0.22.0 each
  is), and every executed scenario passes.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:839-843` — cited, the refusal in `value()`
- **Files:** `crates/spindle-kernel/src/values.rs` — inferred, the target check on row reads that carry a conversion (`Sources::value` subject/changed arms near `:275-285`, `related` `:482`, `selected` `:516`), as ESS `determined::checked_read` does
- **Symbols:** `supported::value`, `Reading`, `ResolvedPayloadField::conversion` — cited; `values::Sources::value`, `crate::typed::validate` — inferred
- **Tests:** `crates/spindle-conformance/tests/spindle-conformance/payload_conversions.rs` (new module, one `mod` line in `main.rs`), `a_payload_field_through_a_conversion_is_written_as_ess_converts_it`, host against `ess_conformance::interpret` — inferred
- **Tests:** `crates/spindle-conformance/tests/fixtures/payload-conversion/` (new neutral fixture: a `conversions:` entry, an input of the source type filling an event field and a `sets:` field of the target type, plus a subject read into a constrained target) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/conform.rs`, `conform_runs_an_outcome_with_a_conversion`, reads the fixture above — inferred
- **Tests (added by the scoper, inferred):** `a_row_read_the_conversion_target_refuses_is_refused_as_ess_refuses_it`, `a_response_field_through_a_conversion_stays_unsupported`, both in the new conformance module
- **Also likely:** `CHANGELOG.md` `[Unreleased]` — inferred
- **Documents:** none required — inferred, no website page names the conversion refusal
- **Confidence:** high for `supported.rs` (the story names the line and it holds); medium for `values.rs` (it follows from reading ESS, not from the story)
- **Would collide with:** any unit editing `supported.rs::value()` or the `Sources::value` match in `values.rs` (kernel value sources); the `mod` list of `spindle-conformance/tests/.../main.rs`; `CHANGELOG.md`
- **Safety fact:** ESS passes every value through a conversion unchanged and only checks it, and refuses a `{response: …}` field that has one, so lifting the refusal for every other source and adding a target check on row reads matches ESS — level 2 (`ess-conformance/src/interpret/execute/determined.rs:270-288`, `:391-399`, `:425`; `execute.rs:2689-2693`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: as ESS 0.99.1: a value through a conversion is passed on unchanged; a value read
  from a row (subject, changed, related, selected) is checked against the source and the target
  type and refused as ESS refuses it; a `{response: ...}` field with a conversion stays refused;
  `sets:` keep their target check.
