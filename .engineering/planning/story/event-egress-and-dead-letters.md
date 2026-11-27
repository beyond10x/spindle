---
format: aep.planning-md/3
id: story:event-egress-and-dead-letters
kind: story
status: rejected
title: Committed events leave to a broker or webhook, and exhausted commands land as dead letters
summary: Events bound by the environment are delivered at least once to a broker or webhook; a binding command that exhausts its retries is kept and replayable.
tags:
- needs-owner-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:committed-events-survive-restart
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
- {from: "proposed", to: "rejected", at: "2026-11-27T11:32:43Z", actor: "human:operator", revision: 8}
---
# Committed events leave to a broker or webhook, and exhausted commands land as dead letters

## Outcome

An environment binds a specification's published events to a broker or a webhook, and Spindle
delivers each committed event there at least once, in commit order per stream. A binding's command
that exhausts its retries is kept as a dead letter an operator can list and replay, not only
printed.

## Why

- No event leaves the process except over Spindle's own channels between components: no broker
  or webhook delivery exists in `crates/`.
- A binding's command that fails after its retries is printed on stderr as "dropped" and lost
  (`crates/spindle-cli/src/serve.rs:456-471`).
- A consumer's service has other systems that react to its events.

## Acceptance

- `a_committed_event_reaches_the_webhook`: with an event bound to a webhook, a committed event is
  posted once; with the webhook down at commit, it is posted after it comes back, also across a
  restart (`story:committed-events-survive-restart`).
- `an_exhausted_command_is_kept_as_a_dead_letter`: a binding command refused on every retry is
  listed with its event, command, error and attempts, survives a restart, and a replay runs it
  once.

## Model first

A dead letter is a noun Spindle's own specification does not declare. It is declared in `ess/`
(`spindle.observe` or a domain of its own) and validated before this story is proposed. The
environment names the broker or webhook; a specification names neither (AGENTS.md).

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: split into `story:event-egress-webhook` (the scoper's D2, D3, D4, D7) and
  `story:dead-letters-kept-and-replayed` (D3, D5, D6, D8); each holds its own Outcome, Acceptance,
  Scope and decisions, and the scope entries are recorded there, not here. answered A by the host operator on 2026-11-27,
  and this default stands. Not taken: B, one story. This story is to be retired;
  the store refuses `proposed -> archived`, so it stays `proposed` until the coordinator moves it.

## Needs the owner's acceptance

The delivery is an outbound connection a run makes on its own.

## Architecture decision

Option A, picked by the operator on 2026-11-27: webhook delivery of published events plus stored
dead letters, with verbs to list and replay them. It reuses the TLS client already in
`spindle-cli` and adds an outbox cursor and a dead-letter table in the SQLite and PostgreSQL
stores; no broker client and no new dependency. A broker can sit behind a webhook relay. The pick
accepts the outbound connection a run makes on its own. It builds on
`story:committed-events-survive-restart`.

Not taken: B (A plus a broker client, a new dependency), C (dead letters only, no egress), D
(defer).

## Scope

Derived 2026-11-27 by `story-scoper` against `plan/wave-31` (8eaea47a). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` (served run: egress thread, dead-letter log line) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:612-650` (the failure loop, "dropped" at `:637`) — cited
- **Files:** `crates/spindle-cli/src/calls.rs:171-419` (`Call::send`, `exchange`, `client`, `file_roots`, `platform_roots`: the TLS client to reuse) — cited
- **Files:** `crates/spindle-host/src/host.rs:77-97` (`Failed`, no input), `:2242-2380` (`Host::attempt`, where a dead letter is recorded) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:93-108` (reserved `spindle:log` streams), `:1269-1360` (`groups_after`, skip at `:1351`) — cited
- **Files:** `ess/domains/observe.yaml` (a `DeadLetter` type and list root beside `RefusedBound`, `:825`) — cited
- **Symbols:** `Host::attempt`, `Failed`, `RefusedCommand`, `Escalated`, `LogStore::groups_after`, `MARKS`, `RESET`, `PUBLISHED`, `Reading::events_after`, `Call::send`, `Calls::bind` — cited
- **Also likely:** `crates/spindle-kernel/src/store.rs` (`Store` methods for cursor and dead letters, default `Unsupported`), `crates/spindle-kernel/src/contract.rs`, `crates/spindle-kernel/src/lib.rs` — inferred
- **Also likely:** `crates/spindle-host/src/host/reader.rs`, `crates/spindle-host/src/lib.rs` (list beside the host's thread; `DeadLetter` re-export) — inferred
- **Also likely:** `crates/spindle-capability/src/environment.rs` (an `[egress]` table naming `[http.<name>]` calls) — inferred
- **Also likely:** `crates/spindle-cli/src/egress.rs` (new), `crates/spindle-cli/src/main.rs` (bind beside `Calls::bind`, `:713`), `crates/spindle-cli/src/cli.rs` (help of `run`) and the generated `website/docs/reference/cli.md` — inferred
- **Also likely:** `crates/spindle-ingress/src/console.rs`, `crates/spindle-ingress/src/router.rs` (console routes to list and replay) — inferred
- **Also likely (generated, model first):** `ess/domains/console.yaml`, `Taskfile.yml` (`OBSERVE_ROOTS`, fixture check), `ess/generated/jsonschema`, `crates/spindle-observe-api`, `ui/src/api/generated`, `ess/fixtures/observe` — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/{main,egress,dead_letters,postgres}.rs`, `crates/spindle-host/tests/spindle-host/{main,dead_letters}.rs`, `crates/spindle-kernel/tests/spindle-kernel/{main,log_streams,postgres}.rs`, `crates/spindle-capability/tests/spindle-capability/main.rs` — inferred
- **Documents:** `docs/design/spindle.md` § 11, `README.md`, `CHANGELOG.md`, `website/docs/concepts/environment-and-admission.md`, `website/docs/concepts/components-and-bindings.md` — inferred
- **Confidence:** medium. The defect site and the reuse sites are read from the tree; where the cursor and dead letters persist, and the replay surface, depend on decisions still open
- **Would collide with:** any unit changing `Host::attempt` or `Failed` in `spindle-host`; the reserved-stream handling and `groups_after` in `spindle-kernel/src/sqlite.rs`; the console routes and serve loop of `spindle-ingress` and `spindle-cli`; `run`'s help in `cli.rs`
- **Safety fact:** `groups_after` treats every feed event outside the `spindle:entity` streams, `RESET`, the `PUBLISHED` end and `MARKS` as a published domain event (`crates/spindle-kernel/src/sqlite.rs:1283-1360`). A cursor or dead-letter stream must be skipped at `:1351`, or it shows in `GET /api/events` and `events_after` and is posted to the webhook itself. Step 2, unproven
