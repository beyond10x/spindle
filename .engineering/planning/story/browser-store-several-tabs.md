---
format: aep.planning-md/3
id: story:browser-store-several-tabs
kind: story
status: implemented
title: Several browser handles on one store see each other's writes
relations:
- depends_on: story:browser-store
- decomposes: epic:browser-host
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-store-browser/src
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T04:21:45Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-07T04:21:45Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-07T06:21:23Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1,"review_outcome":1,"verification":1}}}
---
# Several browser handles on one store see each other's writes

## Outcome

Two `BrowserStore` handles on one IndexedDB database (two tabs of one page) answer as one store: a
creation of an identity the other handle created is a conflict, and a change naming a version the
other handle moved past is a conflict, as `MemoryStore` answers one handle. Today each handle reads
the database once at `open` and checks versions against its own copy
(`crates/spindle-store-browser/src/lib.rs`, the crate docs say one handle per database).

## Acceptance

- `a_second_handle_creating_a_held_identity_is_a_conflict` (the adversary's case in
  `crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs`, ignored until this
  story) passes un-ignored in the browser lane.
- `a_second_handle_changing_a_moved_instance_is_a_conflict`: handle A changes an instance to
  version n+1; handle B, opened before, changing it at version n gets a conflict naming n+1.
- The store contract still passes on one handle.

## Origin

Adversary of `story:browser-store`, wave 11 (2026-11-06): the two-handle case is red and the
limitation is documented; filed so the ignored case has an owner.
