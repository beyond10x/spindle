---
format: aep.planning-md/3
id: story:query-input-beside-body
kind: story
status: draft
title: A command route reads input fields from the query string beside its body
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/ess_http_shapes.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/query_beside_body.rs
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/status.md
revision: 4
---
# A command route reads input fields from the query string beside its body

## Outcome

A command whose `ess-http/1` binding reads some input fields from the query string and the rest from
the body (`request.query` not empty) is served: each query field is read as ESS 0.99.1 publishes it,
joined with the body's fields, and the command runs on the joined input as ESS's interpreter runs
it. Today the ingress keeps the route and names it not served, "this ingress does not serve input
fields read from the query string beside the body" (`crates/spindle-ingress/src/surface.rs`,
`served`), and answers it 501.

## Acceptance

- `a_command_reads_its_query_fields_beside_the_body`: a binding with one query field and one body
  field; a request carrying both answers what the interpreter answers for the joined input.
- `a_missing_required_query_field_is_refused_as_ess_publishes_it`: the status and body ESS's binding
  gives a request without it.
- `a_query_field_that_is_not_a_scalar_is_still_not_served`: named at start, as today for a view
  parameter.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: a served run of its specification reported this reason for
`POST /api/v3/tasks/template/{template}`; no journey reached the route.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` — cited (the story names `surface.rs`, `served`)
- **Files:** `crates/spindle-ingress/src/surface.rs:1772-1774`, the `request.query` refusal in `served`, which is removed; and its doc comment at `:1716-1727` — cited
- **Files:** `crates/spindle-ingress/src/surface.rs:139-144` (`Route` fields beside `queried`/`parameters`), and a new reader beside `Route::flat_input` (`:565`) for only the fields `request.query` lists. ESS reads them with `http::flat_object(query, <listed rows>, &[], unknown, "query")`: under `refuse` a key no listed field names is refused `query.<key>: no field of this request is named so`, under `ignore` it is dropped; a key given twice is `expected one value, found 2`. The path is not consulted — inferred, from ESS 0.99.1 `ess-synth/src/rust/http/bound.rs` `queried` and `with_query`
- **Files:** `crates/spindle-ingress/src/answer.rs:215-225` (`answering`, the encoding match). After the `Json` or `Form` body is read, the query fields are joined into it. A body member or key under a listed field's wire name is refused `400` `body.<wire>: the query carries this field, so the body does not`, even when the query string leaves the field out. This runs before `ignoring` (`:589`) drops members and before `run`'s checks (`:449`) — inferred, ESS `with_query`
- **Symbols:** `served`, `Route`, `Route::flat_input`, `Parameter`, `answering`, `ignoring`, `form`; ESS `ess_gen::http::request(..).query`, `ess_http::Request::query_fields`, `ess_gen::http::queried`, `ess_gen::http::flat` — cited
- **Tests:** a new module `crates/spindle-ingress/tests/spindle-ingress/query_beside_body.rs`, registered in `crates/spindle-ingress/tests/spindle-ingress/main.rs`, on the `kiosk` fixture with a routes string. It holds the story's three tests (the third per D1), each under 60 s, the crate's test step under 120 s — inferred
- **Tests:** `crates/spindle-ingress/tests/spindle-ingress/ess_http_shapes.rs:322-332` names `request: {query: [input.holder]}` not served; that entry and the module doc at `:12` have to change — cited
- **Also likely:** tests for a body naming a listed field (JSON and form, with the query silent), an unlisted query key under `refuse` and `ignore`, and a listed key given twice, matching ESS's `query_beside_body.rs` cases — inferred
- **Documents:** `crates/spindle-docs/src/status.rs:381-385` (the "Form and query requests" entry, or a new one beside it), `website/docs/concepts/capabilities.md:143` (lists `request.query` as not served), `CHANGELOG.md` `[Unreleased]`; generated from `status.rs`: `website/docs/status.md`, `website/data/status.json` — cited for the first two, inferred for the rest
- **Confidence:** high — the story names the refusal site, and the ESS 0.99.1 sources give the exact read and join
- **Would collide with:** any unit editing `served` in `crates/spindle-ingress/src/surface.rs`; the caller-attribute check at `:1752-1761` sits right next to this hunk and shares the doc comment at `:1716-1727`. Also any unit editing `answering` or the input readers in `crates/spindle-ingress/src/answer.rs`, the `ess_http_shapes.rs` not-served table, and `spindle-ingress`'s test `main.rs`
- **Safety fact:** today every route whose `request.query` is non-empty is marked `Unserved::Ingress` and answers 501 before its body is read (`surface.rs:1772`, `answer.rs` `answered_as`). Routes without `request.query` therefore keep their bytes if the new branch runs only where `request.query` is non-empty. `multipart` keeps its refusal, which `served` checks first (`:1769`) — step 2, unproven
