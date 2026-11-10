---
format: aep.planning-md/3
id: story:cross-component-view-refused
kind: story
status: draft
title: A view that needs another process's store is refused by name
relations:
- serves: vision:spindle
- decomposes: epic:split-run
- depends_on: story:run-one-component
- depends_on: story:front-ingress
scope:
- confidence: cited
  path: crates/spindle-cli/tests/fixtures
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
revision: 3
---
# A view that needs another process's store is refused by name

## Outcome

A view is read from the store of the component that owns its source
(`crates/spindle-host/src/host.rs:1206-1208`, `host.rs:1719-1738`), and in one process every store
is at hand. In a split run, a view whose answer reads an entity owned by a component in another
process (a related field, `story:view-related-fields`, or any construct that reads beyond its
source's domain) is not served: before listening, the process prints one line per such view, and
the view's route answers `501` with the refusal "view <view> reads <entity>, owned by component
<component>, which runs in another process". A view that reads only its source's component is
served as today.

## Acceptance

- `a_view_across_processes_is_named_at_start`: on a fixture with a view reading an entity of
  another component, the split process prints one line naming the view, the entity and the
  component.
- `a_refused_view_does_not_stop_the_process`: after printing that line, the process listens.
- `the_refused_view_answers_501`: a read of that view answers `501` with the refusal text.
- `the_front_passes_the_refusal_on`: the same read through the front ingress answers the same
  `501` and text.
- `a_local_view_is_served`: a view reading only its own component answers its rows.

## Scope

- `crates/spindle-host/src/host.rs` (`readable`, line 1719; `reads`, line 1715),
  `crates/spindle-host/src/view.rs` (`unsupported`, line 48).
- `crates/spindle-ingress/src/surface.rs` (the `501` refusal lines at start).
- `crates/spindle-cli/tests/fixtures/` (a fixture with a view across components).
