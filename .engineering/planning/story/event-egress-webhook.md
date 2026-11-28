---
format: aep.planning-md/3
id: story:event-egress-webhook
kind: story
status: implemented
title: Published events leave to a webhook, at least once and in commit order
summary: Each published event the environment binds to an [http.<name>] call is POSTed at least once, in commit order per component store, and delivery resumes after a restart.
tags:
- needs-owner-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:event-egress-and-dead-letters
- informed_by: story:events-read-from-store
- depends_on: story:postgres-lane-kills-slow-tests
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: inferred
  path: crates/spindle-capability/src/environment.rs
- confidence: inferred
  path: crates/spindle-capability/tests/spindle-capability/main.rs
- confidence: cited
  path: crates/spindle-cli/src/calls.rs
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/egress.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/egress.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-kernel/src/contract.rs
- confidence: inferred
  path: crates/spindle-kernel/src/lib.rs
- confidence: cited
  path: crates/spindle-kernel/src/sqlite.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/log_streams.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: docs/design/spindle.md
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T11:41:53Z", actor: "human:operator", revision: 11}
- {from: "proposed", to: "active", at: "2026-11-27T13:07:33Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":4}}}
- {from: "active", to: "implemented", at: "2026-11-28T01:33:46Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"test_result":1,"review_outcome":7}}}
---
# Published events leave to a webhook, at least once and in commit order

## Outcome

Each published event the environment binds to an `[http.<name>]` call is POSTed there at least
once, in commit order per component store, and delivery resumes after a restart. Each POST carries
one event as a `spindle-egress/1` JSON object: `format`, `event` (the qualified event name),
`component`, `position` (its store position) and `fields` (the event's fields as published). The
cursor is a store position per component and egress binding, saved after a 2xx answer, in a
reserved `spindle:log` stream of the component's log; it is no published event. Split from
`story:event-egress-and-dead-letters` (its D1 A).

## Why

- No event leaves the process except over Spindle's own channels between components: no broker
  or webhook delivery exists in `crates/`.
- A consumer's service has other systems that react to its events.
- The TLS client exists in `crates/spindle-cli/src/calls.rs` (`Call::send`, `exchange`, `client`,
  `file_roots`, `platform_roots`), and the committed log is read through `Reading::events_after`
  (`story:events-read-from-store`).

## Acceptance

Each with a stub server on loopback, no outside network. Each POST's body is one JSON object of
the format `spindle-egress/1`, one event per request:

```json
{"format": "spindle-egress/1", "event": "<qualified event name>", "component": "<component>",
 "position": <store position>, "fields": {<the event's fields as published>}}
```

The stub records each body, and every line below reads the bodies, not a count of requests.

- `a_committed_event_reaches_the_webhook`: with an event bound to a webhook, a committed event is
  posted once, its body carrying `format`, `event`, `component`, `position` and the `fields` the
  event was published with.
- `an_event_committed_while_the_webhook_is_down_is_posted_when_it_returns`: the stub answers `503`
  at commit and `200` from 5 s later; the event's body arrives once the stub answers, and the test
  ends within 20 s (the backoff cap is 5 s, D7 A).
- `an_unposted_event_is_posted_after_a_restart`: SIGKILL of the run, restarted on its `--state`.
- `a_posted_event_is_not_posted_again_after_a_restart`.
- `events_of_one_component_are_posted_in_commit_order`: several events of one component arrive
  with strictly rising `position` and with `event` names in the order the commands committed them.
- `an_egress_on_a_memory_store_is_refused`: exit 3 at start (D4 A).
- `an_egress_naming_no_http_call_is_refused` and `an_egress_naming_an_unpublished_event_is_refused`
  (`spindle-capability`).
- `an_egress_cursor_is_no_published_event` (`spindle-kernel`): the cursor shows neither in
  `GET /api/events` nor in `events_after`.
- `postgres::an_egress_cursor_survives_on_postgres`: the `task check:postgres` lane.

## Needs the owner's acceptance

The delivery is an outbound connection a run makes on its own. The operator's pick of 2026-11-27
(option A of `story:event-egress-and-dead-letters`) accepts it.

## Architecture decision

Option A of `story:event-egress-and-dead-letters`, picked by the operator on 2026-11-27: webhook
delivery of published events over the TLS client already in `spindle-cli`; no broker client and no
new dependency. A broker can sit behind a webhook relay.

## Not established when scoped

- Whether a console reset (`forget_events`) moves the egress cursor.
- Whether `state move` copies the new `spindle:log` streams into PostgreSQL.
- Whether `play --component` and `deploy` post webhooks too; only the served `run` was scoped.

## Scope

Derived 2026-11-27 by `story-scoper` against `plan/wave-31` (8eaea47a), as this story's part of the scope of `story:event-egress-and-dead-letters`. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` (served run: egress thread) — cited
- **Files:** `crates/spindle-cli/src/calls.rs:171-419` (`Call::send`, `exchange`, `client`, `file_roots`, `platform_roots`: the TLS client to reuse) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:93-108` (reserved `spindle:log` streams), `:1269-1360` (`groups_after`, skip at `:1351`) — cited
- **Symbols:** `LogStore::groups_after`, `MARKS`, `RESET`, `PUBLISHED`, `Reading::events_after`, `Call::send`, `Calls::bind` — cited
- **Also likely:** `crates/spindle-kernel/src/store.rs` (`Store` methods for the cursor, default `Unsupported`), `crates/spindle-kernel/src/contract.rs`, `crates/spindle-kernel/src/lib.rs` — inferred
- **Also likely:** `crates/spindle-capability/src/environment.rs` (an `[egress]` table naming `[http.<name>]` calls) — inferred
- **Also likely:** `crates/spindle-cli/src/egress.rs` (new), `crates/spindle-cli/src/main.rs` (bind beside `Calls::bind`, `:713`), `crates/spindle-cli/src/serve.rs` (the egress thread beside the served run), `crates/spindle-cli/src/cli.rs` (help of `run`) and the generated `website/docs/reference/cli.md` — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/{main,egress,postgres}.rs`, `crates/spindle-kernel/tests/spindle-kernel/{main,log_streams}.rs`, `crates/spindle-capability/tests/spindle-capability/main.rs` — inferred
- **Documents:** `docs/design/spindle.md` § 11, `README.md`, `CHANGELOG.md`, `website/docs/concepts/environment-and-admission.md` — inferred
- **Documents:** `crates/spindle-docs/src/status.rs` (a row for the new capability; `AGENTS.md:149` asks a capability change to change the site) with the generated `website/data/status.json` and `website/docs/status.md` — inferred (added 2026-11-27 on the plan critics' finding)
- **depends_on:** `story:postgres-lane-kills-slow-tests`, added 2026-11-27 on the plan critics' finding: the `postgres::` acceptance line relies on that lane's 60 s kill — cited (the plan)
- **Confidence:** medium. The reuse sites are read from the tree; the cursor's place rests on D3 A
- **Would collide with:** the reserved-stream handling and `groups_after` in `spindle-kernel/src/sqlite.rs`; the serve loop and `run`'s help in `spindle-cli`; the `calls.rs` TLS helpers, which `story:verified-caller-token` makes reachable for its JWKS fetch
- **Shares with `story:dead-letters-kept-and-replayed`:** `sqlite.rs` (the reserved streams and the skip at `:1351`, one hunk), `store.rs`, `contract.rs`, the kernel's `lib.rs`, `serve.rs`, design § 11, `README.md`, `CHANGELOG.md`, the `main.rs` module lists of the `spindle-cli` and `spindle-kernel` tests, `log_streams.rs`; no `depends_on`
- **Safety fact:** `groups_after` treats every feed event outside the `spindle:entity` streams, `RESET`, the `PUBLISHED` end and `MARKS` as a published domain event (`crates/spindle-kernel/src/sqlite.rs:1283-1360`). A cursor stream must be skipped at `:1351`, or it shows in `GET /api/events` and `events_after` and is posted to the webhook itself. Step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible. Numbered as in the scope of
`story:event-egress-and-dead-letters`.

- D1 A: this story is one of the two that split replaces; answered A by the host operator on
  2026-11-27.
- D2 A: an `[egress]` table maps a qualified event to an existing `[http.<name>]` call, method
  `POST`, reusing its URL, timeout, secret and `ca_file` rules; an `http://` host off loopback is
  refused. Not a derived capability, which would fail admission for every published event left
  unbound.
- D3 A: the cursor is a reserved `spindle:log` stream in each component's log, one code path for
  SQLite and PostgreSQL, no change to `migrate`, written only on the host's thread.
- D4 A: an egress on a `memory` store is refused at start, exit 3.
- D7 A: a webhook that answers non-2xx, or does not answer, is retried with a backoff capped at
  5 s, without end, holding back that store's later events to keep commit order.
- Body, set by the coordinator on 2026-11-27 on the plan critics' finding, reversible: each POST
  carries one event as a `spindle-egress/1` JSON object with `format`, `event` (the qualified
  name), `component`, `position` (the store position) and `fields` (as published).
