---
format: aep.planning-md/3
id: story:view-filter-parameter-literal
kind: story
status: draft
title: A view filter compares a parameter with a literal
relations:
- serves: vision:spindle
- informed_by: story:view-filter-text-contains
revision: 1
---
# A view filter compares a parameter with a literal

## Outcome

A view filter leaf that compares a parameter with a literal of its type (`param.label == ""`,
`param.has_agent == false`), alone or combined by `any`, `all` and `not` with the leaves read
today, is read as ESS's interpreter reads it, and the view answers the same rows instead of being
refused whole. Today `crates/spindle-host/src/view.rs` (`unsupported`, `reads`) reads a field
compared with a literal or a parameter, and `defined(param.<name>)`, but no parameter compared with
a literal, so a filter of the optional-parameter form

    not defined(param.label) or param.label == "" or label contains {param: label}

is refused and the route answers 501.

Reported 2026-11-26 by a consumer: two list routes of its model answer 501 on Spindle 0.20.2 for
this reason alone, one with a text parameter compared with `""`, one with a Boolean parameter
compared with `false`.

## Acceptance

- `a_parameter_compared_with_a_literal_filters_as_the_interpreter`: a view whose filter is
  `not defined(param.label) or param.label == "" or label contains {param: label}` answers the
  interpreter's rows for the parameter left out, given `""` and given a text.
- The same for a Boolean parameter compared with `false` beside field leaves.
- `admit` and `run` serve both views; neither route answers 501.
