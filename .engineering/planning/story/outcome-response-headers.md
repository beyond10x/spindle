---
format: aep.planning-md/3
id: story:outcome-response-headers
kind: story
status: implemented
title: An outcome bound to a response header is served over HTTP
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T17:56:30Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T17:56:30Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-12T00:31:05Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# An outcome bound to a response header is served over HTTP

## Outcome

An HTTP binding that maps an outcome to a response header (for example `WWW-Authenticate` on an
authentication refusal) answers with that header and the declared status and body, instead of 501.

## Why

Reported by a downstream consumer on 2026-11-10: two refusals of its authentication commands are
bound to `WWW-Authenticate` and answer 501. `story:http-outcome-bodies` left headers out of scope.

## Acceptance

- On a neutral fixture whose refusal is bound to `WWW-Authenticate`, the HTTP answer carries the
  header with the value the binding declares, the declared status and the declared body, as ESS's
  HTTP projection describes it.
