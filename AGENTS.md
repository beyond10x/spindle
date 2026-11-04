# AGENTS.md — spindle

Spindle runs ESS specifications on the capabilities an environment provides: it loads and compiles
a specification with `ess-compiler`, derives the capabilities it needs, admits it against an
environment, and executes it. Humans start at [`README.md`](README.md); the design is
[`docs/design/spindle.md`](docs/design/spindle.md).

## Serves

- **O4 — products run on the foundation.** A compiled ESS model is the program; a product's
  specification runs without code generated per application.
- **O2 — decisions as data, with evidence.** Capabilities are derived from the specification, and
  admission records which implementation provides each one, or every one that is missing.
- **O1 — governed reach.** Each component of a system can run in its own isolation unit, with the
  technicals (storage, timers, channels, ingress) bound per environment.

## Map

| path | what |
|---|---|
| `.engineering/` | the AEP store (`aep.project/5`, protocol `adp/1`, profile `development.standard`) |
| `ess/` | Spindle's own specification: `spindle.deployment`, `spindle.placement`, `spindle.observe` (the observe API answers); `ess/fixtures/observe/` holds API fixtures, `ess/generated/` the generated JSON Schema (`task generate:observe`) |
| `crates/spindle-spec/` | `load`/`compile`: source files → `EssIr`, through `ess-domain` and `ess-compiler` only |
| `crates/spindle-capability/` | `derive` (IR → requirements with reasons), `Environment` (`spindle-environment/1`, TOML), `admit` |
| `crates/spindle-kernel/` | executes commands against the compiled IR: guards, outcomes, entity and newtype invariants, events; SQLite or memory store |
| `crates/spindle-timer/` | fires periodic bindings on an injected clock |
| `crates/spindle-channel/` | carries event bindings between components (in-process, TCP) |
| `crates/spindle-target/` | the deployment target contract: `Launcher`, `LaunchPlan`, `Running`, `Exit`, `place`/`start`; the process launcher |
| `crates/spindle-qemu/` | `QemuLauncher`: QEMU `microvm` with the host kernel, a Rust `/init` in a cpio initramfs, a private user and network namespace |
| `crates/spindle-firecracker/` | Firecracker launcher: API socket, taps on a bridge inside a private user and network namespace |
| `crates/spindle-telemetry/` | the `Telemetry` port: atomic counters and fixed-bucket histograms per instance, `CountingStore`, the counter catalogue derived from the IR |
| `crates/spindle-host/` | every component of a system in one process: per-component admission and store, the timer on an injected clock, in-process channels, one-row and list view reads; `rally::frame` |
| `crates/spindle-cli/` | `b10x-spindle`: `check`, `capabilities`, `admit`, `run`, `metrics`, `play` (exit 3 = refused) |
| `crates/spindle-observe-api/` | generated Rust types of the observe API; outside the workspace, drift-checked by `task check` |
| `ui/` | the observe UI: Vue 3 + TypeScript, Vite, pnpm; `ui/src/api/generated/` is generated |
| `examples/environments/` | `laptop.toml`, `microvm.toml` |
| `examples/rally/` | the example system: one table, two players, three components |
| `docs/design/spindle.md` | design, decisions (§11), milestones (§12), the rally showcase (§13) |

## Rules

- **Running code is Rust**, clap derive for the command line. No Python, no shell helpers in the
  repository.
- **ESS is read through its crates.** Parse, assemble and compile with `ess-domain` and
  `ess-compiler` at the pinned tag; never re-implement ESS parsing, validation or guard
  evaluation. A document `ess specify validate` refuses must be refused here too.
- **Capabilities are derived, never authored.** Do not add a field to a specification or a bundle
  that lists capabilities. A new capability is a new derivation rule in
  `crates/spindle-capability/src/derive.rs`, a row in the README table and a test.
- **No technical words in a specification.** A specification never names a database, a protocol
  or a queue. Those live in an environment document or a runtime implementation.
- **The overlay is open.** Runtime defaults first; do not add an overlay document or an ESS
  keyword for deployment defaults without a recorded decision in the design (§11).
- **Out of scope:** the `service-sdk` repository. Do not read it, depend on it or copy from it.
  `eventlog` may be used for storage.
- **Confidentiality.** This repository is public. No name of an external company, customer,
  product or host, and no absolute home-directory path, in any file or commit message.
- No comments in `Cargo.toml`, `Taskfile.yml`, workflow files or environment documents.
- Builds: `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`, the tree's own
  `target/`; check `df -h /` first.
- Tests that need a directory use `tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))`.
- Planning store mutations use `aep plan artifact` only. Scratch bodies go in
  `.engineering/drafts/`, which is git-ignored.
- Use managed worktrees for changes; keep the primary checkout clean.

## Gate

```console
task check
```

Steps (`Taskfile.yml`): `aep plan artifact validate`, `ess specify validate` on `ess` and
`examples/rally/ess`, the three observe contract checks (`check:the_observe_domain_validates`,
`check:the_generated_types_have_no_drift`, `check:every_fixture_answer_validates_against_its_schema`),
`check:ui` (`pnpm` test, typecheck and build in `ui/`), `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked --no-fail-fast`. The ESS pin is the `tag` of `ess-compiler`,
`ess-domain` and `ess-primitives` in `Cargo.toml`; CI installs the `ess` CLI of the same version
(`.github/workflows/ci.yml`). Move them together.

VM tests, and tests that start a user namespace, are `#[ignore]`d: CI runners refuse
unprivileged user namespaces. `task check:vm` runs them on a host with `/dev/kvm`,
`qemu-system-x86_64`, `unshare`, `nsenter` and `ip`; the Firecracker cases also need
`SPINDLE_FIRECRACKER` (the binary) and `SPINDLE_FIRECRACKER_KERNEL` (an uncompressed guest
`vmlinux`).

## Publishing

Commits and pushes are `b10x-bot[bot]` through `b10x-gates bot`; every GitHub write goes through
`b10x-gates api`. No release process yet; the crates are `publish = false`.
