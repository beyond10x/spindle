---
format: aep.planning-md/3
id: story:console-span-waterfall
kind: story
status: draft
title: The observability console draws a request's span tree
relations:
- serves: vision:spindle
- decomposes: epic:observability-console
- depends_on: story:console-request-page
- depends_on: story:trace-nouns-specified
scope:
- confidence: inferred
  path: ui/src/console/fixtures
- confidence: inferred
  path: ui/src/console/traces
revision: 2
---
# The observability console draws a request's span tree

## Outcome

From the request page (`story:console-request-page`), a request whose record carries a
`trace_id` opens its span tree from `/__spindle/traces/{trace_id}`, drawn as a waterfall: one bar
per span, nested under its parent, coloured by level. A record with no `trace_id` opens nothing,
and the page says traces are off. It reads the trace nouns of `story:trace-nouns-specified`, and
its tests run against that story's fixture, so it does not wait for a run to serve traces.

## Acceptance

- `a_span_tree_is_drawn_by_level` (vitest): the rally span tree fixture of
  `ess/fixtures/observe/traces/` is drawn with one bar per span.
- `a_span_bar_nests_under_its_parent`: each bar is indented one level below its parent's bar.
- `traces_off_is_said`: with records carrying no `trace_id`, the page draws no tree and says that
  traces are off.

## Scope

- `ui/src/console/traces/` (new: the waterfall).
- `ui/src/console/fixtures/` (the trace fixture, copied from `ess/fixtures/observe/traces/`).
