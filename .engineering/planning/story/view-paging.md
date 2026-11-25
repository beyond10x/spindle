---
format: aep.planning-md/3
id: story:view-paging
kind: story
status: draft
title: A view that declares paging is answered
summary: A paged view is served and paged as ESS's interpreter pages it, instead of refused at start.
relations:
- serves: vision:spindle
- informed_by: story:views-with-filters-and-lists
revision: 1
---
# A view that declares paging is answered

## Outcome

A view that declares paging is served and answered as ESS's interpreter pages it: the same rows
per page, in the same order, with the same cursor and limit semantics, instead of being refused
at start.

## Why

A consumer's census on 0.19.1: 9 paged views are refused, each at start with

```text
not served: view `svc.agents.Agents` at GET /things: unsupported: view svc.agents.Agents declares paging
```

`unsupported` refuses every view with `view.paging.is_some()`
(`crates/spindle-host/src/view.rs:74-75`). `story:views-with-filters-and-lists` left paging refused
("Parameters, computed fields, aggregation and paging stay refused as Unsupported"), and no story
has taken it since.

## Acceptance

- `a_paged_view_is_served`: a specification with a paged view starts with the view among the
  served routes in `GET /api/served`.
- `pages_match_the_ess_interpreter`: over a fixed set of rows, every page the served view answers,
  first to last, equals the page ESS's interpreter answers for the same request, rows and cursor
  alike.
- `a_paged_view_with_a_filter_pages_the_filtered_rows`: a paged view with a filter Spindle reads
  pages only the rows the filter admits.

## Decided when scoped

- How the cursor is read from and written to HTTP, as ESS publishes the route.
