---
format: aep.planning-md/3
id: story:served-clock-steps
kind: story
status: implemented
title: A served run steps its clock
relations:
- serves: vision:spindle
- informed_by: story:clock-and-mint-in-every-mode
- depends_on: story:events-cursor-across-restart
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/clock_and_mint.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-embed/src/lib.rs
- confidence: inferred
  path: crates/spindle-embed/tests
- confidence: inferred
  path: crates/spindle-embed/tests/main.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/host/range.rs
- confidence: cited
  path: crates/spindle-host/src/world.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/adversary_concurrent_reads.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/adversary_rows.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/concurrent_reads.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/console.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/rows.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: inferred
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/router.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/router.rs
- confidence: cited
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/capabilities.md
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: cited
  path: website/docs/status.md
revision: 20
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T10:45:05Z", actor: "human:operator", revision: 18, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-27T10:45:05Z", actor: "human:operator", revision: 19, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "active", to: "implemented", at: "2026-11-27T17:04:24Z", actor: "human:operator", revision: 20, decided_on: {"recorded":{"test_result":1,"review_outcome":8}}}
---
# A served run steps its clock

## Outcome

A `run` serving HTTP with `--clock` answers `POST /api/clock` `{"at": "<RFC 3339>"}` by moving its
manual clock forward to that instant between commands and answering `{"at"}`, guarded by the proxy
secret as every `/api/*` route. An instant before the clock's current one is refused `409` and
leaves the clock where it stood; without `--clock` both methods of the route answer `409`, and so
does a reset whose world carries `clock`, which then empties nothing; the same holds on every host
without a stepping clock, an embedded host and a Node instance included, since each reads a world
through `spindle_host::read_world`. `GET /api/clock` answers
the current instant. A reset world may carry `"clock": "<RFC 3339>"` to move the clock forward
there; an earlier instant refuses the reset `400` before anything is emptied, because the host's
command sequence, on which event times and both cursors rest, never goes back. Every record of
`GET /api/events` carries `at`, the clock instant its command ran, as `GET /api/history` already
does. Today `--clock` stands at one instant for the whole served run
(`website/docs/reference/cli.md`, `--clock`), and only a stdin line's `"at"` moves it.

`/__spindle/requests` keeps its `at`, the wall-clock time a request's head was read: it is a
published field of `spindle.observe.RequestRecord`, and changing it or adding one is a change of a
published format that this story does not make (decision D4 A).

## Acceptance

- `a_served_clock_moves_forward_and_a_guard_reads_it` (`clock_and_mint.rs`, fixture
  `clock-served`): on a served run with `--clock 2026-02-01T00:00:00Z` a booking answers `early`;
  `POST /api/clock` to 2026-07-01 answers `{"at": "2026-07-01T00:00:00Z"}`, and the next booking
  answers `late`.
- `a_served_clock_refuses_to_move_back`: a later `POST /api/clock` to an earlier instant answers
  `409`, and `GET /api/clock` answers the instant before it.
- `a_served_run_without_clock_refuses_to_step`: without `--clock`, `POST /api/clock` and
  `GET /api/clock` each answer `409`, and a reset whose world carries `clock` answers `409` and
  empties nothing.
- `a_reset_world_moves_the_clock`: on a `--clock` run, a reset whose world carries a later `clock`
  moves the clock there; one with an earlier `clock` answers `400` and empties nothing.
- `every_event_record_carries_at`: on a `--clock 2026-02-01T00:00:00Z` run, one command, then
  `POST /api/clock` to 2026-07-01, then a second command: in `GET /api/events`, unpaged and on an
  `after=` page, the first command's events carry `at` 2026-02-01T00:00:00Z and the second's
  2026-07-01T00:00:00Z, to the millisecond.
- `the_clock_route_needs_the_proxy_secret`: on a `--trusted-caller-header` run, the route without
  the secret answers `403` and moves nothing.
- `get_and_post_share_the_clock_path` (`router.rs`): both methods resolve, another method answers
  `405` with `Allow: GET, POST`, and a specification route at `/api/clock` is refused at start as
  shadowed.
- `a_published_event_carries_its_command_time` (`spindle-host` `console.rs`): one command on a host
  opened on `ManualClock::at(T)` publishes events whose `at` is `T` to the millisecond.
- `an_embedded_host_refuses_a_reset_world_carrying_clock` (`spindle-embed` tests): a reset whose
  world carries `clock` on an embedded host answers `409` and empties nothing.

## Not covered

- Periodic bindings: no served run fires them, and moving the clock fires none.

## Consumer gap

Recorded 2026-11-27. A consumer drives a multi-month simulation over HTTP and needs commands whose
guards read `now` to see the month it simulates, and an event time from Spindle rather than from
its driver. Spindle 0.22.0 fixes the clock of a served run at its start instant.

## Scope

Derived 2026-11-27 by `story-scoper`, against `plan/wave-30` at c65b744e. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong). Assumes D1 A, D2 A, D3 A, D4 A, D5 A.

- **Primary surface:** `crates/spindle-ingress` console routes and `crates/spindle-host` (a clock the host can step, `at` on each published event) — cited
- **Files:** `crates/spindle-ingress/src/console.rs` (`Console` :17-61 with `ALL: [Self; 5]` :35 and one `method()` per variant :44-49; `listed` :457-474 writes `/api/events` without `at`; `reset` :521-552; `/api/history` already writes `at` :395-424) — cited
- **Files:** `crates/spindle-ingress/src/surface.rs` (`resolve` :1662-1671 finds the first console route by path, then compares one method, so a GET and a POST at `/api/clock` need a lookup by path and method and `WrongMethod` listing both; `with_console` shadow check :1554-1566) — cited
- **Files:** `crates/spindle-ingress/src/router.rs` (`reads` :295-306, `read` :312-354, `answer` :379-450: the new variants are answered on the host thread, never read beside it) — cited
- **Files:** `crates/spindle-host/src/host.rs` (`Published` :157-166 gains `at`; `merged` :2404-2428 sets it from `Origin::sequence` through `civil` :2635-2638, as `Revision::at` is; `World` :123-143 gains `clock`; `reset_to` :1499-1545 checks it before anything is emptied; a stepping handle beside `clock` :771, opened like `Host::seeded`) — cited
- **Files:** `crates/spindle-host/src/host/range.rs` (`Published` built at :316-320 inside `range`) — cited
- **Files:** `crates/spindle-host/src/world.rs` (`Body` :34-42 is `deny_unknown_fields`, so `clock` is added there; `read_world` :65-91; module doc :1-22) — cited
- **Files:** `crates/spindle-cli/src/serve.rs` (`Serving::clock` doc :139-141 "never moved"; clock opened :457-465, where the `ManualClock` is handed to the host as its stepping handle) — cited
- **Files:** `crates/spindle-cli/src/cli.rs` (`--clock` doc :453-464 "stands at the instant for the whole run"; reset world doc :231-238; console route list :281) and the generated `website/docs/reference/cli.md` (:117, :134, :146) — cited
- **Symbols:** `Console`, `Console::ALL`, `Surface::resolve`, `Resolved::WrongMethod`, `Pending::answer`, `Published`, `merged`, `civil`, `following`, `World`, `read_world`, `Host::reset_to`, `ManualClock::advance`, `Serving::clock` — cited
- **Tests:** in `crates/spindle-cli/tests/spindle-cli/clock_and_mint.rs`, on fixture `clock-served`, no new module: `a_served_clock_moves_forward_and_a_guard_reads_it` (`--clock 2026-02-01`: `early`; `POST /api/clock` 2026-07-01 answers `{"at"}`; the next booking is `late`), `a_served_clock_refuses_to_move_back` (409, `GET /api/clock` unchanged), `a_served_run_without_clock_refuses_to_step` (409 for both methods and for a reset world carrying `clock`), `a_reset_world_moves_the_clock` (forward accepted; earlier refused 400, nothing emptied), `every_event_record_carries_at` (unpaged and `after=` page), `the_clock_route_needs_the_proxy_secret` (403, clock unchanged; setup as `adversary_trusted_caller.rs:75`). Each starts one or two `run` processes; each must stay under 60 s and the test step under 120 s; the implementor times each and names any over 5 s — inferred
- **Tests:** `crates/spindle-ingress/tests/spindle-ingress/router.rs` `get_and_post_share_the_clock_path` (both methods resolve; PUT answers 405 with `Allow: GET, POST`; a spec route at `/api/clock` is refused as shadowed) — inferred
- **Tests:** `crates/spindle-host/tests/spindle-host/console.rs` `a_published_event_carries_its_command_time` (`ManualClock::at`, one command, `published()[0].at`) — inferred
- **Documents:** `README.md` :72 (console route list), `crates/spindle-ingress/src/lib.rs` :43-50 (module doc), `website/docs/concepts/capabilities.md` :123, `crates/spindle-docs/src/status.rs` :473-476 (the generated status pages follow), `CHANGELOG.md` `[Unreleased]` — inferred
- **Not touched:** `ess/domains/observe.yaml`, `crates/spindle-observe-api/types.rs`, `ui/src/api/generated/source.schema.json`, `crates/spindle-ingress/src/observe.rs`, `crates/spindle-timer`, the stdin loop in `crates/spindle-cli/src/main.rs` — inferred, true under D4 A and D2 A
- **Confidence:** medium — every code site was read; the shape depends on D1 to D4, and the drafted acceptance was stale on three points
- **Would collide with:** `spindle-host/src/host/range.rs` `range` (wave 30 U7, the events cursor); `spindle-host/src/host.rs` (wave 29; wave 30 U1, U6 and U7 on other hunks; the stepping handle, if built beside `Host::seeded` `:1235-1268`, sits next to U6's `establish` `:1270-1281`); `spindle-host/src/world.rs` with U6 (`seed` `:115`; U12 `Body` `:34-42` and `read_world` `:65-91`); `spindle-cli/src/cli.rs` with the generated `reference/cli.md` (wave 29 and wave 30 U9); `README.md` (U3) and `CHANGELOG.md` `[Unreleased]`
- **Shares code with:** `story:node-export-clock-and-seeds` (a host-held clock its program steps); neither depends on the other — inferred
- **depends_on:** `story:events-cursor-across-restart`, which rewrites `range::range` where `Published` is built (`range.rs:316`) — inferred
- **Safety fact:** every console route is refused without the proxy secret where the surface proves callers (`listener.rs:667`, `matches!(asked, Asked::Console(_)) && unproxied`), so a new `Console` variant inherits the guard without new code. Step 2, unproven
- **Safety fact:** a specification that declares a route at `/api/clock` is refused at start once the route exists (`surface.rs:1554-1566`, `SurfaceError::Shadowed`), on every surface with the console, the embedded ones too (`spindle-embed/src/lib.rs:175`). Step 2, unproven
- **Safety fact:** `at` comes from the command's sequence, `max(clock µs, last + 1)` (`host.rs:2380-2382`); on a clock standing still, the n-th command at one instant reads up to n−1 µs past it, shown to the millisecond. Step 3, unproven
- **Architecture:** none under D1 A to D4 A: a new route on the existing console, an optional key on the reset world, an extra field on the `/api/events` JSON, which `observe.yaml` does not declare; no new crate, process, wire format or dependency — inferred
- **Also changed:** the `World { rows, answers, mint }` struct literals of the `spindle-host` tests
  `concurrent_reads.rs:132`, `rows.rs:246`, `adversary_rows.rs:276` and
  `adversary_concurrent_reads.rs:78` gain `clock`; the generated `website/docs/status.md` and
  `website/data/status.json` follow `status.rs` — cited
- **Tests (embed, new):** `crates/spindle-embed/tests/main.rs`, the crate's first and only integration-test binary, with `an_embedded_host_refuses_a_reset_world_carrying_clock` — inferred

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: `serve.rs` hands its `ManualClock` to the host, which answers `GET` and `POST /api/clock`
  on its own thread; an embedded host without that clock answers `409`.
- D2 A: a reset world's `clock` only moves the clock forward; an earlier one is refused `400`
  before anything is emptied.
- D3 A: an event's `at` comes from its `Origin::sequence`, as `/api/history` reads it.
- D4 A: `/__spindle/requests` is unchanged; a command's time is read from `/api/events` or
  `/api/history`. B (a new field of `RequestRecord`) and C (`at` reads the clock) change a published
  format and wait for the operator if a consumer needs them.
- D5 A: the `clock-served` fixture is reused (`early` becomes `late` after a step).
