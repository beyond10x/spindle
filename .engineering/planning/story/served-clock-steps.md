---
format: aep.planning-md/3
id: story:served-clock-steps
kind: story
status: draft
title: A served run steps its clock
relations:
- serves: vision:spindle
- informed_by: story:clock-and-mint-in-every-mode
revision: 1
---
# A served run steps its clock

## Outcome

A `run` serving HTTP with `--clock` answers `POST /api/clock` `{"at": "<RFC 3339>"}` by moving its
manual clock forward to that instant between commands and answering `{"at"}`, guarded by the proxy
secret as every `/api/*` route. An instant before the clock's current one is refused `409` and
leaves the clock where it stood; without `--clock` the route answers `409`. `GET /api/clock` answers
the current instant. A reset world may carry `"clock": "<RFC 3339>"` to restart the clock there.
Every record of `GET /api/events` and of `GET /__spindle/requests` carries `at`, the clock instant
its command ran. Today `--clock` stands at one instant for the whole served run
(`website/docs/reference/cli.md`, `--clock`), and only a stdin line's `"at"` moves it.

## Acceptance

- `a_served_clock_moves_forward_and_a_guard_reads_it`: on a served run with
  `--clock 2026-02-01T00:00:00Z`, a command whose guard holds only once a stored end date has passed
  is refused; after `POST /api/clock` to that date it runs.
- `a_served_clock_refuses_to_move_back`: a later `POST /api/clock` to an earlier instant answers
  `409` and `GET /api/clock` answers the instant before it.
- `a_served_run_without_clock_refuses_to_step`: without `--clock`, `POST /api/clock` answers `409`.
- `a_reset_world_restarts_the_clock`: a reset whose world carries `clock` sets the clock there.
- `every_event_and_request_record_carries_at`: each record of `GET /api/events` and
  `GET /__spindle/requests` (with `--observe`) names the clock instant its command ran.
- `the_clock_route_needs_the_proxy_secret`: on a `--trusted-caller-header` run, the route without
  the secret answers `403` and moves nothing.

## Not covered

- Periodic bindings: no served run fires them, and moving the clock fires none.

## Consumer gap

Recorded 2026-11-27. A consumer drives a multi-month simulation over HTTP and needs commands whose
guards read `now` to see the month it simulates, and an event time from Spindle rather than from
its driver. Spindle 0.22.0 fixes the clock of a served run at its start instant.
