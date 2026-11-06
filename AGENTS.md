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
| `crates/spindle-target/` | the deployment target contract: `Launcher`, `LaunchPlan`, `Running`, `Exit`, `place`/`start`; the process launcher (`with_telemetry`: `SPINDLE_TELEMETRY=unix:<socket>`; each instance in its own process group, sent SIGTERM by `PR_SET_PDEATHSIG` when the launching thread ends) |
| `crates/spindle-qemu/` | `QemuLauncher`: QEMU `microvm` with the host kernel, a Rust `/init` in a cpio initramfs, a private user and network namespace; `with_telemetry`: a `vhost-vsock-device` per guest |
| `crates/spindle-firecracker/` | Firecracker launcher: API socket, taps on a bridge inside a private user and network namespace; `with_telemetry`: a vsock device, `vsock.sock_<port>` |
| `crates/spindle-telemetry/` | the `Telemetry` port: atomic counters and fixed-bucket histograms per instance, `CountingStore`, the counter catalogue derived from the IR; the transport: `Sender` (one `spindle-telemetry/1` frame per period to `SPINDLE_TELEMETRY`, `unix:` or `vsock:`), `Frame`, `FrameReader`, `vsock` |
| `crates/spindle-host/` | every component of a system in one process: per-component admission and store, the timer on an injected clock, in-process channels, one-row and list view reads, an injected decider for `external:` causes (`Host::deciding`), a row written as it rests into its owning component's store (`Host::establish`); `rally::frame` |
| `crates/spindle-conformance/` | `HostTarget`, the in-process `ess-conformance` target (`external:` causes answered as a scenario forces them, `establish` steps written through `Host::establish` after ESS checks them); `synthesize`, `execute`, `Run` (`account`: ESS's known-failure accounting, bound to `build()`), `Counts` (`spindle-conformance-counts/1`) |
| `crates/spindle-ingress/` | the HTTP adapter behind `ingress.network` (`http-loopback`): routes and answers as ESS publishes them (derived, or from an `ess-http/1` binding compiled by ESS); hyper on 127.0.0.1, requests handed to the host's thread; `bind_guarded` also refuses a foreign `Host` (403) and a request without its token (401) |
| `crates/spindle-control/` | the control process `deploy` runs: a `spindle-placement/1` document placed and launched (process targets), telemetry collected over a Unix socket, the last 15 minutes of samples kept, the read-only observe API and the UI on 127.0.0.1 (token, `Host` check; each network component's ingress gets `ingress` and `ingress_token` settings and is guarded the same way; a telemetry frame counts only from a peer in the instance's process group); `build.rs` embeds `ui/dist`, and without it `GET /` answers 503; `spindle-control-fixture` is its tests' instance |
| `crates/spindle-cli/` | `b10x-spindle`: `check`, `capabilities`, `admit`, `run` (JSON lines on stdin, `--scenario` replaying an `ess-scenario` document through `ess-conformance`, or HTTP where `ingress.network` is required, `--http` an `ess-http/1` binding), `metrics`, `play` (`--component`: one component, serving its ingress at `SPINDLE_SETTING_INGRESS` when given), `deploy` (the control process, `spindle-control`), `conform` (a suite synthesised from or written for a specification, run against `HostTarget`, `--counts` held, `--known-failing` an `ess-known-failures/1` declaration) (exit 3 = refused); the clap definitions are `spindle_cli::cli` |
| `crates/spindle-observe-api/` | generated Rust types of the observe API; outside the workspace, drift-checked by `task check` |
| `crates/spindle-docs/` | generates `website/docs/reference/cli.md`, `website/docs/status.md` and `website/data/status.json`; binds a built site to its commit (`provenance`) |
| `ui/` | the observe UI: Vue 3 + TypeScript, Vite, pnpm; `ui/src/api/generated/` is generated; `task ui` builds `ui/dist`, which `b10x-spindle` embeds, so `task build` and `task check` build it before cargo |
| `website/` | the documentation site, Docusaurus pinned to `@beyond10x/docs-system`; logo in `website/static/img/` |
| `examples/environments/` | `laptop.toml`, `laptop-processes.toml` (the same with `channel = tcp`), `microvm.toml` |
| `examples/placements/` | `rally-processes.toml`: the three rally components on one process target, for `deploy` |
| `examples/rally/` | the example system: one table, two players, three components |
| `conformance.toml` | what `task check:conformance` holds: the exact set of scenarios that execute, the skipped and the known-failed counts |
| `conformance-known-failures.json` | the `ess-known-failures/1` declaration `task check:conformance` reads: each scenario known to fail, why, and its tracking record |
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
  `target/`; check `df -h /` first. While implementing, build only the targets you change
  (`cargo test -p <crate> --lib`, `--test <file>`); `cargo test -p <crate>` links every test
  binary of the crate and runs once, at the gate.
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
`check:ui` (`pnpm` test, typecheck and build in `ui/`), `task docs-check`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked --no-fail-fast`. The ESS pin is the `tag` of `ess-compiler`,
`ess-domain` and `ess-primitives` in `Cargo.toml`; CI installs the `ess` CLI of the same version
(`.github/workflows/ci.yml`). Move them together.

VM tests, and tests that start a user namespace, are `#[ignore]`d: CI runners refuse
unprivileged user namespaces. `task check:vm` runs them on a host with `/dev/kvm`,
`qemu-system-x86_64`, `unshare`, `nsenter`, `ip` and, for the QEMU telemetry case,
`/dev/vhost-vsock`; the Firecracker cases also need
`SPINDLE_FIRECRACKER` (the binary) and `SPINDLE_FIRECRACKER_KERNEL` (an uncompressed guest
`vmlinux`).

## Documentation

The site is `website/`, published at <https://beyond10x.github.io/spindle/>. Humans read
[`README.md`](README.md); the site holds the concepts, getting started, the rally example, the CLI
reference and the status page.

- Generated, never edited by hand: `website/docs/reference/cli.md` (from `spindle_cli::cli`) and
  `website/docs/status.md` with `website/data/status.json` (from `crates/spindle-docs/src/status.rs`).
  `task docs-generate` rewrites them; `task docs-check` fails on drift.
- A `shipped` status item names its test as `file::function`; `generate` refuses one that does
  not exist (`crates/spindle-docs/src/status.rs::every_shipped_item_names_a_test_that_exists`).
  Anything not on `main` is `planned`.
- A change to a command, a capability rule or a crate changes the site, `README.md` and this file
  in the same commit. Commands on a page are run and their output pasted from the run.
- Hand-written pages stay passive Markdown: no `{` and no capitalised tag outside code
  (`generate --check` refuses them). Admonition titles use brackets, `:::caution[Planned]`.
- Build the site: `task website` (`npm ci --allow-git=root`, `npm run build`). Workflows:
  `.github/workflows/pages.yml` (`Documentation validation`: generated-page check, site build,
  `spindle-docs` tests and provenance, artifact `b10x-project-site` on `main`) and
  `.github/workflows/b10x-docs-site.yml` (`Documentation site`: deploys through the Website's
  `project-site.yml` after a green bot push to `main`).

## Publishing

Commits and pushes are `b10x-bot[bot]` through `b10x-gates bot`; every GitHub write goes through
`b10x-gates api`. The crates are `publish = false`; nothing goes to a registry.

### Cutting a release

Tags are bare semantic versions (`0.1.0`); `CHANGELOG.md` follows Keep a Changelog. A release is
cut from `main`:

1. On a branch from current `origin/main`, set `version` under `[workspace.package]` in
   `Cargo.toml` and refresh `Cargo.lock` (`cargo update -w`). Rename `## [Unreleased]` in
   `CHANGELOG.md` to `## [<version>] - <YYYY-MM-DD>` and open a new empty `## [Unreleased]` above
   it.
2. Run `task check`, exit 0.
3. Commit `chore: release <version>` through the bot (`git add <paths> && b10x-gates bot --
   commit -F <file>`), push the branch through the bot, open the pull request through
   `b10x-gates api` and merge it with a merge commit once CI's `Gate` is green. `git fetch` and
   confirm the merge commit on `origin/main`.
4. Tag that merge commit and push the tag through the bot:
   `b10x-gates bot -- tag -a <version> <commit> -m "spindle <version>"`, then
   `b10x-gates bot -- push origin <version>`. Released tags are immutable.
5. Create the release page through the bot, so it is authored by `b10x-bot[bot]`:
   `b10x-gates gh --policy <policy> --repository beyond10x/spindle -- release create <version>
   --verify-tag --title <version> --notes-file <file>`, the notes being the version's
   `CHANGELOG.md` section. Never with a personal `gh` login; such a page is deleted and recreated
   under the bot.
6. The tag push starts [`release.yml`](.github/workflows/release.yml). It checks that the tag is
   annotated, on `main`, equal to the workspace version and named by a dated `CHANGELOG.md`
   heading; waits for CI's `Gate` to be green on the tagged commit; builds the observe UI once and
   `b10x-spindle` with it embedded for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`;
   and uploads `b10x-spindle-<version>-<target>.tar.gz` and `SHA256SUMS` to the page once it
   exists. macOS builds are not released yet: the host crates use Linux-only APIs (`accept4` and
   `SOCK_CLOEXEC` in `spindle-telemetry`, the parent-death signal in `spindle-target`). A target is
   added as a matrix row in the `build` job and a name in the `publish` job's `targets`. It never
   creates or edits the page and never replaces an asset. A pull request that changes the workflow
   runs the builds and publishes nothing; `workflow_dispatch` with `tag` backfills an existing tag.
7. Verify: the `Release` run for the tag succeeded; the page is authored by `b10x-bot[bot]`, is
   Latest and resolves to the tag's commit; it carries one archive per target and `SHA256SUMS`, and
   `sha256sum --check SHA256SUMS` passes on the downloaded archives.
