---
format: aep.planning-md/3
id: story:external-cause-bound-to-http-call
kind: story
status: active
title: An environment binds an external cause to an outbound HTTP call
tags:
- downstream-gap
relations:
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 3}
---
# An environment binds an external cause to an outbound HTTP call

## Outcome

An outcome decided `external:` can be answered by a named outbound port that the environment binds
to an HTTP call (method, URL, timeout, a secret by reference), as the design table lists:
"an outcome is decided `external:` | `external.<cause>` | a named port: HTTP call, named service, a
test double" (`docs/design/spindle.md:95`). The reply, a timeout or a failure answers the cause, and
the model records the result as the outcome it declares. No secret is written in a specification or
an environment document; the document names a reference.

Today an `external` cause is answered only from standard input, `--decisions`, `--answer-default`
or a world's `decisions`, and in HTTP mode a cause nothing answers gets 501 (`docs/reference/cli.md`
per the downstream facts; not re-read here). No port or adapter exists in `crates/`.

The downstream need: an application whose integrations are called by the runtime instead of by a
worker outside it. Which ESS construct carries the request payload of such a call is not
established; the first step is to read ESS 0.90's `external:` and binding constructs and record the
answer in the design.

## Acceptance

On a neutral fixture, an environment that binds one external cause to an HTTP call against a local
test server makes the command that asks the cause send exactly one request to that server, and the
outcome the reply selects is committed.
