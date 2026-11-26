---
format: aep.planning-md/3
id: story:view-paging
kind: story
status: implemented
title: A view that declares paging is answered
summary: A paged view is served and paged as ESS's interpreter pages it, instead of refused at start.
relations:
- serves: vision:spindle
- informed_by: story:views-with-filters-and-lists
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/served_operations.rs
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/host/reader.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/tests/fixtures/served-operations/domains/pad.yaml
- confidence: cited
  path: crates/spindle-ingress/tests/spindle-ingress/served.rs
- confidence: cited
  path: ess/fixtures/observe/served/notes.json
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 22
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T17:17:48Z", actor: "human:operator", revision: 20, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-25T17:17:49Z", actor: "human:operator", revision: 21, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:31Z", actor: "human:operator", revision: 22, decided_on: {"recorded":{"test_result":1,"review_outcome":14}}}
---
# A view that declares paging is answered

## Outcome

A view that declares paging is served and answered as ESS 0.99.1's interpreter pages it, instead
of being refused at start. ESS publishes the route with two optional integer query parameters, the
page and the size, under the wire names the view's `paging:` declares, and answers
`{"rows": [...], "total": n}` with `total` only when the view declares `total: true`
(`ess-gen/src/openapi.rs` `view_parameters`, `paged_schema`). The interpreter
(`ess-conformance/src/interpret/views.rs`):

- with neither parameter, answers every row the filter keeps;
- with both, filters, orders by `order_by` (ties in identity order), skips
  `(page - first_page) * size` rows and answers the next `size`;
- counts `total` over the filtered rows before the page is cut;
- refuses one parameter without the other, a negative value, a page below `first_page` and an
  overflow as unsupported. Spindle answers these 400, as it answers every refused view
  parameter (`HostError::Parameters`), and its conformance target reports them unsupported.

There is no default size and no maximum, as ESS's design states.

## Why

A consumer's census on 0.19.1: 9 paged views are refused, each at start with

```text
not served: view `svc.agents.Agents` at GET /things: unsupported: view svc.agents.Agents declares paging
```

`unsupported` refuses every view with `view.paging.is_some()`
(`crates/spindle-host/src/view.rs:77-78`). `story:views-with-filters-and-lists` left paging refused
("Parameters, computed fields, aggregation and paging stay refused as Unsupported"), and no story
has taken it since.

## Acceptance

The served-operations fixture (`crates/spindle-ingress/tests/fixtures/served-operations/domains/pad.yaml`)
holds three paged views over `notes.pad.Note`: `notes.pad.NotePages` as today
(`order_by: [note_id asc]`, `paging: {page: page, size: size, total: true}`, `first_page` left at
its default 0), and two this story adds: `notes.pad.NotePagesByText`
(params `p` and `n`, `order_by: [text asc]`, `paging: {page: p, size: n, first_page: 1}`, no `total`, so its page and size are read under the names `p` and `n`) and `notes.pad.FilteredNotePages`
(params `page` and `size`, `order_by: [note_id asc]`, `filter: text != "c"`, `paging: {page: page, size: size, total: true}`, `first_page` left at its default 0). The rows are five notes:
`n1` "b", `n2` "a", `n3` "b", `n4` "c", `n5` "a". `NotePagesByText` orders them `n2`, `n5`,
`n1`, `n3`, `n4`: the two "a" rows and the two "b" rows are tied and come in identity order.
`FilteredNotePages` admits `n1`, `n2`, `n3`, `n5`.

- `a_paged_view_is_served`: the three views are among the served routes in `GET /api/served`,
  where `NotePages` was refused with "declares paging".
- `pages_match_the_ess_interpreter`: for each of the three views and each size 1, 2 and 3, every
  page from `first_page` to one past the last equals, rows, order and `total` alike, what ESS
  0.99.1's interpreter (`ess_conformance::interpret::Interpreted`, run in the same test over the
  same five notes) answers for the same page and size.
- `a_read_without_page_and_size_answers_every_row`: `NotePages` read with neither parameter
  answers the five rows and `"total": 5`.
- `an_undeclared_total_is_not_answered`: `NotePagesByText` read with `p=1&n=2` answers the rows `n2`,
  `n5` and no `total` key.
- `a_filtered_view_pages_the_admitted_rows`: `FilteredNotePages` page 1 size 2 answers the rows
  `n3`, `n5` and `"total": 4`.
- `a_bad_page_is_answered_400`: on `NotePages`, a page without a size, a size without a page, a
  negative page, a negative size, and page `i64::MAX` with size 3 (the offset overflows; ESS refuses it through `checked_mul`), and on
  `NotePagesByText` `p=0` (below `first_page`), each answer 400.
- `the_conformance_target_answers_a_page_and_its_total`: `HostTarget::query_view` on `NotePages`
  page 1 size 2 answers the rows `n3`, `n4` and `SemanticViewResult::total` 5.
- `the_conformance_target_reports_a_bad_page_unsupported`: `HostTarget::query_view` reports each
  case of `a_bad_page_is_answered_400` as unsupported.

## Decided when scoped

- The page and the size are read from the query under the wire names `paging:` declares, as
  integers, both optional whatever type the view declares them at; nothing is read from or
  written to a header. The answer is the `rows` body with `total` beside it when declared. A
  paged view's binding can only answer `rows` (ESS refuses `single`, `array`, a member and
  `representations` for it: `ESS-HTTP-024`, `ESS-HTTP-035`).
- A refused page or size answers 400, as every refused view parameter does.
- Tied rows are ordered by identity, as ESS orders them. The tests use a text identity. Whether
  ESS and Spindle compare an identity that is not text the same way is not proven and is left
  to a follow-up if a consumer needs it.
- No default size and no maximum: a read that sends neither parameter gets every row.

## Scope

- `crates/spindle-host/src/view.rs`: `unsupported` stops refusing `view.paging`; `parameters`
  lets a read leave out both paging parameters whatever their declared type, and refuses one
  without the other, a negative value, a page below `first_page` and an overflow.
- `crates/spindle-host/src/host.rs` (`Host::list_with`, `listing`, `Listing::rows`) and
  `crates/spindle-host/src/host/reader.rs` (`Reader::list_with`): the page is cut after
  `crate::view::order`, and the listing carries `total`.
- `crates/spindle-ingress/src/answer.rs` (`answering`, `reading`, `listed`, `shaped`): `total`
  beside `rows` when declared.
- `crates/spindle-conformance/src/target.rs` (`HostTarget::query_view`): passes the page and size,
  sets `SemanticViewResult::total`, and reports a refused page or size as unsupported.
- Fixture: `crates/spindle-ingress/tests/fixtures/served-operations/domains/pad.yaml` gains
  `NotePagesByText` and `FilteredNotePages`.
- Tests: `crates/spindle-ingress/tests/spindle-ingress/served.rs`,
  `crates/spindle-cli/tests/spindle-cli/served_operations.rs` and
  `ess/fixtures/observe/served/notes.json` assert today that `NotePages` is refused; a new module
  in the existing `crates/spindle-ingress/tests/spindle-ingress/` binary and one in
  `crates/spindle-conformance`'s binary carry the acceptance.
- Not touched: `crates/spindle-ingress/src/router.rs`, `surface.rs`. `Route::parameters` already
  reads query keys under wire names as typed integers.
- Documents: `website/docs/concepts/specification.md` ("paging … refused as unsupported"),
  `CHANGELOG.md` `[Unreleased]` (one bullet).

## Shared files

- `crates/spindle-host/src/view.rs` `unsupported`: `story:cross-component-view-refused` edits it
  too and runs after this story (`depends_on`).
- `crates/spindle-host/src/host.rs` `Host::list_with` and `view.rs`: `story:view-read-families`
  counts the rows `list_with` returns and runs after this story (`depends_on`); for a paged read it
  counts the rows of the page answered, not `total`.
- `crates/spindle-host/src/view.rs` `project`: `story:view-rows-absent-optional-fields`; a
  different function, the overlap is accepted and whichever lands second rebases.
- `crates/spindle-host/src/host.rs`, other functions (execution, scrape-time reads, `advance`, the
  periodic path): `story:store-and-kernel-durations`, `story:queue-families`,
  `story:binding-external-timer-durations`, `story:periodic-bindings-retry-on-failure`,
  `story:world-row-integer-fields`; the overlap is accepted and whichever lands second rebases.
- `crates/spindle-ingress/src/answer.rs` `shaped`: `story:ess-099-path-types-and-wrapped-views`
  changed it for wrapped views (merged in 0.20.0); this story builds on that code.
- `CHANGELOG.md` `[Unreleased]`: every story that ships adds a bullet there.
- `crates/spindle-conformance/src/target.rs`: `story:one-time-response-admission` and `story:create-on-held-identity-unavailable` touch `execute_command` and `begin_scenario` there; this story touches `query_view`, the overlap is accepted and whichever lands second rebases.
