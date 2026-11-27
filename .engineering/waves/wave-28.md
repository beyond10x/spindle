# Wave 28

**Proposed.** Planned by the coordinator on the host operator's order of 2026-11-27, relaying the
operator's words "resolve all missing spindle features". Skill `aep:implementing` 0.26.0, wave mode.
All stories serve `vision:spindle`.

Integration branch `wave/28`, forked from `plan/wave-28` (based on `main` at release 0.22.0). The
ESS pin 0.99.1 stays.

## Selection

The triage of the 98 open stories on 2026-11-27 put the gaps consumers reported first: running
Spindle as a production service (console off, JSON logs, bounded metrics, seed readiness, health
and drain), the history of one instance with the caller of each event, decisions answered from
the environment and in replay, and a consumer-reported replay fault on one-time responses
(`story:one-time-response-admission`, which shares `Replays::run` with replay with decisions). The
host operator accepted this list for the operator on 2026-11-27.

Seven stories that would change the architecture wait for the operator's picks, put to the operator
one at a time by the host operator, and are not in this wave: `story:event-egress-and-dead-letters`,
`story:verified-caller-token`, `story:listener-tls`, `story:several-writers-one-postgres`,
`story:spec-change-over-stored-rows`, `story:one-ingress-several-specifications` and
`story:spindle-image-published`. `story:sensitive-fields-redacted` waits for an ESS release.

Each story's `## Scope` was derived against `plan/wave-28`; its `## Decided when scoped` records the
options taken, and its `## Acceptance` was amended where the scoper found the draft stale.

## Units

| unit | stories, in order | branch | tree |
|---|---|---|---|
| U1 | `story:console-routes-off` | `unit/w28-console-off` | `spindle-w28-console` |
| U2 | `story:structured-json-logs` | `unit/w28-json-logs` | `spindle-w28-logs` |
| U3 | `story:metrics-series-bounded` | `unit/w28-metrics` | `spindle-w28-metrics` |
| U4 | `story:seed-readiness`, `story:health-and-graceful-drain` | `unit/w28-health` | `spindle-w28-health` |
| U5 | `story:events-record-their-caller`, `story:entity-event-history` | `unit/w28-history` | `spindle-w28-history` |
| U6 | `story:environment-binds-external-decisions` | `unit/w28-env-decisions` | `spindle-w28-env` |
| U7 | `story:one-time-response-admission`, `story:replay-with-decisions` | `unit/w28-replay` | `spindle-w28-replay` |

U4 and U5 each hold a story and the story that `depends_on` it; U7's two stories both rewrite
`Replays::run`. Each pair runs one after the other in one unit. Each unit is built by one
`aep:implementor` and gets one `aep:adversary` pass when green. Builds and tests run through the
shared pool; a step that writes into the tree runs as `slot --fetch <path>`.

## Surfaces

Each story's `## Scope` holds its surface. Shared between units, each on separate hunks and resolved
by the coordinator at merge:

- `crates/spindle-cli/src/cli.rs` (`run` flags) with the generated `website/docs/reference/cli.md`:
  U1 `--console`, U2 `--log-format`, U3 `--observe-series`, U4 `--drain`, U6 and U7 both the
  `--decisions` help (`:259-265`: U6 names the environment's `[decisions]` file and which wins, U7
  says it applies to `--scenario` too), U7 the `--scenario` conflicts (`:392`) and help
  (`:119-134`). U7 merges before U6, and U6 rewrites the `--decisions` help over U7's text.
- `crates/spindle-cli/src/main.rs`: `Command::Run` and `Execute` gain a field each from U1, U2, U3
  and U4; U4 `signals`; U6 the environment read before `decisions::load` (`:533-590`); U7 the
  `scenario` arm (`:190-197`); U2 the exit error in `main` (`:67`).
- `crates/spindle-cli/src/serve.rs`: U1 the `with_console` call (`:355`); U3 `surface.observing`
  (`:368`); U4 the seed handle, the loop and the drain before `Listener::close`; U2 every stderr
  line.
- `crates/spindle-cli/src/seed.rs`: U4 `Seeding` publishes its state; U5 `submitted` passes the
  entry's `request_id`; U2 every stderr line.
- `crates/spindle-ingress/src/observe.rs`: U3 `Ring` and `Recorder::new`; U4 `Reserved` and
  `reserved()`; U5 the request-log query elision (`:1070-1100`).
- `crates/spindle-ingress/src/surface.rs`: U3 a builder beside `observing`; U4 `with_seed`; U5 the
  console routes from `Console::ALL`; U1 doc text of `with_console` only.
- `crates/spindle-ingress/src/listener.rs`: U4 only. `crates/spindle-ingress/src/console.rs`: U5
  only (U1 does not change it: it skips `with_console`).
- `crates/spindle-ingress/src/router.rs`: U4 the match arms over `Reserved` (`:123-140`, `:342`,
  `:429`) for its new variants; U5 the console dispatch (`:299-346`, `:400-421`) for
  `/api/history`. U5 merges before U4.
- `website/docs/concepts/observe.md`: U3 the series bound of each mode (`:47-64`); U4 a new section
  on the seed, readiness and liveness routes, placed after the metrics section. U3 merges before U4.
- `website/docs/concepts/environment-and-admission.md`: U4 edits § A seed submitted at start
  (`:153-240`); U6 the `external.*` row (`:71`) and a new section placed after § A seed submitted at
  start ends, not inside it. U6 merges before U4.
- `crates/spindle-kernel/src`, `crates/spindle-host/src/host.rs`, `crates/spindle-store-browser`,
  `crates/spindle-ingress/src/answer.rs` (the request id minted per HTTP request): U5 only.
  `crates/spindle-capability` and `crates/spindle-kernel/tests/spindle-kernel/adversary_environment.rs`
  (the `Environment` literal): U6 only. `crates/spindle-cli/src/replay.rs` and
  `crates/spindle-conformance/src/target.rs`: U7 only.
- `crates/spindle-cli/tests/spindle-cli/seed.rs`: U4 the `mod` list at the end of the file
  (`:1225-1230`); U5 adds `a_seed_event_records_the_seed` in a new module file
  `tests/spindle-cli/seed/caller.rs`, so its only edit there is one `mod` line. U5 merges before U4.
- `ess/domains/observe.yaml`, `Taskfile.yml` and their generated outputs: U4 only (`SeedState`, the
  health answers). `ess/fixtures/observe/metrics/families.json`: U3 only.
- `crates/spindle-docs/src/status.rs` with `website/docs/status.md` and `website/data/status.json`
  (U4, U6, U7, separate rows), regenerated after the merges.
- `AGENTS.md` crate table, `README.md`, `CHANGELOG.md` `[Unreleased]` and the `mod` lines of the
  test `main.rs` files.

Merge order: U7, U6, U5, U3, U1, U4, U2. U2 merges last: before its merge its implementor merges
`wave/28` into `unit/w28-json-logs` and routes every stderr line the earlier units added on the
served path through the log writer.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run) and, where the
adversary found something, a `review_outcome`.

## Commits

The unit commits, the merges into `wave/28`, review fix commits, the closing store commit, one PR to
`main` through the bot, and a release on the host operator's word after the PR merges.
