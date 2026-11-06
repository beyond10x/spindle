---
format: aep.planning-md/3
id: story:guard-over-absent-optional-input
kind: story
status: draft
title: A guard over an absent optional input is decided as ESS decides it
relations:
- serves: vision:spindle
revision: 1
---
# A guard over an absent optional input is decided as ESS decides it

## Outcome

A command whose guard reads an optional input that the request leaves out answers as ESS's
interpreter answers it, or with a declared outcome. Today the served HTTP route answers
501 "guard undecidable".

## Acceptance

- `a_guard_over_an_absent_optional_input_answers_as_the_interpreter` — a neutral fixture with an
  `Optional` input read by a `when:` guard, sent with and without the input, gives the
  interpreter's outcome both times, in process and over HTTP.

## Origin

Pre-existing. Reported by a consumer serving a test backend on main `88c1c5a`.
