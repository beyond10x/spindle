---
format: aep.planning-md/3
id: story:health-and-graceful-drain
kind: story
status: draft
title: A served run answers liveness and readiness, and drains on SIGTERM
summary: Liveness and readiness routes, readiness after the seed, and SIGTERM answering the requests in flight before exit.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- depends_on: story:seed-readiness
revision: 1
---
# A served run answers liveness and readiness, and drains on SIGTERM

## Outcome

A served `run` answers a liveness route while its host thread answers, and a readiness route
`200` only once its stores are open and its seed has ended (`story:seed-readiness`). On SIGTERM it
turns readiness to `503`, stops accepting, answers the requests already received, and then exits
143.

## Why

- No health route exists in any crate (no `/health`, `/livez` or `/readyz` route under `crates/`).
- On a signal the serve loop returns at once (`crates/spindle-cli/src/serve.rs:442-447`), and
  dropping the listener answers every request not yet answered `503`
  (`crates/spindle-ingress/src/listener.rs:215`, `listener.rs:368-375`). A rolling restart of a
  consumer's service fails the requests in flight.
- `story:seed-readiness` covers whether the seed has ended; it adds no liveness and no drain.

## Acceptance

- `liveness_answers_200_while_the_host_answers`: the liveness route answers `200`, and does not
  while the host thread is stopped.
- `readiness_waits_for_the_stores_and_the_seed`: readiness answers `503` until the seed has ended,
  then `200`.
- `sigterm_answers_the_requests_in_flight`: a request held by a slow external cause when SIGTERM
  arrives is answered with its outcome, not `503`; a new connection after SIGTERM is refused; the
  run exits 143 within the drain bound.

## Decided when scoped

- Route paths (under `/__spindle/` or of their own), whether a probe needs the proxy secret, and
  the drain bound. Whether the health answers are declared in `spindle.console` (`ess/`) first.
