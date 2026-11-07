---
format: aep.planning-md/3
id: story:browser-store-stale-write-conflict
kind: story
status: draft
title: A stale write across browser tabs is refused at once
relations:
- decomposes: epic:browser-host
- serves: vision:spindle
- depends_on: story:browser-store-several-tabs
revision: 1
---
# A stale write across browser tabs is refused at once

## Outcome

When two tabs hold handles on one browser store and one tab writes a row the other has since
changed, the second write is refused synchronously with the store's `Conflict`, the same answer a
stale write gets within one tab. Today, across real tabs, the refusal arrives late: a `Backend`
error on the next flush or the next call, never a `Conflict` on the write itself
(found while delivering `story:browser-store-several-tabs`).

## Acceptance

- `a_stale_write_from_another_tab_is_refused_with_conflict`: two handles in separate scopes over
  one store; the stale write returns `Conflict` from the write call, and no later call returns
  `Backend`.

## Scope

- `crates/spindle-kernel`: the `Store` interface needs a way to report a conflict decided
  asynchronously, or a version check before the write returns.
- `crates/spindle-store-browser`.
