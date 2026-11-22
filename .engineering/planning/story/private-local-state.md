---
format: aep.planning-md/3
id: story:private-local-state
kind: story
status: implemented
title: Local state and diagnostics keep row and identity data private
relations:
- serves: vision:spindle
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T11:26:05Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-22T11:26:05Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-22T11:26:05Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Local state and diagnostics keep row and identity data private

## Outcome

- A SQLite store (`crates/spindle-kernel/src/sqlite.rs:223`, `SqliteStore::open`) creates a
  missing state directory as `0700` and the database, its lock file and any journal or WAL file as
  `0600`, whatever the umask. An existing file keeps its mode. In 0.17.0, under umask `0022`, both
  measured databases were created `0644`.
- The opt-in request record (`crates/spindle-ingress/src/observe.rs:631`, `GET
  /__spindle/requests`) keeps the route template (`/loans/{id}`) and not the raw path with its
  resource identifiers. A request that matched no route keeps no path segments beyond the first.
- The fixed-caller startup lines (`crates/spindle-cli/src/serve.rs:285`) print the caller's
  attribute names, never their values.

## Acceptance

- `a_new_sqlite_store_is_private`: under umask `0022` a store opened in a missing directory leaves
  the directory `0700` and every file it created `0600` (Unix only).
- `a_request_record_keeps_the_route_not_the_identifier`: a served `GET /loans/abc-123` is recorded
  with `/loans/{id}` and the text `abc-123` appears nowhere in `GET /__spindle/requests`.
- `the_fixed_caller_line_names_attributes_only`: `--fixed-caller '{"tenant":"t-secret"}'` prints
  `tenant` and not `t-secret` on stderr.
- The observe contract (`ess/` `spindle.observe`, `ess/generated/`, `ess/fixtures/observe/`,
  `crates/spindle-observe-api/`, `ui/`) changes with the record, and its three checks pass.

## Origin

Spindle 0.17.0 review, 2026-11-22: "Persistent SQLite state" and "Identity data in diagnostics".

## Residual

- A refusal reason that quotes an identity in backticks is still kept in the request record
  (implementor, wave review-017); no view was checked for such a reason.
- The request record keeps the field `path`, now holding the route template, so readers of the
  observe API need no shape change.
