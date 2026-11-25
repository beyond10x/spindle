---
format: aep.planning-md/3
id: story:sensitive-fields-redacted
kind: story
status: draft
title: Fields a specification marks sensitive appear in no answer and no log
summary: Sensitive entity fields and command inputs are redacted from events, views and logs, hashed where declared; waits on an ESS sensitivity marker.
tags:
- depends-on-ess
relations:
- serves: vision:spindle
- decomposes: epic:production-service
revision: 1
---
# Fields a specification marks sensitive appear in no answer and no log

## Outcome

A value of an entity field or a command input that the specification marks sensitive never
appears in `GET /api/events`, a view answer, the request log, a stderr line or a metric label;
where the specification says so, it is stored hashed and compared by hash.

## Why

A consumer's service holds password, secret and token fields. Spindle has no sensitive marker to
read: ESS 0.98.0, the pinned tag (`Cargo.toml:17`), and ESS 0.99.0 mark only component settings
`secret`, not entity fields or command inputs. Today `GET /api/events` answers every published
event in full (`crates/spindle-ingress/src/console.rs:90-96`), and views answer each observable
field as stored.

## Depends on ESS

This story waits on a sensitivity marker for entity fields and command inputs in ESS, and is not
proposed before an ESS release carries it. In the ESS repository's plan:

- ESS `story:field-sensitivity-class`: the marker this story reads. It waits on this one.
- ESS `story:a-secret-value-appears-in-no-output-of-the-process`: the matching rule on ESS's side.

Spindle reads the marker through `ess-compiler` as ESS publishes it; it adds no syntax of its own
(AGENTS.md: ESS is read through its crates).

## Acceptance

- `a_sensitive_field_is_redacted_from_events_and_views`: a command writing a sensitive field
  commits it; `GET /api/events` and every view answer it redacted.
- `a_sensitive_input_is_in_no_log_or_request_record`: a command with a sensitive input leaves the
  value in no stderr line and no `GET /__spindle/requests` record.
- `a_hashed_field_is_stored_and_compared_by_hash`: where the specification declares a field
  hashed, the store holds no plain value and a guard comparing an input with it decides as with
  the plain value.

## Decided when scoped

- Everything that depends on the shape ESS gives the marker.
