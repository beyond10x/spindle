<p align="center"><img src="website/static/img/logo.svg" alt="Spindle" width="96" height="96"></p>

# Spindle

Documentation: <https://beyond10x.github.io/spindle/> ·
[Getting started](https://beyond10x.github.io/spindle/docs/getting-started) ·
[CLI](https://beyond10x.github.io/spindle/docs/reference/cli) ·
[Status](https://beyond10x.github.io/spindle/docs/status)

Spindle runs ESS specifications. A specification says what an application means — entities,
commands, guards, events, bindings, views. Spindle supplies everything technical underneath: where
state is stored, how a timer fires, how an event of one component reaches a command of another.

Those technical needs are **capabilities**, and Spindle derives them from the specification. An
author never lists them. An **environment** says which implementations provide which capabilities
where Spindle runs. **Admission** binds each required capability to one implementation, or refuses
and names every capability the environment lacks. The same specification runs on a laptop with an
in-process channel and in a microVM with a TCP one; admission is the only step that differs.

Status: early. Loading, capability derivation, admission, command execution (with list views,
minted identities and external decisions from a file), per-component counters and the rally example
in one process and as three processes joined by TCP channels work, and `run` serves a network-reached component over HTTP on 127.0.0.1; the process, QEMU and Firecracker launchers exist as libraries and sample each
instance's CPU and memory; `deploy` launches the instances of a placement as processes and serves
the live observe API and the UI built into the binary. Running each component in its own microVM is
planned. The
[status page](https://beyond10x.github.io/spindle/docs/status) lists each item with the test that
holds it; the design is [`docs/design/spindle.md`](docs/design/spindle.md).

## Try it

```console
task build
target/release/b10x-spindle check ess
target/release/b10x-spindle capabilities ess
target/release/b10x-spindle admit ess --environment examples/environments/laptop.toml
target/release/b10x-spindle play examples/rally/ess --environment examples/environments/laptop.toml
target/release/b10x-spindle admit ess --environment examples/environments/microvm.toml
```

The last command is refused: Spindle's own specification has an outcome decided outside the
model, and the `microvm` environment provides nothing for it. Exit status 3 means refused.

`task build` builds the observe UI (`ui/`, pnpm) and then `b10x-spindle`, which embeds it; a
binary built without `ui/dist` answers `GET /` with 503. `b10x-spindle deploy examples/rally/ess
--environment examples/environments/laptop-processes.toml --placement
examples/placements/rally-processes.toml` launches the three rally components as processes and
prints the URL of the observe UI, `http://127.0.0.1:7341/?t=<token>`.

`b10x-spindle conform ess --environment examples/environments/laptop.toml` runs the suite ESS
synthesises from Spindle's own specification against Spindle itself; `task check:conformance`
holds its counts against `conformance.toml`, with the scenarios known to fail declared in
`conformance-known-failures.json` (`ess-known-failures/1`).

## Capabilities

| capability | derived from |
|---|---|
| `store.durable` | an entity in a domain the deployment owns |
| `timer`, `clock` | a periodic binding |
| `channel` | a binding from one component's event to another component's command |
| `ingress.network` | a component `reached_by: network` |
| `ingress.command_line` | a component `reached_by: command_line` |
| `projection` | a view with `consistency: eventual` |
| `external.<command>.<outcome>` | an outcome whose condition is `external:` |
| `identity.mint` | a `creates:` outcome whose identity is neither an input nor a payload source (`{generated: true}`); a `Uuid` identity is minted as a UUIDv4 (`uuid-v4`), a `Uuid` newtype without invariants likewise, and a `String` newtype identity whose prefix and alphabet admit a UUID's text and whose invariants bound only its length (`value.count`) as its prefix and a UUIDv4's text; any other identity type, and an entity invariant no minted length meets, is refused by name where a request selects that outcome, before anything is written, and the command's other outcomes run |
| `identity.mint`, `clock` | an outcome field marked `{generated: true}` (or `{input: f, else: {generated: true}}`) other than a minted creation identity, a refusal's error field the outcome gives no source and the row it is answered for does not hold (an `Optional` one stays absent; a type the runtime does not mint is refused by name where a request selects that outcome, before anything is written), and each `response:` field of a command whose outcome returns it (`returns: true`; a `one_time_response:` field is minted fresh per invocation and written nowhere else): a `Uuid`, `String`, `Integer`, `Boolean`, `Bytes` or `List` is minted through `identity.mint` (a `List` as `[]`, as ESS mints it), a `Timestamp` read from `clock`; a response record, a union's first variant and a one-entry map need what their members, content and key need, an enum response is the variant one `identity.mint` draw names, and a constrained newtype answered by a literal its invariants name needs nothing |

## Environment

```toml
format = "spindle-environment/1"
name = "laptop"
os = "linux"
arch = "x86_64"

[provides]
"store.durable" = ["sqlite", "memory"]
channel = ["in-process"]
"external.*" = ["operator-prompt"]
```

Each list is in order of preference. A key ending in `.*` covers every capability under that
prefix; an exact key wins over it.

## Layout

- `ess/` — Spindle's own specification: environments, deployments, admission.
- `crates/spindle-spec` — loads and compiles a specification with `ess-compiler`.
- `crates/spindle-capability` — derivation, environments, admission.
- `crates/spindle-kernel` — executes commands against the compiled model.
- `crates/spindle-timer`, `crates/spindle-channel` — periodic bindings and event bindings.
- `crates/spindle-target` — the deployment target contract and the process launcher.
- `crates/spindle-qemu`, `crates/spindle-firecracker` — microVM launchers.
- `crates/spindle-telemetry` — counters and histograms per instance, their catalogue derived from the specification, and the sender that reports them as `spindle-telemetry/1` frames over a Unix socket or vsock.
- `crates/spindle-host` — runs every component of a system in one process.
- `crates/spindle-store-browser` — the store on the browser's IndexedDB, for a run in a page or a Service Worker.
- `crates/spindle-wasm` — the wasm-bindgen entry a Service Worker opens: a specification compiled in the browser, its routes and the console routes answered by the same router the loopback listener calls, state in IndexedDB, external outcomes from a decisions file.
- `crates/spindle-conformance` — runs an ESS conformance suite against every component in one process.
- `crates/spindle-control` — the control process `deploy` runs: launch, collect, and the observe API and UI on 127.0.0.1.
- `crates/spindle-cli` — the `b10x-spindle` binary.
- `crates/spindle-docs` — generates the site's CLI reference and status page.
- `examples/` — environments and the `rally` example system.
- `web/` — the static site: a Vue shell that registers the Service Worker (`task web` builds `web/dist`).
- `website/` — the documentation site (Docusaurus).
- `docs/design/spindle.md` — the design.

Agents start at [`AGENTS.md`](AGENTS.md).

## Releases

Binaries for Linux and macOS (x86_64 and aarch64 each) are attached to each
[GitHub release](https://github.com/beyond10x/spindle/releases) with a `SHA256SUMS` file; changes
are in [`CHANGELOG.md`](CHANGELOG.md), and the release steps in
[`AGENTS.md`](AGENTS.md#cutting-a-release).

## License

Apache-2.0.
