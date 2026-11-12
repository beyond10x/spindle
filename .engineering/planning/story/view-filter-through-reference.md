---
format: aep.planning-md/3
id: story:view-filter-through-reference
kind: story
status: implemented
title: A view filter read through a reference or against a parameter is served
relations:
- serves: vision:spindle
- depends_on: story:view-related-fields
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T20:31:12Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T20:32:22Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-12T01:42:32Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

A view whose filter reads a field through a reference, or compares a field with a parameter or
another field, is served, as ESS's interpreter reads it, instead of answering `501`.

## Why

A downstream consumer on 0.14.0 (12 contexts, about 183,000 events) has 6 views refused at start
with "only a field compared with a literal of its type or ordered against `now`", shown on 13
pages. Their filters, with names neutralised:

- `state == Proposed and class.holder == person`: a field reached through a reference, compared
  with another field
- `person_id == param.person_id and approval_id.state == Pending`
- `skill == param.skill and state == Attested and person_id.state == Active and (...)`
- `team_id == param.team_id and state == Active and person_id.state == Active and (...)`
- `canvas_id == param.canvas_id and state == Placed and (canvas_id.owner == param.viewer or
  (canvas_id.visibility == shared ...))`, two views

A filter with `field == param.x` alone is served. The refused parts are the reference hop and the
comparison of a referenced field with a parameter or another field.

## Acceptance

1. A view whose filter reads `<reference>.<field>` answers the rows ESS's interpreter answers for
   the same state, on the memory and SQLite stores, including a row whose reference names no row
   (`absent_navigation`).
2. A filter comparing a field with another field of the row, or a referenced field with a
   parameter, is served the same way.
3. Each row a reference names is loaded once per read, by its identity, as
   `story:view-related-fields` loads it; the referenced entity is not scanned.

## Scope

- `crates/spindle-host/src/view.rs`, inferred
- `crates/spindle-host/src/host.rs`, inferred
