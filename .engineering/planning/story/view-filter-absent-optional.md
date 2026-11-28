---
format: aep.planning-md/3
id: story:view-filter-absent-optional
kind: story
status: draft
title: A view filter over an unwritten optional field leaves the row out as ESS does
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A view whose filter compares an `Optional` field that a source row never wrote (a `Timestamp`
ordered against a parameter or `now`, or any other non-text operand) leaves that row out of the
view, as ESS's interpreter does, instead of answering 501 "the filter `…` is unknown for a row, so
the model does not determine the rows it holds".

Today `holds` in `crates/spindle-host/src/view.rs` (972-978) drops a row whose filter is Unknown
only when the unknown comes from an absent text operand or an absent navigation (`truth`,
1100-1104); an absent operand of another type makes the whole query fail.

## Why

Reported 2026-11-27 by a consumer on 0.25.0: four window views of one specification answer 501 when
a row's optional `Timestamp` is unset, unless the filter first checks `defined(<time>)`. ESS reads
the unset optional as not matching, so its conformance scenarios expect the row left out.

## Acceptance

- `a_view_filter_over_an_absent_optional_leaves_the_row_out`: for each of `==`, `!=`, `<`, `<=`,
  `>`, `>=` and `in` over an unwritten `Optional` field of type `Timestamp`, `Integer`, `Boolean`
  and a reference, alone and under `and`, `or` and `not`, the rows `HostTarget::query_view` answers
  equal the interpreter's, compared query by query.
- A served view with such a filter answers 200 with the row left out where it answered 501 before;
  red first at the base.
- A filter whose Unknown does not come from an absent field still answers 501 as today.
