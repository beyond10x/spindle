<p align="center"><img src="website/static/img/logo.svg" alt="Spindle" width="96" height="96"></p>

# Spindle

Documentation: <https://beyond10x.github.io/spindle/> ·
[Getting started](https://beyond10x.github.io/spindle/docs/getting-started) ·
[CLI](https://beyond10x.github.io/spindle/docs/reference/cli) ·
[Status](https://beyond10x.github.io/spindle/docs/status)

Spindle runs ESS specifications. A specification says what an application means: entities,
commands, guards, events, bindings, views. Spindle supplies everything technical underneath: where
state is stored, how a timer fires, how an event of one component reaches a command of another.

Those technical needs are **capabilities**, and Spindle derives them from the specification. An
author does not list them. An **environment** says which implementations provide which capabilities
where Spindle runs. **Admission** binds each required capability to one implementation, or refuses
and lists every capability the environment lacks. The same specification runs on a laptop with an
in-process channel and in a microVM with a TCP one. Admission is the only step that differs.

Status: early, and these parts work:

- loading, capability derivation, admission and command execution, with list views, minted
  identities and external decisions from a file
- per-component counters
- the rally example in one process, and as three processes or three microVMs joined by TCP
  channels
- `run`: serves a network-reached component over HTTP on 127.0.0.1
- the process, QEMU and Firecracker launchers, as libraries that sample each instance's CPU and
  memory
- `deploy`: launches a placement's instances as processes and serves the live observe API and UI

A bundle uploaded to deploy, and cells with ownership leases, are planned. The
[status page](https://beyond10x.github.io/spindle/docs/status) lists each item with the test that
checks it, and the design is [`docs/design/spindle.md`](docs/design/spindle.md).

## Try it

```console
task build
target/release/b10x-spindle check ess
target/release/b10x-spindle capabilities ess
target/release/b10x-spindle admit ess --environment examples/environments/laptop.toml
target/release/b10x-spindle play examples/rally/ess --environment examples/environments/laptop.toml
target/release/b10x-spindle admit ess --environment examples/environments/microvm.toml
```

The last command is refused with exit status 3: Spindle's own specification has an outcome decided
outside the model, and the `microvm` environment provides no implementation for it.

`task build` builds the observe UI (`ui/`, pnpm) and then `b10x-spindle`, which embeds it. A
binary built without `ui/dist` answers `GET /` with 503. `b10x-spindle deploy examples/rally/ess
--environment examples/environments/laptop-processes.toml --placement
examples/placements/rally-processes.toml` launches the three rally components as processes and
prints the URL of the observe UI, `http://127.0.0.1:7341/?t=<token>`.

`run --clock <RFC 3339>` starts a run on a manual clock at that instant instead of the system
clock, reading stdin, serving HTTP or replaying `--scenario`; reading stdin, a line's
`"at": "<RFC 3339>"` moves it forward before its command runs, an earlier instant rejects the
line, and a rejected line leaves the clock where it stood. `run --mint-seed <n>` mints every
identity and generated value from that seed, so two runs of the same commands mint the same
identities, never one an instance of the kept state already holds; a served run's reset without a
world mints from it again, and an environment that binds `identity.mint` to `seeded` runs on stdin
with seed 0 unless the flag names another, and serves HTTP given the flag. No `run` fires a
periodic binding. Reading stdin, `run` fires the event bindings as a served run does: each command
a binding invokes prints its result on its own line after the line that caused it, with
`"caused_by": {"line": <n>, "binding": "<id>"}`, and one that is refused or does not run makes the
run exit 1 after the last line. A stdin line `{"arrange": [<row>]}` writes rows in the shape
`POST /api/reset` takes them, `{"entity", "identity", "state", "fields"}`, before the next line,
and a command's input may name what an earlier line answered, such as the identity line 1 minted,
with `{"$line": 1, "pointer": "/events/0/payload/<field>"}`.

Where `run` serves HTTP, it also serves the console routes `POST /api/reset`, `GET /api/events`,
`GET /api/model` and `GET /api/served`. `--console off` (or `SPINDLE_SETTING_CONSOLE=off`)
turns them off: each answers `404` as an unrouted path does, whatever secret the request carries.
A run whose `--address` is off loopback serves them only with `--console on`. A route the
specification publishes at a console path exits 1 before listening, the console on or off.

Where `run` serves HTTP, `--seed <file>` (or `[seed] file` in the environment document) submits a
JSON array of commands, each with its input, actor, caller and request id, on every start: checked
before listening, then one at a time in order beside the listener. An outcome declared
`existing_instance:` counts as already there. On a `--state` directory the run records the seed
file's digest, the entries sent and those rejected in `seed.json`, so a restart with the same file
sends only the entries not yet done (a rejected entry is sent again), and none once all are; one
with another file exits 1 before listening, naming both digests, unless the state holds no row. A
reset by a run without `--seed` removes `seed.json`. A store kept elsewhere records nothing, and
every start sends the whole seed. Within one start, an entry whose request id an earlier applied
or already-there entry carried is not submitted again. A declared refusal is logged and the rest still run; one line on
stderr gives the counts when the seed ends. An entry that did not run (the host or the kernel
refused it before an outcome, such as `not_granted`) is counted `rejected`, and once the seed ends
the run exits 1. `POST /api/reset` drops the entries not yet sent; with no
world it then submits the seed again, from its first entry, before it answers, and with a world it
loads that world alone and records every entry done. `play` and `deploy` refuse an environment with a `[seed]` table.

Where `run` serves HTTP, `--log-format json` writes each line on stderr as one JSON object with
`time` (RFC 3339 in UTC), `level` (`info`, `warn`, `error`), `message` and the fields the line
names (`component`, `command`, `binding`, `attempts`, `error`, `request_id`; an operation a line
before listening names as `command`, or `view` for a view); `text`, the default, writes each line
as before. On a run that reads stdin, `--log-format json` exits 1.

Where `run` serves HTTP, `--observe` records every request it answers. `GET /__spindle/requests`
lists the last 256 (`--observe-requests <N>`, 1 to 10000), newest first, with totals per class
since start. `?class=not-served` keeps only the requests answered `501`.

A record lists every external cause asked while the request was answered, by its command or by
one an event binding invoked. For each cause it gives the command that asked and what answered:
`world` for the last reset's decision table, `file` for `--decisions`, `default` for
`--answer-default`, or `unanswered`.

A cause asked outside any request, by a timer-fired command, is counted and listed in no record.

Under `unmet`, a record lists each event binding whose input an event published on the way did
not fill, so its command was not invoked. The entry gives the binding, the component, the command and
why.

Under `refused_bound`, a record lists each command an event binding invoked on the way that was
refused. The entry gives the binding, the component, the command, the command whose event the
binding carried (`caused_by`), the refusing outcome and its error or the kernel's error kind, and
the attempts, never a value of the error's fields.

`GET /__spindle/metrics` answers the run's metrics in the Prometheus text format
(`text/plain; version=0.0.4`). The families are those of `ess/fixtures/observe/metrics/families.json`:
requests by method, route template, operation, kind, class and status class, their durations,
commands by outcome and failure, events appended, bindings, timer ticks, store writes, the rows
each entity holds, the resets, the external causes answered, by command, outcome and source, the
refused bound commands, by binding, command and refusal, in a run given a seed, its entries
sent, applied, already there and refused, and the series mode. `--observe-series aggregated`
writes every route, operation, command, outcome, event, binding and entity label empty, and every
refusal that names a declared outcome, and sums the series that then match: the series count does
not grow with the number of commands or outcomes, and `component` keeps one value per declared
component. A scrape writes as many series for a specification of 300 commands as for one of 30;
the default, `per-command`, writes each value as its own series.

`GET /__spindle/topology` answers the graph of what the run serves (`spindle-topology/1`), derived
from the specification. It holds the ingress, the specification, its domains, their commands,
views and events, each external cause and each binding.

Its edges are `routes`, `contains`, `asks`, `emits`, `carries` and `invokes`. Each edge but
`contains` carries live numbers, read from the same counters as the metrics and the same ring as
the request log: a route's requests by status class and its latest five, a cause's answers by
source, events emitted, envelopes sent, commands invoked.

`POST /api/reset` empties the stores and keeps every count and the request log.

`GET /__spindle/ui` answers a panel of them: one HTML page with its script and style inline. Any
page may frame it, and it reads `./requests` and `./topology` beside itself. It draws the topology
top-down as SVG. The page is `ui/dist-panel/panel.html`, built by `pnpm build:panel` and embedded.
A binary built without it answers `503`. Paths under `/__spindle/` belong to the run: a binding
route there is refused at start, and without `--observe` they answer `404`.

`b10x-spindle conform ess --environment examples/environments/laptop.toml` runs the suite ESS
synthesises from Spindle's own specification against Spindle itself, with the scenarios
`ess/ess-inputs.yaml` lists. `task check:conformance` checks its counts against `conformance.toml`.
No scenario is known to fail, so the task declares none and every executed scenario must pass.

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
| `identity.mint` | a `creates:` outcome whose identity is neither an input nor a payload source (`{generated: true}`); a `Uuid` identity is minted as a UUIDv4 (`uuid-v4`), a `Uuid` newtype without invariants likewise, a bare `String` identity as a UUIDv4's text, a `String` newtype identity whose prefix and alphabet admit a UUID's text and whose invariants bound only its length (`value.count`) as its prefix and a UUIDv4's text, and an `Integer` identity, or an `Integer` newtype without invariants, as an integer drawn from a UUIDv4; any other identity type, and an entity invariant no minted length meets, is refused by name where a request selects that outcome, before anything is written, and the command's other outcomes run |
| `identity.mint`, `clock` | an outcome field marked `{generated: true}` (or `{input: f, else: {generated: true}}`) other than a minted creation identity, a refusal's error field the outcome gives no source and the row it is answered for does not hold (an `Optional` one stays absent; a type the runtime does not mint, and a required field of an `unknown_instance:` refusal, which has no row and which ESS's served application does not serve, are refused by name where a request selects that outcome, before anything is written), and each `response:` field of a command whose outcome returns it (`returns: true`; a `one_time_response:` field is minted fresh per invocation and written nowhere else): a `Uuid`, `String`, `Integer`, `Boolean`, `Bytes` or `List` is minted through `identity.mint` (a `List` as `[]`, as ESS mints it), a `Timestamp` read from `clock`; a response record, a union's first variant and a one-entry map need what their members, content and key need, an enum response is the variant one `identity.mint` draw names, and a constrained newtype answered by a literal its invariants name needs nothing |

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
prefix, and an exact key wins over it.

| `store.durable` | where each component's state is kept |
|---|---|
| `memory` | in the process, gone when it ends |
| `sqlite` | one SQLite file per component (`run --state`) |
| `postgres` | the tables `spindle_<component>_*` of the PostgreSQL the settings `database_url`, `database_ca`, `database_schema`, `database_pool` and `database_connections` name; `b10x-spindle store migrate` creates them ([environment and admission](https://beyond10x.github.io/spindle/docs/concepts/environment-and-admission#state-in-postgresql)); `b10x-spindle state move` moves a `run --state` directory into them, versions kept ([moving a state directory](https://beyond10x.github.io/spindle/docs/concepts/environment-and-admission#moving-a-state-directory-into-postgresql)) |
| `indexeddb` | one IndexedDB database per component, in the Service Worker host |

The Node export keeps every component in `memory` and serves components whose `ingress.network`
is bound to `node`.

An `external.` key may name an HTTP call the document declares as `[http.<name>]`, which `run`
asks for each such cause ([an external cause bound to an HTTP call](https://beyond10x.github.io/spindle/docs/concepts/environment-and-admission#an-external-cause-bound-to-an-http-call)).
The call leaves the host only over verified TLS: an `https://` URL, its certificate verified
against the platform's trust roots or the PEM file `ca_file` names. A plain `http://` URL is
accepted only on loopback (`localhost`, `127.0.0.0/8`, `[::1]`). Any other URL is refused when
the document is read.

An `[egress]` table binds a published event to such a call (`"<qualified event>" = "<name>"`,
method `POST`): a served `run` posts each such event there as one `spindle-egress/1` JSON body
(`format`, `event`, `component`, `log`, `position`, `fields`), at least once, in commit order
per component, and goes on after a restart from the position it keeps in the component's store;
a component kept in `memory` is refused
([published events posted to a webhook](https://beyond10x.github.io/spindle/docs/concepts/environment-and-admission#published-events-posted-to-a-webhook)).

## Node export

`spindle-node` serves a specification inside a Node program, as an in-process fake backend:

```console
cargo build --locked --release --target wasm32-unknown-unknown -p spindle-node
wasm-bindgen --target web --out-dir target/spindle-node target/wasm32-unknown-unknown/release/spindle_node.wasm
```

```js
import { readFileSync } from "node:fs";
import { initSync, Spindle } from "./target/spindle-node/spindle_node.js";

initSync({ module: readFileSync("./target/spindle-node/spindle_node_bg.wasm") });
const spindle = Spindle.create({ sources, environment, decisions });
const answer = await spindle.handle({ method: "GET", path: "/review/views/Submissions" });
```

`create` takes the specification's sources, an environment whose stores are bound to `memory`
(`examples/environments/node.toml`), and optionally a decisions file, `answerDefault`, an
`ess-http/1` binding and `fixedActors` (`[{ actor, caller }]`, as `run --fixed-actor` with
`--fixed-caller`). `handle` answers through the router the loopback listener calls, the console
routes included; `notServed`, `reset(world)` and `dispose` complete it
([the Node export](https://beyond10x.github.io/spindle/docs/concepts/environment-and-admission#the-node-export)).
`task check:node_serves_a_specification` builds it and runs its tests under Node.

## Layout

| path | what |
|---|---|
| `ess/` | Spindle's own specification: environments, deployments, admission |
| `crates/spindle-spec` | loads and compiles a specification with `ess-compiler` |
| `crates/spindle-capability` | derivation, environments, admission |
| `crates/spindle-kernel` | executes commands against the compiled model |
| `crates/spindle-timer`, `crates/spindle-channel` | periodic bindings and event bindings |
| `crates/spindle-target` | the deployment target contract and the process launcher |
| `crates/spindle-qemu`, `crates/spindle-firecracker` | microVM launchers |
| `crates/spindle-telemetry` | counters and histograms per instance, their catalogue derived from the specification, and the sender that reports them as `spindle-telemetry/1` frames over a Unix socket or vsock |
| `crates/spindle-host` | runs every component of a system in one process |
| `crates/spindle-store-browser` | the store on the browser's IndexedDB, for a run in a page or a Service Worker |
| `crates/spindle-wasm` | the wasm-bindgen entry a Service Worker opens: a specification compiled in the browser, its routes and the console routes answered by the same router the loopback listener calls, state in IndexedDB, external outcomes from a decisions file |
| `crates/spindle-embed` | a host opened in-process from texts and answered one request at a time, which the Service Worker host and the Node export share |
| `crates/spindle-node` | the Node export: an ES module (`wasm-bindgen --target web`) that serves a specification in-process in a Node program, every store in memory |
| `crates/spindle-conformance` | runs an ESS conformance suite against every component in one process |
| `crates/spindle-control` | the control process `deploy` runs: launch, collect, and the observe API and UI on 127.0.0.1 |
| `crates/spindle-cli` | the `b10x-spindle` binary |
| `crates/spindle-docs` | generates the site's CLI reference and status page |
| `examples/` | environments and the `rally` example system |
| `web/` | the static site: a Vue shell that registers the Service Worker (`task web` builds `web/dist`) |
| `website/` | the documentation site (Docusaurus) |
| `docs/design/spindle.md` | the design |

Agents start at [`AGENTS.md`](AGENTS.md).

## Releases

Binaries for Linux and macOS (x86_64 and aarch64 each) are attached to each
[GitHub release](https://github.com/beyond10x/spindle/releases) with a `SHA256SUMS` file. The Linux
binaries need glibc 2.17 or newer; `x86_64-unknown-linux-musl` is a static build for hosts without
glibc. Each release also publishes the image `ghcr.io/beyond10x/spindle:<version>`, one manifest
list for `linux/amd64` and `linux/arm64`: the release's gnu binary on `gcr.io/distroless/cc-debian12`
(no shell), run as UID 65532, with `b10x-spindle` as the entrypoint. The release's `IMAGES` asset,
listed in `SHA256SUMS`, names that list by digest (`ghcr.io/beyond10x/spindle:<version>@sha256:<digest>`);
pull it by that line. A container port reaches `run` only on an address off loopback
(`--address 0.0.0.0`), which needs `--trusted-caller-header` and `--trusted-caller-secret`. Changes
are in [`CHANGELOG.md`](CHANGELOG.md), and the release steps in
[`AGENTS.md`](AGENTS.md#cutting-a-release).

## License

Apache-2.0.
