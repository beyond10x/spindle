---
format: aep.planning-md/3
id: story:console-request-page
kind: story
status: draft
title: The observability console shows a model's recent requests and their span trees
relations:
- serves: vision:spindle
- decomposes: epic:observability-console
- depends_on: story:console-model-pages
scope:
- confidence: inferred
  path: ui/src/console/fixtures
- confidence: inferred
  path: ui/src/console/requests
- confidence: cited
  path: ui/src/panel/requests.ts
revision: 4
---
# The observability console lists a model's recent requests

## Outcome

A request page per model lists the requests its `/__spindle/requests` holds (the ring that exists
today, `spindle.observe.RequestLog`), newest first, filterable by class, with each request's
route, operation, class, outcome, reason and duration. Opening a request's span tree is
`story:console-span-waterfall`.

## Acceptance

- `the_request_page_lists_the_ring` (vitest, recorded answers): every record of the recorded
  `RequestLog` is listed, newest first.
- `the_class_filter_keeps_one_class`: choosing `not-served` lists only the records of that class.
- `the_totals_banner_matches_the_log`: the page's totals equal the recorded `RequestTotals`.

## Scope

- `ui/src/console/requests/` (new: the request page), `ui/src/panel/requests.ts` (the ring reader,
  reused).
- `ui/src/console/fixtures/` (a recorded `RequestLog`).
