---
format: aep.planning-md/3
id: story:leases-by-default
kind: story
status: draft
title: A run on PostgreSQL takes its component leases by default
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A run on PostgreSQL takes its component leases by default: `database_lease` defaults to 10 s, and
two processes on one store without the setting fence each other as processes with it do since
wave 31. The lease connection fits a role limited to its pools' share plus one per process.

## Why

Wave 31's U6 (`story:several-writers-one-postgres`) takes the lease only when `database_lease` is
set (D9 C, decided 2026-11-27), because eventlog 0.10's hosted open refuses a role whose
connection limit leaves room for the lease's session. eventlog's draft
`story:hosted-open-counts-extra-connections` lifts that.

## Acceptance

- Spindle's eventlog dependency moves to the release that admits extra connections.
- Two processes on one store without `database_lease`: one serves, one stands by.
- `adversary_state_move::a_move_with_a_serving_role_limited_to_the_stores_share_moves_the_directory`
  passes with the role limit raised by one per process, and the docs name that rule.

## Not covered

- Waits on the eventlog release; it enters no wave before that release exists.
