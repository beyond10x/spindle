---
format: aep.planning-md/3
id: story:entity-invariant-over-stored-rows
kind: story
status: draft
title: A specification change that tightens an entity invariant is judged against the stored rows
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A specification change that adds or tightens an invariant on an entity is judged against the stored
rows: `run` exits 3 naming the entity and the number of stored rows that break the invariant, and
starts when every stored row holds it.

## Why

Found during wave 31's U5 correction round: the spec-change check reads each stored value under a
changed newtype or struct (prefix, alphabet, invariants) but not under an entity's own invariants,
so a run starts over rows its new specification does not admit. The docs page on a specification
change over stored rows says so.

## Acceptance

- A case adds an entity invariant that one stored row breaks; `run` exits 3 naming the entity and 1
  row, on SQLite and on PostgreSQL.
- A case adds an entity invariant every stored row holds; the run starts.
