# Changelog

This file follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and Spindle
follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Bare tags such as `0.1.0`
are Spindle's releases.

## [Unreleased]

### Added

- `b10x-spindle` builds for macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`), and each
  release carries both archives in `SHA256SUMS`. The in-process commands run there as on Linux;
  `deploy`'s telemetry peer check reads `LOCAL_PEERPID`, and the longest telemetry socket path
  follows the platform's `sun_path` (103 bytes on macOS). The microVM launchers, vsock and the
  `/proc` resource readings answer `Unsupported` off Linux, and a process instance gets no
  parent-death signal there. CI checks and lints both macOS targets and runs the in-process test
  lanes on a macOS runner; a release publishes only once that job is green on the tagged commit.

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
