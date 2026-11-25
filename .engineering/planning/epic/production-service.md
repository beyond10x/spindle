---
format: aep.planning-md/3
id: epic:production-service
kind: epic
status: draft
title: A specification's service runs exposed in production
summary: Verified callers, no console on the public surface, TLS, bounded load, audit, durable and evolvable state, health, logs, egress, HA, signed artifacts.
relations:
- serves: vision:spindle
revision: 1
---
# A specification's service runs exposed in production

## Outcome

A service a specification describes runs exposed in production: verified callers, no console on
the public surface, TLS, bounded load, audit, durable and evolvable state, health, logs, egress,
HA, signed artifacts.

## Why

A consumer runs a consumer's service on Spindle as a local prototype only. A survey of `main` at
`7bcc325ed` (2026-11-25) found twelve gaps between that and a service exposed to the internet: the
ingress trusts a proxy header for the caller, serves the console routes beside the public ones,
speaks plain HTTP, accepts any number of connections, records `spindle-kernel` as the actor of
every event, opens a durable store with no check against the specification it was written under,
has no sensitive-field handling, stops without draining, logs free text, sends no event out of the
process, assumes one writer per store, and publishes unsigned archives. Each story below cites its
lines.

## Stories, in priority order

| order | story | related existing drafts |
|---|---|---|
| 1 | `story:verified-caller-token` | `architecture-decision-record:http-caller-proof` (option B) |
| 2 | `story:console-routes-off` | |
| 3 | `story:listener-tls` | `story:decision-calls-tls` (outbound TLS already shipped) |
| 4 | `story:listener-limits` | |
| 5 | `story:events-record-their-caller` | |
| 6 | `story:spec-change-over-stored-rows` | |
| 7 | `story:sensitive-fields-redacted` | depends on ESS (see the story) |
| 8 | `story:health-and-graceful-drain` | `story:seed-readiness` (readiness after the seed) |
| 9 | `story:structured-json-logs` | `story:otlp-trace-export` |
| 10 | `story:event-egress-and-dead-letters` | `story:committed-events-survive-restart` |
| 11 | `story:several-writers-one-postgres` | `story:request-id-idempotency`, `epic:split-run` |
| 12 | `story:release-artifacts-signed` | `story:spindle-image-published`, `story:release-binary-portable-libc` |

The existing drafts keep their own epics; the stories here relate to them instead of repeating
them.

## Acceptance

- A consumer's service runs on one host off loopback with TLS, a verified token per request, no
  console route reachable, bounded connections and request time, liveness and readiness probes,
  JSON logs and a PostgreSQL store, from a signed release artifact, and every event it commits
  names the caller that caused it.
- Two processes of it on one PostgreSQL store keep answering when one is killed, and no request
  lands twice.

## Nouns

Most of this is runtime technicals, which never enter a specification (AGENTS.md: no technical
words in a specification): TLS, limits, logs, probes, signatures. Two stories may add a noun to
Spindle's own specification and model it in `ess/` before they are proposed: a dead letter
(`story:event-egress-and-dead-letters`) and the health answers (`story:health-and-graceful-drain`,
beside `spindle.console`). The caller an event records is the caller ESS already declares.

## Not in this epic

- A Kubernetes chart or operator (`epic:kubernetes-target`), and backups of the PostgreSQL store,
  which the operator of the database owns.
- The consumer's own login flow and proxy configuration.
- Ownership leases with fencing for cells across threads (`docs/design/spindle.md:454`, M5), beyond
  what `story:several-writers-one-postgres` needs.
