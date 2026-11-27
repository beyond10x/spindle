---
format: aep.planning-md/3
id: story:dead-letters-kept-and-replayed
kind: story
status: active
title: A bound command that exhausts its attempts is kept as a dead letter and can be replayed
summary: A bound or periodic command whose last attempt failed or was refused, not escalated, is kept with its input, survives a restart, is listed and can be replayed once.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:event-egress-and-dead-letters
- depends_on: story:postgres-lane-kills-slow-tests
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/dead_letters.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/src/host/reader.rs
- confidence: inferred
  path: crates/spindle-host/src/lib.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/dead_letters.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/main.rs
- confidence: inferred
  path: crates/spindle-ingress/src/console.rs
- confidence: inferred
  path: crates/spindle-ingress/src/router.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/router.rs
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
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: inferred
  path: crates/spindle-observe-api
- confidence: inferred
  path: docs/design/spindle.md
- confidence: inferred
  path: ess/domains/console.yaml
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/fixtures/observe
- confidence: inferred
  path: ess/generated/jsonschema
- confidence: inferred
  path: ui/src/api/generated
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/components-and-bindings.md
- confidence: inferred
  path: website/docs/status.md
revision: 10
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T11:41:53Z", actor: "human:operator", revision: 9}
- {from: "proposed", to: "active", at: "2026-11-27T13:07:34Z", actor: "human:operator", revision: 10, decided_on: {"recorded":{"review_outcome":4}}}
---
# A bound command that exhausts its attempts is kept as a dead letter and can be replayed

## Outcome

A bound or periodic command whose last attempt failed or was refused, and that its binding did not
escalate, is kept as a dead letter with its binding, event, command, input, error and attempts. A
dead letter survives a restart, is listed on `GET /api/dead-letters` without field values, and
`POST /api/dead-letters/replay` runs it once; both routes are served only where the console is.
Split from `story:event-egress-and-dead-letters` (its D1 A).

## Why

- A bound command that fails after its retries is printed on stderr as "dropped" and lost
  (`crates/spindle-cli/src/serve.rs:612-650`, the word at `:637`).
- A bound command that is refused (`RefusedCommand`) is not printed at all; it is recorded only in
  the request log's `refused_bound`, and only under `--observe`
  (`crates/spindle-ingress/src/answer.rs:531`).
- `Failed` carries no input (`crates/spindle-host/src/host.rs:77-97`), so a dead letter is recorded
  inside `Host::attempt`, where the input and binding are at hand.

## Acceptance

- `an_exhausted_command_is_kept_as_a_dead_letter` (`spindle-host` and `spindle-cli`): a binding
  command that failed or was refused on every retry is listed with its binding, event, command,
  error and attempts.
- `a_dead_letter_survives_a_restart`.
- `a_replay_runs_the_command_once`.
- `a_replayed_dead_letter_is_not_replayed_again`.
- `an_escalated_command_is_no_dead_letter`.
- `a_dead_letter_lists_no_field_value` (D8 A).
- `a_dead_letter_is_no_published_event` (`spindle-kernel`): a dead letter shows neither in
  `GET /api/events` nor in `events_after`.
- `the_dead_letter_routes_are_not_served_with_the_console_off` (`spindle-ingress`).
- `postgres::a_dead_letter_survives_on_postgres`: the `task check:postgres` lane.
- The fixture of a dead letter passes `check:every_fixture_answer_validates_against_its_schema`.

## Model first

A dead letter is a noun Spindle's own specification does not declare. `DeadLetter` is declared in
`ess/` (`spindle.observe`, beside `RefusedBound`) and validated as this story's first commit.

## Architecture decision

Option A of `story:event-egress-and-dead-letters`, picked by the operator on 2026-11-27: stored
dead letters with a way to list and replay them, kept in the store; no new dependency.

## Not established when scoped

- Whether a console reset (`forget_events`) forgets dead letters.
- Whether `state move` copies the new `spindle:log` streams into PostgreSQL.
- Whether the scan and row folding ignore stream types other than `spindle:entity`; only
  `groups_after` was read.

## Scope

Derived 2026-11-27 by `story-scoper` against `plan/wave-31` (8eaea47a), as this story's part of the scope of `story:event-egress-and-dead-letters`. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (`Host::attempt`, where a dead letter is recorded) and `crates/spindle-cli` (served run: dead-letter log line) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:612-650` (the failure loop, "dropped" at `:637`) — cited
- **Files:** `crates/spindle-host/src/host.rs:77-97` (`Failed`, no input), `:2242-2380` (`Host::attempt`, where a dead letter is recorded) — cited
- **Files:** `crates/spindle-kernel/src/sqlite.rs:93-108` (reserved `spindle:log` streams), `:1269-1360` (`groups_after`, skip at `:1351`) — cited
- **Files:** `ess/domains/observe.yaml` (a `DeadLetter` type and list root beside `RefusedBound`, `:825`) — cited
- **Symbols:** `Host::attempt`, `Failed`, `RefusedCommand`, `Escalated`, `LogStore::groups_after`, `MARKS`, `RESET`, `PUBLISHED` — cited
- **Also likely:** `crates/spindle-kernel/src/store.rs` (`Store` methods for dead letters, default `Unsupported`), `crates/spindle-kernel/src/contract.rs`, `crates/spindle-kernel/src/lib.rs` — inferred
- **Also likely:** `crates/spindle-host/src/host/reader.rs`, `crates/spindle-host/src/lib.rs` (list beside the host's thread; `DeadLetter` re-export) — inferred
- **Also likely:** `crates/spindle-ingress/src/console.rs`, `crates/spindle-ingress/src/router.rs` (console routes to list and replay) — inferred
- **Also likely (generated, model first):** `ess/domains/console.yaml`, `Taskfile.yml` (`OBSERVE_ROOTS`, fixture check), `ess/generated/jsonschema`, `crates/spindle-observe-api`, `ui/src/api/generated`, `ess/fixtures/observe` — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/{main,dead_letters}.rs`, `crates/spindle-host/tests/spindle-host/{main,dead_letters}.rs`, `crates/spindle-kernel/tests/spindle-kernel/{main,log_streams,postgres}.rs`, `crates/spindle-ingress/tests/spindle-ingress/router.rs` — inferred
- **Documents:** `docs/design/spindle.md` § 11, `README.md` (the console route list), `CHANGELOG.md`, `website/docs/concepts/components-and-bindings.md` — inferred
- **Documents:** `crates/spindle-docs/src/status.rs` (a row for the new capability; `AGENTS.md:149` asks a capability change to change the site) with the generated `website/data/status.json` and `website/docs/status.md` — inferred (added 2026-11-27 on the plan critics' finding)
- **depends_on:** `story:postgres-lane-kills-slow-tests`, added 2026-11-27 on the plan critics' finding: the `postgres::` acceptance line relies on that lane's 60 s kill — cited (the plan)
- **Confidence:** medium. The defect site is read from the tree; where dead letters persist and the replay surface rest on D3 A and D5 A
- **Would collide with:** any unit changing `Host::attempt` or `Failed` in `spindle-host`; the reserved-stream handling and `groups_after` in `spindle-kernel/src/sqlite.rs`; the console routes of `spindle-ingress` and the serve loop of `spindle-cli`
- **Shares with `story:event-egress-webhook`:** `sqlite.rs` (the reserved streams and the skip at `:1351`, one hunk), `store.rs`, `contract.rs`, the kernel's `lib.rs`, `serve.rs`, design § 11, `README.md`, `CHANGELOG.md`, the `main.rs` module lists of the `spindle-cli` and `spindle-kernel` tests, `log_streams.rs`; no `depends_on`
- **Safety fact:** `groups_after` treats every feed event outside the `spindle:entity` streams, `RESET`, the `PUBLISHED` end and `MARKS` as a published domain event (`crates/spindle-kernel/src/sqlite.rs:1283-1360`). A dead-letter stream must be skipped at `:1351`, or it shows in `GET /api/events` and `events_after` and is posted to a webhook. Step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible. Numbered as in the scope of
`story:event-egress-and-dead-letters`.

- D1 A: this story is one of the two that split replaces; answered A by the host operator on
  2026-11-27.
- D3 A: dead letters are a reserved `spindle:log` stream in each component's log, one code path
  for SQLite and PostgreSQL, no change to `migrate`, written only on the host's thread.
- D5 A: console routes `GET /api/dead-letters` and `POST /api/dead-letters/replay`, answered on
  the host's thread and served only where the console is. Not taken: offline CLI verbs, which on
  PostgreSQL could become a second writer.
- D6 A: a dead letter is a last attempt that `Failed` or was `RefusedCommand`, not escalated, from
  a binding or a tick, `--drop-retries` included; a command whose input was unmet
  (`attempts == 0`) is none.
- D8 A: the list shows no field value, like `RefusedBound`; the input is stored for the replay
  only.
