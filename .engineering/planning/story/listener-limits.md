---
format: aep.planning-md/3
id: story:listener-limits
kind: story
status: draft
title: The ingress listener bounds connections, headers and request time
summary: A connection cap, a head size limit and a per-request time, and a proxy-secret compare that does not reveal the length.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
revision: 1
---
# The ingress listener bounds connections, headers and request time

## Outcome

The listener holds at most a configured number of connections, refuses heads over a configured
size, and answers every request within a configured time, `503` or `504` past it. The proxy
secret is compared in time that does not reveal its length.

## Why

- Each accepted connection is spawned with no cap (`crates/spindle-ingress/src/listener.rs:338-345`).
- The HTTP/1.1 builder sets only a header read timeout (`HEADER_READ`, 10 s, `listener.rs:34`,
  `listener.rs:380-383`): no limit on header count or head size beyond hyper's defaults.
- The body read is bounded (`BODY_READ`, `BODY_LIMIT`, `listener.rs:534-537`), but a request then
  waits for the host's answer with no bound (`listener.rs:621-627`), so one slow command holds
  every request queued behind it.
- `same` (`listener.rs:203-210`), which `presents` uses for the proxy secret (`listener.rs:589-595`),
  returns at once when the lengths differ, so a caller learns the secret's length.

## Acceptance

- `connections_over_the_cap_are_refused`: with a cap of N, the N+1st concurrent connection is
  refused or held until one closes, and the first N are answered.
- `a_head_over_the_limit_answers_431`: a request whose head exceeds the limit answers `431` and
  writes nothing.
- `a_request_past_its_time_answers_and_frees_the_listener`: a command held past the request time
  answers `503` or `504` to its caller; the next request is answered.
- `the_secret_compare_takes_the_same_time_for_any_length`: the compare reads the same number of
  bytes for a presented value shorter, longer and equal in length to the secret.

## Decided when scoped

- Defaults for each bound, and whether a timed-out command is still applied when the host reaches
  it.
