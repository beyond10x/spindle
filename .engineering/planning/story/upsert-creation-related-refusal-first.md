---
format: aep.planning-md/3
id: story:upsert-creation-related-refusal-first
kind: story
status: active
title: 'An upsert''s creation answers its exists: false refusal first'
tags:
- downstream-gap
relations:
- serves: vision:spindle
- informed_by: story:kernel-creation-takes-unknown-row
scope:
- confidence: cited
  path: crates/spindle-kernel/src/execute.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/upsert-creation-related-refusal
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/upsert_creation_related_refusal.rs
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:47:56Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:47:56Z", actor: "human:timo", revision: 3}
---
# An upsert's creation answers its `exists: false` refusal first

## Observed (Spindle 0.8.0, ESS 0.90.0)

A command that creates or refreshes a row under an input key (`unknown_instance: true` on its
`creates:` branch, plus an `updates:` branch) and declares, first, a refusal
`when_related: {via: input.box_id, exists: false}`:

- no row under the key yet, no row behind `input.box_id`: Spindle selects the creation and
  answers 202 `recorded`. The refusal is not selected.
- a row under the key already held, same missing `input.box_id`: Spindle answers 409 with the
  refusal.
- a plain creation (`creates:` with a generated id, no `unknown_instance:`) with the same refusal:
  409 with the refusal.

So the same request is refused or accepted depending on whether the keyed row exists.

## Expected

ESS `docs/design/selection-plan.md` places `exists: false` over a row the input names in phase
`RelatedRow` (rank 1) and `unknown_instance:` in `Existence` (rank 3). The refusal answers first in
all three cases: 409.

## Reproduction

Neutral model `store.shelf` (`ess specify validate`: `store v1 — 3 file(s), valid`), the upsert:

```yaml
  - name: store.shelf.RecordNote
    naming:
      display: Record a note
      summary: Create or refresh the note under this key; the box must exist.
    input:
      - {name: key, type: store.shelf.NoteKey}
      - {name: box_id, type: Uuid}
      - {name: text, type: String}
    outcomes:
      - name: no-box
        when_related: {via: input.box_id, exists: false}
        error: store.shelf.BoxNotFound
      - name: recorded
        unknown_instance: true
        creates: store.shelf.Note
        instance: key
        sets:
          box_id: input.box_id
          text: input.text
        emits: [store.shelf.NoteRecorded]
        payload:
          store.shelf.NoteRecorded:
            key: input.key
      - name: refreshed
        updates: store.shelf.Note
        instance: key
        sets:
          text: input.text
        emits: [store.shelf.NoteRecorded]
        payload:
          store.shelf.NoteRecorded:
            key: input.key

```

`OpenTicket` creates a ticket with a generated id and declares the same `no-box` refusal first. Transcript on 0.8.0:

```
### plain creation, no box: expect 409 no-box
--- POST /shelf/commands/OpenTicket {"box_id":"00000000-0000-4000-8000-0000000000aa"}
{"error":"store.shelf.BoxNotFound","outcome":"no-box","published":[]} HTTP 409
### upsert creation, no box: expect 409 no-box
--- POST /shelf/commands/RecordNote {"key":"n1","box_id":"00000000-0000-4000-8000-0000000000aa","text":"t"}
{"outcome":"recorded","published":[{"event":"store.shelf.NoteRecorded","payload":{"key":"n1"}}]} HTTP 202
### upsert refresh, no box: expect 409 no-box
--- POST /shelf/commands/RecordNote {"key":"n1","box_id":"00000000-0000-4000-8000-0000000000aa","text":"t2"}
{"error":"store.shelf.BoxNotFound","outcome":"no-box","published":[]} HTTP 409
```

## Acceptance

A kernel test on this model answers 409 `no-box` for all three requests, equal to ESS 0.90.0's interpreter.

## Lead (inferred, not checked)

`story:kernel-creation-takes-unknown-row` (wave 16, in 0.8.0) changed how a creation beside `unknown_instance:` takes an absent row (`Phase::Existence` in `Run::select`, `crates/spindle-kernel/src/execute.rs`). That is the first place to look.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` — cited, the Lead names `crates/spindle-kernel/src/execute.rs`
- **Files:** `crates/spindle-kernel/src/execute.rs` — cited, `Phase::Existence` in `Run::select` (lines 481-587, the existence step at 502-518)
- **Symbols:** `Run::select`, `Phase::Existence` — cited
- **Symbols:** `Run::creation_takes_absent` (:808), `Run::missing_related` (:887), `reads_missing` (:353), the `Phase::RelatedRow` arm (:496) — inferred
- **Also likely:** `Run::look_ahead` / `Run::select_ahead` (:1026, :1063) — inferred
- **Also likely:** a new test in `crates/spindle-kernel/tests/spindle-kernel/` against ESS 0.90.0's interpreter, its `mod` line in `main.rs`, and a new fixture under `crates/spindle-kernel/tests/fixtures/` (the name `shelf` is taken) — inferred
- **Documents:** none — inferred
- **Confidence:** medium — the story names the file and function but marks its Lead "not checked"; the defect could sit in `creation_takes_absent` or in the order of the `RelatedRow`/`Existence` arms
- **Would collide with:** units touching `execute.rs` selection (`Run::select` and helpers) and units adding a `mod` line to `crates/spindle-kernel/tests/spindle-kernel/main.rs`
- **Safety fact:** the change affects only commands with an addressed absent row and an `exists: false` branch over a row the input names: the `RelatedRow` read is gated on `!stored && reads_missing(&plan, Phase::RelatedRow)` (:496) and the existence answer on `addressed.stored.is_none()` (:503-505) — unproven
- **Not established:** the root cause (likely `no-box` placed in `PresentRelated`, read late); whether `creation_takes_absent` applies; whether copying ESS's `existence::existing` call inside its `RelatedRow` arm (`ess-conformance` `src/interpret/execute.rs:622` at `9a2468e`) is the fix; whether `values::related` (:262) is involved.

## Finding (wave 24): the interpreter answers 202 for the upsert creation

ESS 0.92.0's interpreter, and 0.90.0's (`interpret/execute.rs` and `command/precedence.rs` are
byte-identical), answer request 2, the upsert creation naming a missing box, `202 recorded`, not
`409 no-box`. From `ess/27` an `exists: false` branch over an input-named row is read at the
related-row step (`PresentRelated`, ess-domain `command/precedence.rs:487`), after the existence step
where `unknown_instance:` takes the absent row. ESS's `docs/design/generic-language-features.md`
(lines 460, 488-494) calls `selection-plan.md`, which this story cites, stale. Under `ess/26` the
validator refuses this command (ESS-COMMAND-004).

The kernel already answers as the interpreter does and is unchanged. Commit fe4f4a3d pins the plain
creation and the upsert refresh to `no-box`, the upsert creation to `recorded`, and every answer and
stored row of four step lists to the interpreter's. The acceptance line "409 for all three" cannot
hold beside "equal to the interpreter"; the downstream consumer takes it up on the model side.
ESS's own refusal hint (ess-domain `command/related_guard.rs:782`) says the unknown identity answers
after an `exists: false` over the input's row, which contradicts its precedence and interpreter.
