---
format: aep.planning-md/3
id: story:control-process-serves-the-api
kind: story
status: active
title: The control process serves the API
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:telemetry-transport
- depends_on: story:launcher-resource-sampling
- depends_on: story:observe-api-contract
- depends_on: story:run-over-http-ingress
- depends_on: story:rally-runs-as-three-processes
- depends_on: story:observe-ui
scope:
- confidence: inferred
  path: .github/workflows/ci.yml
- confidence: cited
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-cli
- confidence: inferred
  path: crates/spindle-control
- confidence: cited
  path: ui
revision: 8
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
---
# The control process serves the API

## Outcome

`b10x-spindle deploy <spec> --environment <file> --placement <file>` runs `spindle-control`: it
places and launches the instances through each target's `Launcher`, collects their telemetry
frames and the launchers' host-side resource samples, keeps the last 15 minutes per instance, and
serves the read-only observe API and the UI on `127.0.0.1` with a token and a Host check. Design
`docs/design/observe.md` § 5.

It also serves the UI: the bundle `ui/` builds is embedded in `b10x-spindle` at compile time and
answered at `GET /`, so `deploy` needs nothing installed. This moved here from
`story:observe-ui`, which shipped the UI on fixtures under `pnpm dev` only (plan review,
2026-11-04).

## Acceptance

Each is a named test with the outcome it observes.

- `a_request_without_the_token_is_refused` — `GET /api/deployment` without `?t=` or with a wrong
  token is answered 4xx and carries no deployment.
- `a_request_with_a_foreign_host_header_is_refused` — `Host: example.test:<port>` is refused;
  `127.0.0.1:<port>` and `localhost:<port>` are answered.
- `the_listener_binds_loopback_only` — the control listener's local address is `127.0.0.1`.
- `the_deployment_answer_validates_against_its_schema` — `GET /api/deployment` validates against
  `spindle.observe.Deployment` and names every component, binding and instance of the placement.
- `the_stream_carries_one_sample_per_instance_per_period` — over three periods with two process
  instances, `GET /api/stream` carries six sample messages, each valid against
  `spindle.observe.Sample`.
- `a_state_change_is_streamed` — an instance that exits produces one `StateChange` message.
- `the_ui_is_served_from_the_binary` — `GET /?t=<token>` answers the built `index.html` from a
  binary built without `pnpm` on `PATH` at run time.
- `a_view_is_answered_through_the_instance_ingress` — `GET /api/views/<component>/<view>` returns
  the instance's own view answer, read only.
- `samples_older_than_fifteen_minutes_are_dropped` — on an injected clock, an instance's sample
  taken 15 minutes and one period before the newest is no longer held, and one taken within the
  window still is (design `docs/design/observe.md` § 5, "the last 15 minutes").
- `the_instance_screen_shows_one_instances_counters_and_quota` — a test in `ui/src/screens/`
  renders `InstanceScreen.vue` on the fixture samples and finds each command's rate and count and
  the CPU and memory share of the quota. Carried from `story:observe-ui`, whose instance screen
  has no named test on `main`.
- `the_view_screen_renders_a_declared_view_as_a_table` — a test in `ui/src/screens/` renders
  `ViewScreen.vue` on the fixture view answer (`ui/dev/views.json`) and finds one table row per
  answer row. Carried from `story:observe-ui` for the same reason.

## Scope

- `crates/spindle-cli` — cited: the `deploy` verb goes into `src/cli.rs` and `src/main.rs`.
- `crates/spindle-control` — inferred, new: launch, collect and serve, so the server code does
  not land in the CLI crate.
- `ui` — cited: the build output the binary embeds (`ui/vite.config.ts` writes `ui/dist`).
- `Taskfile.yml` — cited: `build` and `check` must build `ui/` before the crate that embeds it.
- `.github/workflows/ci.yml` — inferred: CI builds the UI before `cargo test` for the same reason.

Shared, not typed: `website/docs/reference/cli.md` (generated from `spindle_cli::cli`),
`crates/spindle-docs/src/status.rs`, `README.md`, `AGENTS.md`.

## Depends on

- `story:telemetry-transport` — frames to collect.
- `story:launcher-resource-sampling`, `story:observe-api-contract` — implemented.
- `story:observe-ui` — implemented; added after the round-1 critics, 2026-11-04:
  `the_ui_is_served_from_the_binary` embeds the bundle that story's `ui/` builds.
- `story:run-over-http-ingress` — added by the plan review: design § 5 answers
  `/api/views/<component>/<view>` "through the instance's own ingress".
- `story:rally-runs-as-three-processes` — added by the plan review: an instance the control
  process launches must run one component with its timer and TCP channels, and that per-component
  mode is what that story builds. Today `b10x-spindle run` reads JSON lines on stdin and `play`
  runs every component in one process.

## Shared with story:spindle-conforms-to-its-specification

`Taskfile.yml`: this story changes `build` and `check` to build `ui/` first; story:spindle-conforms-to-its-specification adds a `check:conformance` step to `check`. Different lines of one file: if both land in one wave, the wave coordinator resolves `Taskfile.yml` at integration, as it does `Cargo.lock`. The current order puts conformance first (its dependencies are earlier).
