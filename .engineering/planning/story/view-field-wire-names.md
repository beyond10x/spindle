---
format: aep.planning-md/3
id: story:view-field-wire-names
kind: story
status: active
title: A view row is answered under its fields' wire names
summary: 'A view field declared with wire: is answered under its wire name, as ESS''s projected row schema and generated servers answer it; 0.20.0 answers the model name.'
relations:
- serves: vision:spindle
- informed_by: story:wire-names-everywhere
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/tests/fixtures/wire-names
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/view_wire_names.rs
- confidence: cited
  path: website/docs/concepts/capabilities.md
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T10:58:36Z", actor: "human:operator", revision: 8, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-25T10:58:36Z", actor: "human:operator", revision: 9, decided_on: {"recorded":{"review_outcome":7}}}
---
# A view row is answered under its fields' wire names

## Outcome

A view field declared `{name: job_id, wire: jobId}` is answered under its wire name `jobId` in
every row a view route answers, as ESS's projected schema declares the row and as ESS's generated
servers answer it: each field under its wire name and its value written by its type (a struct's
members under their wire names, an enum variant in its wire spelling), the mapping every command
response already goes through (`crates/spindle-ingress/src/wire.rs`). A field without `wire:`
keeps its model name. The host keeps listing rows under the model names; only the HTTP answer is
mapped. View parameters are not changed: they are read under their wire names already (see Why),
and a test holds that.

## Why

A consumer sees `job_id` where its contract says `jobId`.

The compiled IR carries a view field's wire name at `views["<view>"].fields[i].naming.wire`.
ESS's generated servers answer under it; ESS's interpreter answers semantic rows and never applies
it, so an executor serving HTTP applies it itself. The comparison is therefore with ESS's
projected row schema and with the interpreter's rows renamed, not with the interpreter's rows as
they are.

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


## Carved out of story:wire-names-everywhere

`story:wire-names-everywhere` lists four answers that still key a value by its declared name; a
view's `rows` is one of them. This story takes the view's rows alone; event payloads in
`published`, the error `payload` of a refusing outcome and the console's `/api/events` stay with
that story, which drops view rows from its list when it is taken up.

## Shared files

- `crates/spindle-ingress/src/answer.rs`: this story adds `wired` and routes the two view reads
  (`answering`, `reading`) through it. The active `story:ess-099-path-types-and-wrapped-views`
  shapes the same answers (`shaped`), which this story leaves as they are. It lands after
  `story:view-filter-always-and-key`, whose branch it is based on.
- `crates/spindle-ingress/tests/spindle-ingress/main.rs`: one `mod` line beside the one
  `story:view-filter-always-and-key` adds.
- `CHANGELOG.md` `[Unreleased]`: one bullet of its own, beside those of
  `story:view-filter-always-and-key`, `story:ess-099-path-types-and-wrapped-views` and
  `story:node-export-serves-a-specification`; it edits none of theirs.
- `website/docs/concepts/capabilities.md`: one paragraph of its own after the request-body
  paragraph, saying how a view's parameters and rows are named; the active
  `story:ess-099-path-types-and-wrapped-views` updates the same page. It edits none of that
  story's lines, and lands after it and after `story:view-filter-always-and-key`.
- An absent `Optional` field: `story:view-rows-absent-optional-fields` decides how the host
  answers one. No row of this story's fixture holds one, so neither story's acceptance reads the
  other's outcome, and either may land first.

## Acceptance

One fixture, `crates/spindle-ingress/tests/fixtures/wire-names/`: entries with the view
`Grouped`, whose fields are `entry_id` (no wire name), `entry_kind` (wire `type`), `group_label`
(wire `groupLabel`), the `Optional` `about_text` (wire `aboutText`), the enum `shade` (variant
`pale_blue` spelled `PALE_BLUE` on the wire) and the struct `size` (member `width_cm`, wire
`widthCm`), and whose parameter `group_label` has the wire name `groupLabel`. Every entry holds
every field. Scenarios in the spindle-ingress test binary, module `view_wire_names`, each read
for the groups `tools`, `books` and `none` where it reads rows:

- `a_view_row_validates_against_the_projected_schema`: red today (the rows under the model
  names, `pale_blue`, `width_cm`); after, each answer is `200` with a body ESS's projected row
  schema admits.
- `a_view_row_is_answered_under_wire_names`: red today; after, the rows equal ESS's interpreter
  rows with each field renamed to its wire name, `pale_blue` spelled `PALE_BLUE` and `width_cm`
  named `widthCm`.
- `a_field_without_a_wire_name_keeps_its_name`: red today; after, every row holds `entry_id`
  and none of `entry_kind`, `group_label` and `about_text`.
- `a_view_parameter_is_read_under_its_wire_name`: `?group_label=tools` answers `400` and
  `?groupLabel=books` answers `200` with one row, today and after.
