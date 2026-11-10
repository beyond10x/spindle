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
revision: 3
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
