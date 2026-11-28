---
format: aep.planning-md/3
id: story:egress-tests-steady-under-load
kind: story
status: draft
title: The egress tests pass on a loaded workstation
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

The egress tests in `crates/spindle-cli/tests/spindle-cli/egress.rs` and
`adversary_w31_u7.rs` pass in the PostgreSQL lane while other lane tests hold transactions open:
they wait for the posts by an observable state (the store's events visible to a read) with a
bound inside the 60 s a test may run, not 10 s.

## Why

Found by wave 31's U6 implementor: a PostgreSQL read shows a write only once every older
transaction on the server has ended, and other lane tests keep transactions open. U6's replica
tests failed on that now and then until they waited for the events; the egress tests wait 10 s
and can fail the same way.

## Acceptance

- The PostgreSQL lane passes 10 runs in a row with the egress tests inside it.
- Each egress test still fails when its webhook is never posted to.
