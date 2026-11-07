# Changelog

This file follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and Spindle
follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Bare tags such as `0.1.0`
are Spindle's releases.

## [Unreleased]

### Added

- A refusing outcome an `ess-http/1` binding answers with an error object, `body: {<member>:
  error}`, is served: at the bound status, an object whose `<member>` is the error's wire name,
  beside the error's fields under their wire names, as `ess generate --kind openapi --http <file>`
  publishes it, and nothing is written. Before, such an outcome answered `501`.

## [0.5.0] - 2026-11-07

### Added

- A Service Worker serves a specification: `spindle-wasm` compiles it in the browser, admits it on
  a browser environment and answers its routes and the console routes (`GET /api/model`,
  `GET /api/events`, `POST /api/reset`) through the router the loopback listener calls, so the
  answers are the listener's and no request leaves the browser. A component whose state is bound
  to `indexeddb` keeps it in a `BrowserStore` of its own, every write landed before the answer;
  external outcomes are answered from a decisions file. `examples/environments/browser.toml` binds
  `ingress.network` to `service-worker` and `external.*` to `decisions-file`. `web/` is the static
  site, a Vue shell that registers the worker; `task web` builds it, and `task check` builds it
  and runs the Service Worker in a headless browser.
- `Host::supplied` opens a host on stores it is given, by component, for state bound to an
  implementation the host has no store of its own for; a reset removes every instance they hold.
  The decisions file is read by `spindle_host::Answers`, which `run` now uses too.
- `run --observe [--observe-requests <N>]` records every request its listener answers outside
  `/__spindle/`, `404`, `405` and `400` included, and answers `GET /__spindle/requests` with the
  totals per class and the last N records, newest first; `?class=` filters them. The reserved
  prefix is matched before any specification route, and a specification route whose first literal
  segment is `__spindle` is refused at start.
- The kernel evaluates sets, payload and error values read from the row an input field names, read
  once per command before the outcome. References stored on the subject, chains and lists stay
  refused by name.
- Several `BrowserStore` handles on one database, in one page or across tabs and workers: handles
  in one scope share one copy, every write re-reads its record inside its IndexedDB transaction
  and aborts on a mismatch, and landed writes reach handles in other scopes over a
  `BroadcastChannel`. Across real tabs a stale write is refused on the next flush or call, not on
  the write itself.

### Changed

- ESS 0.89.0: every ESS crate, `schema-contract` included, and CI's `ess` CLI move to tag 0.89.0.

### Fixed

- The link refusal tests wait for the refusals they assert, bounded by 10 s, instead of a fixed
  300 ms.

## [0.4.0] - 2026-11-07

### Added

- A view's `List<T>` parameter that the path does not carry is read from the query string as ESS
  publishes it: the key repeated once per element (`tags=a&tags=b`), each element read as `T`,
  required unless the parameter is `Optional`. `tags=a,b` is the one element `a,b`. An empty
  element, an element of the wrong type and a required list left out answer `400` naming the
  parameter. The host reads a filter that quantifies over a list parameter with `exists` or
  `forall`, its element compared by `==` or `!=` with the state, the identity or a field.
  Before, such a view answered `501`.
- A view's `Decimal` parameter, scalar or list element, is read from the query string as the
  number its decimal text spells (`amount=3.25`), so the view answers the rows ESS's interpreter
  answers; text that is not a decimal literal answers `400` naming the parameter. Before, every
  such read answered `400`.
- A command outcome an `ess-http/1` binding answers with `body: response`, `body:
  response.<field>` or `body: none` is served: at the bound status, the command's response bare
  as `application/json`, the value of the one field bare, or no body and no `Content-Type`, as
  `ess generate --kind openapi --http <file>` publishes it. Before, such an outcome answered `501`.
  An outcome bound with headers, a redirect, the `text` encoding or an error object still answers
  `501` when a request selects it, now naming which of those it is.
- `spindle.observe` specifies the request log (`RequestLog`, format `spindle-requests/1`), the
  metric families (`MetricFamilies`) and the topology (`Topology`, format `spindle-topology/1`) a
  served run will answer under `/__spindle/`, projected to JSON Schema, `spindle-observe-api` and
  the UI's TypeScript types, with fixtures checked against their schemas. No route serves them yet.

### Changed

- The command's response in an HTTP answer is written as ESS publishes it: each field under its
  wire name (`naming: {wire: …}`), through struct members, list items, map values, newtypes and a
  union's payload, and each enum variant in its wire spelling. That holds for the envelope's
  `response` and for a `body: response` or `body: response.<field>` answer alike. Before, the
  envelope's `response` keyed each field by its declared name.
- An outcome the runtime does not run refuses only the request that selects it. A command with
  one such outcome (a value it would generate of a type the runtime does not mint, an identity it
  cannot mint, a response field it does not choose, an error field nothing fills, a refusal that
  changes its row (`compensates: true`), secondary effects (`affects:`)) keeps its HTTP route and
  answers every other outcome as declared; the request that selects that outcome answers `501`
  naming the outcome and what it lacks, and nothing is minted or written. `run` reading stdin
  refuses it the same way, as an `unsupported` failure line. An outcome an `ess-http/1` binding
  answers other than with its status and the envelope is refused the same way. `admit`, `run`'s
  startup and `run` reading stdin name each such outcome on stderr. A construct selection reads
  (a condition, an outcome's shape) still refuses the whole command, and so does a command none of
  whose outcomes runs; `admit` and `run` reading stdin name such a command once on stderr as not
  served, as `run` over HTTP does. New: `spindle_kernel::selects`, `spindle_kernel::takes`,
  `spindle_kernel::execute_refusing`, `spindle_host::Host::submit_refusing`,
  `spindle_ingress::Route::refused`.

## [0.3.0] - 2026-11-06

### Added

- `spindle_ingress::route` answers a request value on a host with a response value, with no
  listener, socket or async runtime; the loopback listener answers through it. Without the default
  `listener` feature `spindle-ingress` builds for `wasm32-unknown-unknown`, and the wasm build
  check covers it.
- `spindle-store-browser`: a `Store` on the browser's IndexedDB, `store.durable` bound to
  `indexeddb` (`examples/environments/browser.toml`). It answers versions, conflicts and removed
  instances as the memory store does, writes each commit in one IndexedDB transaction, and its
  state is read back after the database is reopened. `task check` runs its tests in a headless
  browser. The host does not open it yet.
- `spindle_kernel::contract`: the cases every `Store` passes, run on the memory, SQLite and
  IndexedDB stores.
- `b10x-spindle` builds for macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`), and each
  release carries both archives in `SHA256SUMS`. The in-process commands run there as on Linux;
  `deploy`'s telemetry peer check reads `LOCAL_PEERPID`, and the longest telemetry socket path
  follows the platform's `sun_path` (103 bytes on macOS). The microVM launchers, vsock and the
  `/proc` resource readings answer `Unsupported` off Linux, and a process instance gets no
  parent-death signal there. CI checks and lints both macOS targets and runs the in-process test
  lanes on a macOS runner; a release publishes only once that job is green on the tagged commit.

### Fixed

- A SQLite conflict names the version the change named (none for a creation) and no held version
  for an instance never written, as the memory store does, instead of eventlog's stream positions.

### Changed

- Each crate's integration tests build into one test binary, `tests/<crate>/main.rs` with one
  module per former `tests/*.rs` file; test names are unchanged under their module path. Two
  tests that hold process-wide state keep their own binary: telemetry's allocation test and
  QEMU's context-id test.

## [0.2.0] - 2026-11-06

### Added

- Outcomes that `deletes:` their subject run: the memory store keeps the removed version, SQLite
  appends a `spindle.removed` event, and a later write to the removed instance is a conflict. A
  `DELETE` route runs a deleting outcome. Set deletions (`instances: where`) stay refused.
- Response fields of record, union, map, list, enum and constrained newtype type answer a value of
  that type; one rule decides the answer, the refusal and the derived capabilities.
- `--fixed-actor` may repeat: each command runs as the first fixed actor whose grant names it, a
  view is read as the first that may read it, and anything no fixed actor may run answers 403 and
  writes nothing. The n-th `--fixed-caller` pairs with the n-th `--fixed-actor`.

### Changed

- A refusal answers every error field it declares: a field with no source is read from the refused
  row, left absent when optional, minted from its type, or refused by name before the command
  runs. Capability derivation requires `identity.mint` or `clock` for such fields.
- A `wrong_state` refusal for an identity with no record answers no payload, as the published
  schema describes.
- A view the ingress cannot route (a `List` parameter in the query string, a body other than
  `rows`) answers 501 naming the construct and is listed at startup, instead of refusing startup.
- ESS moved to 0.88.0.

## [0.1.0] - 2026-11-06

The first release: `b10x-spindle` loads an ESS specification, derives the capabilities it needs,
admits it against an environment, and runs it in one process, as processes joined by TCP channels,
or deployed from a placement with a live observe API and UI.

### Added

- Served views take parameters from the path or the query string, typed and checked as ESS's
  served application reads them; a read-granted view checks its reader before reading the request.
- Commands accept `form`- and `query`-encoded requests as their HTTP binding declares.
- Guards over one related row (`when_related:`), including external branches in the look-ahead.
- `spindle-spec`, `spindle-kernel` and `spindle-host` build for `wasm32-unknown-unknown` with
  `--no-default-features`; a command runs there on the memory store. SQLite is the default-on
  `sqlite` feature.
- A release process: `release.yml` builds `b10x-spindle` for Linux (x86_64 and aarch64) on a
  pushed `X.Y.Z` tag and attaches `b10x-spindle-X.Y.Z-<target>.tar.gz` archives and `SHA256SUMS`
  to the bot-created GitHub release. macOS builds are not released yet: the host crates use
  Linux-only APIs (`accept4` and `SOCK_CLOEXEC` in `spindle-telemetry`, the parent-death signal in
  `spindle-target`).
- `b10x-spindle deploy`: place and launch every instance of a `spindle-placement/1` document as
  processes, and serve the read-only observe API and the observe UI built into the binary on
  127.0.0.1.
- `b10x-spindle run` serves a network-reached component over HTTP on 127.0.0.1: routes, command
  responses, read-granted views, a caller fixed at start and a world loaded per reset.
- `b10x-spindle conform`: run the conformance suite ESS synthesises from a specification against
  every component in one process, with known failures and counts held by `task check`.
- A documentation site, built with Docusaurus under `website/` and published at
  <https://beyond10x.github.io/spindle/>: overview, getting started, concepts, the rally example,
  the CLI reference and the status page; and a logo.
- `spindle-docs` generates the CLI reference from the clap definitions and the status page from a
  capability list whose shipped items each name the test that holds them; `task check` fails on
  drift.
- `b10x-spindle metrics`: the counters and histograms one component reports, derived from the
  specification (`spindle-telemetry`); `play` prints the table's non-zero counters under the grid.
- `b10x-spindle run --decisions <file>` and `--answer-default`: answer external outcomes without a
  per-line `external` object.
- List views with a `state ==` filter and `order_by`; minted `Uuid` identities through the derived
  `identity.mint` capability (bound to `uuid-v4` in both example environments).
- Host-side CPU and memory sampling per launched instance, and its share of the request.
- The `spindle.observe` domain with generated JSON Schema, Rust (`spindle-observe-api`) and
  TypeScript types, and the observe UI in `ui/` (Vue 3) built and tested on fixture answers.
- A specification directory is read through its `ess-inputs.yaml` when it has one. ESS moved to
  0.87.0.
- `b10x-spindle play`: the rally example's three components in one process, redrawn in the
  terminal after every tick until a side wins (`spindle-host`).
- `b10x-spindle run`: admit a specification, then execute commands read as JSON lines on stdin
  (`spindle-kernel`, with memory and SQLite stores).
- `spindle-timer`: periodic bindings on an injected clock, with the ESS periodic profile.
- `spindle-channel`: event bindings between components, in process or over TCP, with an outbox and
  at-least-once redelivery.
- `spindle-target`: the deployment target contract, placement within a target's ceiling, and the
  process launcher. `spindle.placement` in Spindle's own specification.
- `spindle-qemu` and `spindle-firecracker`: microVM launchers without root, as libraries; their
  tests run on a host with KVM (`task check:vm`).
- The rally example specification, `examples/rally/ess`.
- `b10x-spindle check`, `capabilities` and `admit`: load and compile a specification with
  `ess-compiler`, derive its capabilities, and admit it against a `spindle-environment/1`
  document (exit 3 when refused).

### Changed

- The `b10x-spindle` command-line definitions moved to `spindle_cli::cli` so the documentation
  generator reads the same clap definitions the binary parses. The commands are unchanged.
