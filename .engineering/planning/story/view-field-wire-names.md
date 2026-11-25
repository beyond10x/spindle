---
format: aep.planning-md/3
id: story:view-field-wire-names
kind: story
status: draft
title: A view row is answered under its fields' wire names
summary: 'A view field declared with wire: is answered under its wire name, as ESS''s projected row schema and generated servers answer it; 0.20.0 answers the model name.'
relations:
- serves: vision:spindle
revision: 1
---
# A view row is answered under its fields' wire names

## Outcome

A view field declared `{name: job_id, wire: jobId}` is answered under its wire name `jobId` in
every row a view route answers, as ESS's projected schema declares the row and as ESS's generated
servers answer it. A field without `wire:` keeps its model name. View parameters are not affected:
they are read under their wire names already (see Why).

## Why

A consumer sees `job_id` where its contract says `jobId`.

Probe on 0.20.0 (a one-component specification, not committed: entity `Job` with identity
`job_id` and field `run_by`, view `Jobs` with the parameter `{name: run_by, wire: runBy}` and the
fields `{name: job_id, wire: jobId}` and `{name: run_by, wire: runBy}`, served by `run` on the
laptop environment at the derived routes):

- `GET /queue/views/Jobs?runBy=ada` answered `200`
  `{"rows":[{"job_id":"j1","run_by":"ada"}]}`: the rows under the model names.
- The `OpenAPI` document ESS projects for the component (`ess_gen::openapi::json`) declares the
  row `jobs.queue.Jobs.Row` with the properties `jobId` and `runBy` (each `x-ess-field` naming the
  model name), both required, `additionalProperties: false`. The answer above fails that schema.
- `GET /queue/views/Jobs?run_by=ada` answered `400`
  `{"refused":"jobs.queue.Jobs: the parameter `run_by` is required"}`, and `?runBy=ada` was read:
  view parameters already use their wire names.

## Acceptance

- `a_view_row_is_answered_under_wire_names`: a fixture view with fields declaring `wire:`; red
  today with the row under the model names. After: each route's `200` answer validates against
  ESS's projected row schema, and its rows equal ESS's interpreter rows with each field renamed to
  its wire name.
- `a_field_without_a_wire_name_keeps_its_name`: a field with no `wire:` is answered under its
  model name.
