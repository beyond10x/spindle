---
format: aep.planning-md/3
id: story:service-worker-host-matches-run
kind: story
status: draft
title: The Service Worker host does what a served run does
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:node-export-serves-a-specification
- depends_on: story:node-export-fixed-actor
revision: 1
---
# The Service Worker host does what a served run does

## Outcome

A Service Worker that opens Spindle (`crates/spindle-wasm`, `web/src/sw.ts`) answers as `run`
serving HTTP answers, where a browser can. On main 7bcc325ed it lacks:

- **Timers.** Nothing calls `Host::advance` (`crates/spindle-host/src/host.rs:1508`), so no
  periodic binding fires in the browser.
- **Seed.** The `Config` (`crates/spindle-wasm/src/lib.rs:41-57`) has no seed; the environment's
  `[seed]` is not submitted.
- **A reset that carries a world.** `fetch` routes with `spindle_ingress::route`, without a world
  reader, so `POST /api/reset` with a body answers `400` ("this run loads no world"); `run`
  loads rows, a decision table and a mint seed.
- **Observe.** The surface is not observing (`Surface::observing`), so `/__spindle/requests`,
  `/__spindle/metrics` and `/__spindle/ui` answer `404`.
- **Run options.** No fixed actor with callers (`--fixed-actor`, `--fixed-caller`), no
  `--drop-retries`, no `--scenario` replay.

The opening shared with the Node export (`crates/spindle-embed`,
`story:node-export-serves-a-specification`) reads worlds and the answer default already; this
story turns them on for the Service Worker and adds the rest.

## Acceptance

- `a_periodic_binding_fires_in_the_service_worker` — the worker advances the host on its clock;
  a periodic binding's event is listed at `GET /api/events` after its interval.
- `the_environment_seed_is_submitted_once` — a `seed` in the `Config` is submitted at open, its
  rows listed, not submitted again on a second open of the same databases.
- `a_reset_carries_a_world_in_the_service_worker` — the exchanges of
  `crates/spindle-node/tests/fixtures/world-exchanges.json` get their recorded answers.
- `the_observe_routes_answer_in_the_service_worker` — with `observe` in the `Config`,
  `GET /__spindle/requests` lists the answered requests and `GET /__spindle/metrics` counts them.
- `fixed_actors_and_dropped_retries_in_the_service_worker` — with `fixedActors` and
  `dropRetries` in the `Config`, a granted command is sent as the fixed actor and a failing
  retried binding is dropped, as `run` with the same options answers.
- `--scenario` replay stays out: a browser has no stdin and no scenario file; the story records
  that it is not served.

All in `task check:a_service_worker_serves_a_specification`, each test bounded at 60 s.

## Scope

- `crates/spindle-wasm/src/lib.rs`, `crates/spindle-wasm/src/worker.rs`,
  `crates/spindle-wasm/tests/spindle-wasm/`, `web/src/sw.ts` (inferred).
