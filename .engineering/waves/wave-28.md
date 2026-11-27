# Wave 28

**Proposed.** Planned by the coordinator on the host operator's order of 2026-11-27, relaying the
operator's words "resolve all missing spindle features". Skill `aep:implementing` 0.26.0, wave mode.
All stories serve `vision:spindle`.

Integration branch `wave/28`, forked from `plan/wave-28` (based on `main` at release 0.22.0). The
ESS pin 0.99.1 stays.

## Selection

The triage of the 98 open stories on 2026-11-27 put the gaps consumers reported first: running
Spindle as a production service (console off, JSON logs, bounded metrics, seed readiness, health
and drain), the history of one instance with the caller of each event, and decisions answered from
the environment and in replay. The host operator accepted this list for the operator on 2026-11-27.
Seven stories that change the architecture wait for the operator's picks and are not in this wave.
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
  U1 `--console`, U2 `--log-format`, U3 `--observe-series`, U4 `--drain`, U6 the `--decisions`
  help, U7 the `--scenario` conflicts (`:392`).
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
- `crates/spindle-ingress/src/listener.rs`: U4 only. `crates/spindle-ingress/src/console.rs` and
  `router.rs`: U5 only (U1 does not change them: it skips `with_console`).
- `crates/spindle-kernel`, `crates/spindle-host/src/host.rs`, `crates/spindle-store-browser`: U5
  only. `crates/spindle-capability`: U6 only. `crates/spindle-cli/src/replay.rs` and
  `crates/spindle-conformance/src/target.rs`: U7 only.
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
