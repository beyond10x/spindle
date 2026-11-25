---
format: aep.planning-md/3
id: story:node-export-serves-a-specification
kind: story
status: draft
title: Node loads the wasm host and serves a specification
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:service-worker-serves-a-specification
- informed_by: story:wasm-test-library
scope:
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/src/world.rs
- confidence: inferred
  path: crates/spindle-embed
- confidence: inferred
  path: crates/spindle-host/src/world.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-node
- confidence: inferred
  path: crates/spindle-wasm/src/worker.rs
- confidence: inferred
  path: examples/environments/node.toml
revision: 2
---
# Node loads the wasm host and serves a specification

## Outcome

A Node program (Node 22, ESM) loads Spindle's wasm host in-process, from the module's bytes, and
serves a specification from it: it creates an instance from the specification's sources, an
environment whose components keep their state in `memory`, a decisions file or the answer-default
rule and an optional `ess-http/1` binding, and hands it one request at a time. Each request is
answered by the router the native loopback listener calls (`spindle_ingress::route_with`), console
routes included, so a consumer can mount the instance in any Node HTTP server as an end-to-end fake
backend. Requested 2026-11-25 by a consumer, through the coordinating session.

The Service Worker host (`crates/spindle-wasm`) opens a host from the same texts; the opening
moves out of its wasm-bindgen gate into a transport-free crate both hosts call, so neither copies
the other.

## Acceptance

Each case runs in `task check:node_serves_a_specification`: the lane builds `spindle-node` for
`wasm32-unknown-unknown`, binds it with `wasm-bindgen --target web`, and runs
`crates/spindle-node/tests/node/*.test.mjs` under `node --test`, each test bounded at 60 s.

1. `answers the recorded exchanges as the loopback listener does` — the module is loaded from its
   bytes (`initSync`), an instance is created from the review fixture
   (`crates/spindle-wasm/tests/fixtures/review`), `examples/environments/node.toml` and the
   fixture's decisions file, and every exchange of
   `crates/spindle-wasm/tests/fixtures/review/exchanges.json` gets the status, `Allow` header and
   body the loopback listener gives natively
   (`tests/spindle-wasm/listener.rs::the_loopback_listener_answers_the_recorded_exchanges`).
2. `a reset carries a world as the native reset route does` — the exchanges of
   `crates/spindle-node/tests/fixtures/world-exchanges.json` (a reset whose world holds 3
   `Submission` rows, the view listing them, a world with an undeclared entity refused `400`, a
   reset without a world, the view empty) are answered as a native loopback listener answers them
   with `run`'s world reader (`tests/spindle-node/listener.rs`, a native test). `reset(world?)`
   answers what `POST /api/reset` with that body answers.
3. `decisions and the answer default answer external outcomes as run does` — with `decisions` the
   review fixture's `Decide` is vetoed (`502`), as `run --decisions`; with `answerDefault: true`
   and no decisions file each external outcome takes its command's default branch, as
   `run --answer-default`: item 2's instance is created that way, and `world-exchanges.json`
   holds a `Submit` and a `Decide` answered by the default branches; with neither,
   `create` refuses a specification that declares an external outcome, with the reason `run`
   gives.
4. `create returns the not-served report` — for
   `crates/spindle-cli/tests/fixtures/outcome-scoped-refusal` with its `ess-http/1` binding,
   `notServed()` lists, in order, the lines `run` prints on stderr before listening (without the
   `b10x-spindle: ` prefix): each operation not served and each outcome answered `501` with its
   reason. Both come from one function (`Surface::report`), which `run` prints.
5. `create refuses what run refuses before listening` — each refusal is a thrown `Error` whose
   message names the reason: configuration not of the shape below; a specification that does not
   compile; an environment that does not parse or does not admit every component; a component
   whose state is bound to anything but `memory`, naming the component and the implementation; a
   component with `ingress.network` bound to anything but `node`; a decisions file or binding
   refused; a surface that serves no operation.
6. `two instances share nothing and a disposed one refuses` — a write in one instance is not
   seen by a second in the same process; after `dispose()` every call throws, naming the
   disposal.
7. `requests are answered one at a time in arrival order` — requests handed over without waiting
   for each answer are answered in the order handed over (a write, then a view that sees it).
8. `the module reads no file and opens no socket` — the generated glue imports no Node built-in
   module; the bytes are handed to it.

The new crate passes `cargo clippy` with the wasm32 configuration (`clippy/wasm32/clippy.toml`,
std's clock banned) in `check:the_wasm_build_of_the_host_succeeds`, and CI's `Gate` runs the lane
through `task check` (Node 22 and `wasm-bindgen` 0.2.130 are installed there already).

## Design

- **Crates.** `crates/spindle-embed` (new, builds native and wasm32): opens a host from texts —
  compile, admit, decisions and answer default, binding, surface with the console routes, the
  host on supplied stores — and maps a routed response to its headers and body (the null-body
  rule included). `crates/spindle-wasm` (Service Worker) and `crates/spindle-node` (new cdylib,
  wasm32 only) both call it; the Service Worker keeps its `Config`, its `open`/`serves`/`fetch`
  and its answers. `run`'s world reader moves from `spindle-cli` into `spindle-host`
  (`spindle_host::read_world`), and the not-served lines `run` prints into `spindle-ingress`
  (`Surface::report`); `run` calls both.
- **Binding.** `wasm-bindgen --target web`: an ES module that loads on Node 22 with
  `initSync({ module: bytes })` or `await init({ module_or_path: bytes })`, importing no `fs`;
  `--target nodejs` reads the file with `fs` itself and is CommonJS, and
  `experimental-nodejs-module` imports the `.wasm` as a module, which Node 22 does not do without
  a flag.
- **Surface.**
  - `Spindle.create(config)` → `Spindle`, or throws `Error(reason)`. `config` is a JSON string or
    a plain object: `{ sources: [[name, text], ...], environment: <TOML>, decisions?: <TOML>,
    answerDefault?: boolean, binding?: <YAML or JSON> }`; unknown keys are refused.
  - `spindle.notServed()` → `string[]`, the report of item 4.
  - `spindle.handle({ method, path, query?, headers?, body? })` → `Promise<{ status, headers,
    body }>`. `path` may carry `?query`; `query` (without `?`) wins where given. `headers` is a
    plain object or an array of `[name, value]`; only `content-type` is read. `body` is a string
    or a `Uint8Array`. The answer's `headers` is an array of `[name, value]` (lower-case names, the
    content type, `allow`, the binding's declared headers) and `body` a `Uint8Array`, empty for a
    null-body status. A request the instance cannot read (no method, no path) rejects with an
    `Error`; every routed refusal is an answer with its status.
  - `spindle.reset(world?)` → `Promise` of the answer `POST /api/reset` gives with `world` as
    its JSON body (no body without one).
  - `spindle.dispose()`: drops the host; later calls throw `Error("this Spindle was disposed")`.
    `free()` (wasm-bindgen's) also releases the memory.
- **One at a time.** Each call runs to its end before it returns its promise, so requests are
  answered in the order they arrive and never overlap; the promise is the shape a later
  asynchronous answer (an external call) keeps.
- **Environment.** `ingress.network` is bound to `node` (`examples/environments/node.toml`),
  every store to `memory`; the console routes are served as on a loopback listener.

## Follow-ups (separate drafts, not this story)

- `story:node-export-fixed-actor` — fixed actors with their callers in the configuration.
- `story:node-export-clock-and-seeds` — a clock start, set and advance (or a `now()` callback),
  timers fired through `Host::advance`, and seeded minting and randomness, so a run repeats.
- `story:node-export-events` — event delivery to the host program, by callback or pull.
- `story:node-export-release-artifact` — the module built in the release workflow as an
  artifact, its size in the release notes.

## Scope

- `crates/spindle-embed/` — new.
- `crates/spindle-node/` — new: `src/lib.rs`, `tests/node/*.test.mjs`, `tests/fixtures/`,
  `tests/spindle-node/` (native recording tests).
- `crates/spindle-wasm/src/worker.rs` — the opening and response mapping call `spindle-embed`.
- `crates/spindle-host/src/world.rs` (new), `crates/spindle-cli/src/world.rs`,
  `crates/spindle-cli/src/serve.rs`, `crates/spindle-ingress/src/surface.rs`.
- `Taskfile.yml`, `examples/environments/node.toml`, `README.md`, `AGENTS.md`, `website/docs/`,
  `CHANGELOG.md`, `Cargo.lock`.
