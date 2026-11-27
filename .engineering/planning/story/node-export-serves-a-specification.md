---
format: aep.planning-md/3
id: story:node-export-serves-a-specification
kind: story
status: implemented
title: Node loads the wasm host and serves a specification
relations:
- serves: vision:spindle
- depends_on: story:service-worker-serves-a-specification
- informed_by: story:wasm-test-library
- decomposes: epic:node-export
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/src/world.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/outcome_scoped_refusal.rs
- confidence: inferred
  path: crates/spindle-embed
- confidence: inferred
  path: crates/spindle-host/Cargo.toml
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
- confidence: inferred
  path: website/docs
revision: 12
transitions:
- {from: "draft", to: "proposed", at: "2026-11-25T02:59:15Z", actor: "human:operator", revision: 10, decided_on: {"recorded":{"review_outcome":6}}}
- {from: "proposed", to: "active", at: "2026-11-25T02:59:15Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":6}}}
- {from: "active", to: "implemented", at: "2026-11-27T07:11:14Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"test_result":1,"review_outcome":6}}}
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

`story:wasm-test-library` stays as drafted: a TypeScript library that takes capabilities as
bindings supplied from TypeScript. This story serves a specification over its routes and takes no
capability from JavaScript.

The Service Worker host (`crates/spindle-wasm`) opens a host from the same texts; the opening
moves out of its wasm-bindgen gate into a transport-free crate both hosts call, so neither copies
the other.

## Acceptance

Each case runs in `task check:node_serves_a_specification`: the lane builds `spindle-node` for
`wasm32-unknown-unknown`, binds it with `wasm-bindgen --target web`, and runs
`crates/spindle-node/tests/node/*.test.mjs` under `node --test`, each test bounded at 60 s. The
tests load the module from the directory `SPINDLE_NODE_MODULE` names, the lane's own output
(`target/spindle-node`) without it, so a released archive can be tested the same way.

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
3. External outcomes, two tests (the refusal with neither source is item 5's
   `refuses external outcomes nothing answers`):
   - `the decisions file answers external outcomes as run --decisions does` — item 1's instance:
     its third exchange, `Decide`, is vetoed (`502`) as the decisions file says.
   - `the answer default answers external outcomes as run --answer-default does` — item 2's
     instance is created with `answerDefault: true` and no decisions file;
     `world-exchanges.json` opens with a `Submit` its default branch answers (`202`) and a
     `Decide`, which declares no default branch and is refused `501` naming that, both recorded
     natively with `--answer-default`'s answers.
4. `create returns the not-served report` — for
   `crates/spindle-cli/tests/fixtures/outcome-scoped-refusal` with its `ess-http/1` binding,
   `notServed()` equals `crates/spindle-node/tests/fixtures/not-served.json`: the lines
   `b10x-spindle run` prints on stderr before listening, in order, without the `b10x-spindle: `
   prefix. That file is recorded from `run` itself: the native test
   `tests/spindle-cli/outcome_scoped_refusal.rs::run_prints_the_recorded_not_served_lines` starts
   `b10x-spindle run` on the same fixture and binding and compares its stderr lines with the file.
5. `create` refuses each of the following with a thrown `Error`, one test each. Where a bullet
   gives a whole message, the test compares the whole message; where it says "starts with", the
   rest is a parser's text, and the test compares the prefix. `(R)` marks a reason taken from the
   code `run` runs, so the text is `run`'s.
   - `refuses a configuration with an unknown key` — `{..., "port": 1}`: starts with
     `the configuration: unknown field \`port\``.
   - `refuses a specification that does not compile` — the review fixture's `system.yaml` alone:
     starts with `the specification: `.
   - `refuses an environment that does not parse` — `environment: "not toml ="`: starts with
     `the environment: `.
   - `refuses an environment that does not admit a component` — `examples/environments/node.toml`
     without `"external.*"`: the whole admission refusal `spindle_host::admit` writes (R).
   - `refuses state not bound to memory` — `"store.durable" = ["sqlite"]`, whole message:
     `probe has store.durable bound to \`sqlite\`; the Node export keeps state in \`memory\` only`.
   - `refuses an ingress other than node` — `"ingress.network" = ["http-loopback"]`, whole
     message: `probe has ingress.network bound to \`http-loopback\`; the Node export serves
     \`node\` only`.
   - `refuses a decisions file it cannot read` — `decisions: "[nope"`: starts with
     `the decisions file: `, the rest `Answers::parse`'s text (R).
   - `refuses external outcomes nothing answers` — the review fixture with neither `decisions`
     nor `answerDefault`: the whole refusal `Host::supplied` gives without a decider (R).
   - `refuses a surface that serves no operation` —
     `crates/spindle-cli/tests/fixtures/unservable`, whole message: the report's lines, then
     `<component> serves no operation: ...`, the lines and the reason `run` prints (R, functions
     in `spindle-ingress` that `run` calls).
6. `two instances share nothing` — a write in one instance is not seen by a second in the same
   process.
7. `a disposed instance refuses every call` — after `dispose()`, `handle`, `reset`, `notServed`
   and `dispose` throw `Error("this Spindle was disposed")`.
8. `requests are answered one at a time in arrival order` — requests handed over without waiting
   for each answer are answered in the order handed over (a write, then a view that sees it).
9. `the module reads no file, opens no socket and starts no thread` — the generated glue imports
   no Node built-in module and names no `Worker`, `worker_threads`, `SharedArrayBuffer` or
   `Atomics`; the `.wasm` imports no shared memory; the bytes are handed to it.

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
  (`Surface::report`, with `Surface::serves_nothing` for the refusal of a surface that serves no
  operation); `run` calls them.
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
  After the fixed-actor story: both extend the configuration and `crates/spindle-node/src/lib.rs`.
- `story:node-export-events` — event delivery to the host program, by callback or pull. After the
  clock story, whose host-driving path delivery hooks into.
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

Ordering: a branch upgrading the ESS pins (`Cargo.toml`, `Cargo.lock`, spec, kernel and
conformance code) runs beside this one. This story changes no ESS pin; its `Cargo.lock` change is
the two new crates' entries only, and it rebases onto that branch once it merges and runs its
lanes again before it lands.
