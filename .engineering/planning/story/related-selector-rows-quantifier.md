---
format: aep.planning-md/3
id: story:related-selector-rows-quantifier
kind: story
status: draft
title: A when_related row-set selector that walks an entity's rows is decided
relations:
- serves: vision:spindle
revision: 1
---
# A when_related row-set selector that walks an entity's rows is decided

## Outcome

A `when_related:` row-set condition whose selector or test holds a rows quantifier
(`exists: {in: {entity: …}, …}` or `forall:` over the rows of an entity, ess/27) is decided as
ESS's interpreter decides it, and the command is served instead of refused at start. Today
`crates/spindle-kernel/src/supported.rs:389` refuses it as "a rows quantifier in a `when_related:`
row-set selector", because the kernel reads the row set from the store before selection and does
not walk a second entity's rows inside the selector.

Reported 2026-11-26 by a consumer: a usage-charging command guarded against rows of another
organization was refused by Spindle 0.21.0, and the consumer rewrote its guard to get served. The
same refusal stands for an `affects:` filter (`supported.rs:464`) and an `instances:` filter
(`supported.rs:486`); this story covers the `when_related:` selector, and names whether the other two
share its fix.

## Acceptance

- `a_rows_quantifier_in_a_related_selector_decides_as_the_interpreter`: a command whose
  `when_related:` row-set selector holds `exists: {in: {entity: …}, …}` over a second entity selects
  the outcome ESS 0.99.1's interpreter selects, for a request where a matching row exists and one
  where none does.
- The same with `forall:` in the row-set test.
- `admit` and `run` serve the command; the route answers the interpreter's status, not 501.
