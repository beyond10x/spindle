---
format: aep.planning-md/3
id: story:verified-caller-token
kind: story
status: proposed
title: A served run verifies a signed caller token itself
summary: Each request's OIDC/JWT token is verified against the issuer's JWKS and its claims mapped to the actor and caller attributes.
tags:
- needs-owner-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: architecture-decision-record:http-caller-proof
- informed_by: story:http-caller-proves-actor
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
---
# A served run verifies a signed caller token itself

## Outcome

A served `run` verifies a signed token on each request (OIDC/JWT, signature checked against the
issuer's JWKS, issuer, audience and expiry checked) and maps its claims to the actor and the
caller attributes of the model, so no request proves a caller by a header any proxy could copy.

## Why

Today a caller is proven only by `--trusted-caller-header`: a JSON value a proxy sets, trusted
because the proxy also sends a shared secret (`crates/spindle-cli/src/cli.rs:244-262`;
`Caller::proven`, `crates/spindle-ingress/src/surface.rs:142-158`, parses the value and verifies
nothing). A consumer's proxy copies a value the browser chose into that header, so any user acts
as any actor. `architecture-decision-record:http-caller-proof` chose the proxy header and kept
"OIDC tokens checked in Spindle" (option B) open as another source of the same caller.

## Acceptance

- `a_request_with_a_valid_token_runs_as_its_mapped_caller`: a token signed by a key of the
  configured JWKS, with the configured issuer and audience, runs a command that reads the caller
  as the actor and attributes its claims map to.
- `a_bad_token_is_refused_and_writes_nothing`: an expired token, a wrong issuer or audience, an
  unknown key id, a signature that does not verify, `alg: none`, and no token each answer `401`
  and move no row, event or counter.
- `a_claim_that_maps_to_no_caller_of_the_model_is_refused`: a token whose claims name an
  undeclared actor or leave out a required attribute answers `403` "not granted".
- `keys_rotate_without_a_restart`: a key added to the JWKS after start verifies a token after the
  next refresh; a removed key no longer does.

## Decided when scoped

- How claims map to the actor and attributes (a mapping file or flags), the JWKS refresh period,
  and whether the token source excludes `--trusted-caller-header` or stands beside it.

## Needs the owner's acceptance

The JWKS fetch is an outbound connection a run makes on its own.

## Architecture decision

Option A, picked by the operator on 2026-11-27: a JWT checked against a JWKS beside
`--trusted-caller-header`, which keeps working. New dependency `jsonwebtoken`, a claim-mapping
file, and a periodic JWKS fetch. The pick accepts the JWKS fetch as an outbound connection a run
makes on its own.

Not taken: B (the token replaces the header flag), C (no change; the docs tell the proxy what to
strip), D (defer).
