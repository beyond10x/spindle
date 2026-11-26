---
format: aep.planning-md/3
id: story:guard-over-absent-row-field
kind: story
status: draft
title: A guard over an absent optional field of a held row is decided as ESS decides it
relations:
- serves: vision:spindle
revision: 1
---
# A guard over an absent optional field of a held row is decided as ESS decides it

## Outcome

A command guard that reads an `Optional` field of the held instance (or of a related row) which was
never written selects the outcome ESS 0.99.1's interpreter selects, instead of failing the command
as `undecidable`. Where the interpreter also leaves the guard undetermined, Spindle's refusal says
so and names the field, so the specification's author can add `defined(<field>)`.

Reported 2026-11-26 by two consumers on Spindle 0.21.0, at run time: an owner-enrolment guard over
`claimed_by` on an organization created without it, an auto-upgrade guard over `auto_upgrade` never
set, and a number-assignment guard over `phone_number_id` absent on a stocked number. Each guard is
answered `undecidable`, so the outcome behind it is unreachable. The kernel binds the held row's
fields as facts (`crates/spindle-kernel/src/facts.rs`) and an absent field leaves the leaf unknown.
Inferred, not traced: whether ESS reads an absent `Optional` leaf as false, as absent, or as unknown
in each comparison form.

## Acceptance

- `a_guard_over_an_absent_optional_field_decides_as_the_interpreter`: for each of `field == x`,
  `field != x`, `field in [..]`, ordering, `defined(field)` and `not (...)` over an `Optional`
  field of the held instance that was never written, the kernel selects the outcome
  `ess_conformance::interpret` selects, compared request by request.
- The same over a related row's absent `Optional` field read through `when_related:`.
- Where the interpreter determines no outcome, the kernel's refusal names the field and the guard,
  and `run` answers it as today.
