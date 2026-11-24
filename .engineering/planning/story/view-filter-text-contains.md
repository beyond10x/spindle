---
format: aep.planning-md/3
id: story:view-filter-text-contains
kind: story
status: draft
title: A contains filter against a parameter is served
revision: 1
---
## Outcome

A view whose filter tests a text field with `contains` against a parameter
(`address: {contains: {param: text}}`) is served, as ESS's interpreter reads it, instead of
being named not served by `admit` and `run` and answering `501`.

## Why

A downstream consumer searches people by a part of their address with exactly that filter. Spindle
refuses the view whole, so the search page has no rows on any store.

## Acceptance

1. A view whose filter is `contains`, `starts_with` or `ends_with` on a text field against a text
   parameter or literal answers the rows ESS's interpreter answers on the memory, SQLite and
   PostgreSQL stores: byte-wise and case-sensitive, the empty text matching every row, an
   `Optional` parameter left out holding no row, a field the row does not hold refusing the read.

## Scope

- `crates/spindle-host/src/view.rs`
- `crates/spindle-host/tests/spindle-host/text_filters.rs`, `tests/fixtures/text-filters/`
- `crates/spindle-conformance/src/target.rs`: views read with the scenario's parameters
