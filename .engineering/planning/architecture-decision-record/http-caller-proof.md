---
format: aep.planning-md/3
id: architecture-decision-record:http-caller-proof
kind: architecture-decision-record
status: accepted
title: An HTTP caller proves its actor through a header a trusted proxy sets
relations:
- decides: story:http-caller-proves-actor
revision: 2
transitions:
- {from: "proposed", to: "accepted", at: "2026-11-10T09:45:52Z", actor: "human:timo", revision: 2}
---
# An HTTP caller proves its actor through a header a trusted proxy sets

## Context

`docs/design/spindle.md` § 11 lists "how an HTTP caller proves its actor and attributes
(authentication)" as undecided. `ess/components.yaml:37-43` holds it as `UNMAPPED:`. Today a command
that reads its caller answers `501` unless `run` fixes one caller for the whole run with
`--fixed-actor` and `--fixed-caller`, which is a test switch. `story:http-caller-proves-actor` cannot
start without the decision.

## Options

- **A. A trusted proxy header.** A reverse proxy in front of Spindle authenticates the user and sets
  a header that carries the actor and the caller attributes. Spindle reads it only when the
  operator enables it with a flag. Smallest change. Fits a deployment that already runs a proxy in
  the same pod.
- **B. OIDC tokens checked in Spindle.** Spindle verifies a JWT against the provider's keys (JWKS)
  and maps its claims to the actor and the caller. It needs a key fetch and a configuration, about
  three times the work of A.
- **C. Client certificates (mTLS).** Every client needs a certificate.

## Decision

A. Spindle trusts a caller header only when the operator enables it, and only from a listener the
operator chose. Without the flag, a command that reads its caller keeps answering `501`. B and C
stay possible later as other sources of the same caller.

## Consequences

- `story:http-caller-proves-actor` implements A. The header name, its encoding, the flag and the
  answer to a request that lacks the header are decided in that story and written to § 11.
- The proxy, not Spindle, authenticates. A Spindle listener reachable without the proxy accepts any
  header, so the flag is documented together with `story:ingress-bind-address`.

## Who decided

The operator, 2026-11-10: "i accept all ADRs", in answer to a report listing A, B and C with A
recommended.
