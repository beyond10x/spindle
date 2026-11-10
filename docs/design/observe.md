# Spindle observe: every instance visible while it runs

Status: design, 2026-11-04. Extends [spindle.md](spindle.md) § 6 (runtime architecture) and § 7
(deploy). Planned as `epic:observe`.

## 1. The idea

Every effect of a spindle instance passes through the runtime: a command is executed by
`spindle-kernel`, an event is appended by its store, a binding is carried by `spindle-channel`, a
periodic binding is fired by `spindle-timer`, and the instance itself is started by a `Launcher`
within a resource request. So the runtime can count all of it without the specification saying a
word about metrics, and without the application doing anything.

`b10x-spindle deploy` becomes a long-running control process. While the three rally instances
run in their microVMs, it serves `http://127.0.0.1:<port>/` with the three components, the
bindings between them, and for each instance what it executes, sends, writes, and how much of its
CPU and memory quota it uses.

```
 spindle · rally · deployment d-7f3a · qemu-microvm · 00:02:41                  ● live
┌──────────────────────────┐   BallMoved 1.0/s    ┌──────────────────────────┐
│ rally-table              │ ───────────────────▶ │ rally-player-west        │
│ Running  vm qemu 1 vCPU  │ ◀─────────────────── │ Running  vm qemu 1 vCPU  │
│ cpu  ▇▁▁▁▁  4 % of 1     │ PaddleMoveRequested  │ cpu  ▁▁▁▁▁  1 % of 1     │
│ mem  ▇▇▁▁▁ 41 / 128 MiB  │        0.6/s         │ mem  ▇▁▁▁▁ 22 / 64 MiB   │
│ cmd  Tick 1.0/s ok 161   │                      │ cmd  ObserveBall 1.0/s   │
│      MovePaddle 1.2/s    │                      │ evt  BallSeen 161        │
│ evt  BallMoved 161       │   BallMoved 1.0/s    ├──────────────────────────┤
│ store 1.9 KiB/s  322 ev  │ ───────────────────▶ │ rally-player-east        │
│ timer Tick on time 161/0 │ ◀─────────────────── │ ...                      │
└──────────────────────────┘                      └──────────────────────────┘
 board (MatchState view)          recent events (rally-table)
 ┌────────────────┐               00:02:41.002 Tick → moved-east-up    BallMoved {x: 9, y: 4}
 │       ●        │               00:02:40.611 MovePaddle → west-up
 ││              ││               00:02:40.002 Tick → moved-east-up    BallMoved {x: 8, y: 3}
 └────────────────┘ west 2 · east 1
```

## 2. Where the numbers come from

| source | what it counts | where it is measured |
|---|---|---|
| kernel | commands by name and outcome (executed, refused by error, wrong state, invalid input, broken invariant, unsupported); execution time; decider questions | `spindle-kernel` around `execute` |
| store | events appended by name, rows written, bytes written, the log's size | the store port (memory, SQLite) |
| channel | envelopes sent, delivered, acknowledged, redelivered, dropped, per binding; bytes and frames per peer; acknowledgements pending | `spindle-channel` endpoint and transport |
| timer | ticks fired, late, dropped, overlapping, per periodic binding | `spindle-timer` |
| ingress | requests by command and answer | ingress adapters, when they exist |
| views | queries by view; for an eventual view, its lag in events | view projection |
| instance resources | CPU time, memory in use, disk written, against the instance's request | the launcher on the host (see § 4) and the guest |

**The catalogue is derived, like capabilities.** `spindle-capability` already derives what an
instance needs from the compiled IR; the same pass derives which counters exist: one per command
and outcome the component accepts, one per event it publishes, one per binding it sends or
receives, one per periodic binding. `b10x-spindle metrics <spec> --component <c>` prints it. A
counter that the IR does not name is a bug, and a counter the IR names but the runtime never
reports is visible as "never reported" instead of zero.

**Names** are stable and dotted: `command.<CommandName>.<outcome>`, `event.<EventName>.appended`,
`binding.<binding>.<sent|delivered|redelivered|dropped>`, `binding.<binding>.<attempted|exhausted>`
(an `on_failure: retry` binding, or an `on_refusal` one that retries some refusal, on the component that invokes it), `timer.<binding>.<fired|late|dropped>`,
`store.bytes_written`, `resource.cpu_seconds`, `resource.memory_bytes`. The ESS handles are used
as written (they implement `Display`), so a name in the UI is the name in the specification.

## 3. Telemetry: from instance to control

- **A `Telemetry` port** in a new crate `spindle-telemetry`. Kernel, store, channel and timer get
  a sink; the default sink is a set of atomic counters and fixed-bucket histograms in the
  instance. Recording is a counter increment, never an allocation or a lock on the command path.
- **Samples, not events.** Every 1 s (configurable) the instance sends one
  `spindle-telemetry/1` frame: instance id, monotonic sequence, cumulative counters, histogram
  buckets, and the last N command answers (name, outcome, event names, payloads truncated to
  1 KiB). Cumulative counters make a lost frame harmless; the control process computes rates.
- **Transport by target.** The instance writes to one endpoint given at launch as
  `SPINDLE_TELEMETRY`:

  | target | endpoint |
  |---|---|
  | process | a Unix socket owned by the control process |
  | QEMU microvm | `vhost-vsock-device` (`/dev/vhost-vsock` is usable without root on this host), guest CID per instance, host port fixed |
  | Firecracker | its vsock device, which appears on the host as a Unix socket |
  | Docker, Compose, Kubernetes (later) | TCP to the control process, or a sidecar |

  vsock connects guest to host only, which is the direction telemetry needs; it does not touch
  the VMs' shared network, so telemetry never competes with rally's bindings.
- **Back-pressure.** If the control process is gone or slow, the instance drops frames and counts
  the drops; it never blocks a command on telemetry. The sender waits at most one period to
  connect and one period to write a frame; a frame it could not write whole in that time is
  dropped. A frame counted as sent was written to the socket, which is not proof that the
  control process read it: a Unix socket takes writes into a connection the listener has not yet
  accepted, until its buffer is full. The sequence number and the cumulative counters let the
  control process see what it missed.
- **Endpoints are checked where they are given.** A Unix socket path must be UTF-8 and fit
  `sun_path` less its NUL (107 bytes on Linux, 103 on macOS); the launcher and the sender refuse
  any other by name instead of dropping every frame. QEMU guest context ids come from one count per process,
  `3 + ((pid - 1) << 10 | n)` for `n` below 1024, so no two guests on the host share one.

## 4. Resources against the quota

The request comes from placement (`spindle.placement.Instance.request`: vCPUs, memory, disk).
Two views are measured, because each misses something:

| measured | where | what it shows |
|---|---|---|
| host view | the launcher reads `/proc/<qemu or firecracker pid>/stat` and `status` (utime + stime, RSS) every sample | what the VM costs the host, including the VMM |
| guest view | the instance reads its own `/proc/stat`, `/proc/meminfo`, `/proc/self/io` and sends them in its frame | what the application uses inside its quota |

CPU % of quota = Δ cpu seconds / (Δ wall seconds × requested vCPUs). Memory % = in use / requested
MiB. For the `process` target the host view is the child's `/proc/<pid>`; it has no quota unless a
later launcher puts it in a cgroup, and the UI shows "no limit" instead of a percentage.

## 5. The control process

`b10x-spindle deploy <spec> --environment <file> --placement <file>` runs component
`spindle-control`, which already exists in spindle's own specification and owns
`spindle.deployment` and `spindle.placement`. So the control process is itself a spindle
instance: it runs spindle's own specification on `spindle-kernel` (declare and retire targets,
plan, place, launch, report exit), plus three runtime parts:

1. **Launch** through the `Launcher` of each target.
2. **Collect**: a telemetry listener per transport; keeps the last 15 minutes of samples per
   instance in memory (ring buffer); optional write to a file for later replay.
3. **Serve** on `127.0.0.1` only:

   | path | answer |
   |---|---|
   | `GET /api/deployment` | components, bindings (from the IR), instances, targets, requests, states (targets and requests from the views `TargetState` and `InstanceState`, § 5a) |
   | `GET /api/catalogue` | the derived counter catalogue |
   | `GET /api/stream` | Server-Sent Events: one message per sample per instance, plus state changes |
   | `GET /api/views/<component>/<view>` | a view answer, queried through the instance's own ingress, read only |
   | `GET /` | the UI |

   The API is read only in this design: the UI starts and stops nothing. A random token is printed
   with the URL (`http://127.0.0.1:7341/?t=…`); every request without it, or with a `Host` header
   other than `127.0.0.1:<port>` or `localhost:<port>`, is refused, which stops other local users
   and DNS rebinding from another site.

Spindle's own `ess/` gets a domain `spindle.observe` for the nouns the API returns (Sample,
Counter, Catalogue), so the API types are specified, and `ess` projects their JSON Schema.

## 5a. What `spindle.placement` declares

`ess/domains/placement.yaml` is the model the control process runs. What it declares, and what
stays a runtime rule outside it (each item names the case in
`crates/spindle-kernel/tests/spindle-kernel/placement.rs` that checks it):

- **Admission gates placement.** Nothing of a deployment that is not admitted is planned, placed
  or launched; a running deployment counts as admitted. `PlanInstance` answers
  `unknown-deployment` for a deployment no one offered and `not-admitted` for one that is not
  admitted, both with `DeploymentNotAdmitted`, and creates no instance; `PlaceInstance` answers
  `not-admitted` and refuses the instance; `LaunchInstance` answers `not-admitted` with
  `DeploymentNotAdmitted` and the instance stays `Placed`, which covers a deployment stopped after
  its instance was placed. Both errors name the deployment. `LaunchInstance` reads the deployment
  only for a `Placed` instance, so a planned, refused, running or exited one answers `wrong-state`
  first, as `PlaceInstance` answers a decided one. `LaunchInstance` decides it with a guard over
  the deployment its instance names, true unless it is `Admitted` or `Running`. `PlanInstance` and
  `PlaceInstance` keep it an `external:` outcome, which the control process answers from the
  stored `spindle.deployment.Deployment`: as a guard, `PlanInstance`'s leaves ESS no way to
  arrange a placed instance of a stopped deployment, and `PlaceInstance`'s no way to force its
  other refusals (`an_instance_of_a_refused_deployment_cannot_be_placed`).

- **A placement refusal names its cause.** `PlaceInstance` has one refusal per cause, asked in the
  order `spindle_target::place` lists its reasons: `target-retired`, `environment-mismatch`,
  `not-admitted`, `above-ceiling`. The first that holds is taken; `InstanceRefused.cause` and the
  instance's `refusal` record it as `TargetRetired`, `EnvironmentMismatch`, `NotAdmitted` or
  `AboveCeiling`, the order `RefusalCause` declares, and `spindle_target::Refusal::cause` names
  each reason's variant. The model records one cause. `spindle_target::place` lists more: a
  retired target, then an environment mismatch or, when the environments match, an admission
  that refused with every capability it lacks, then each resource over the ceiling, and
  `spindle_target::start` refuses with that list (`StartError::Refused`); the model records its
  first. `place` does not read the deployment's state, so it gives no reason for an offered or
  stopped deployment; that reaches the model only through the `not-admitted` answer. The four
  stay `external:`: ESS reads one related row per command and compares none of its fields with
  the instance's or another row's, and a guard on the target's state leaves ESS no way to force
  the other three (`a_placement_refusal_names_its_cause`).

- **Targets and requests are published by views.** `InstanceState` publishes each instance's
  state, deployment, component, settings, target, request and refusal; `TargetState` publishes
  each target's state, kind, environment and ceiling. `GET /api/deployment` answers its targets
  and requests from these two views (`targets_and_requests_are_published_by_a_view`), and with
  `TargetState` ESS synthesises the refusal of `RetireTarget` on a retired target
  (`retire_target_is_synthesised`).

- **A retired target takes no further instance.** `RetireTarget` moves a target from `Available`
  to `Retired`, which is terminal; retiring it again answers `TargetAlreadyRetired` and changes
  nothing. An instance planned onto a retired target is refused `target-retired` when it is
  placed. Instances already placed or running there are not moved; the operator retires a
  target to stop placing on it, not to stop what runs (`retire_target_is_stated`).

- **The ceiling bounds one instance.** `Target.ceiling` is the most one instance may take on the
  target, in vCPUs, memory MiB and disk MiB, declared with the target. An instance whose request
  exceeds the ceiling in any resource is refused `above-ceiling` with cause `AboveCeiling`. The
  model records the request as the instance asked for it, a zero left as zero, and
  `InstanceState.request` publishes it as requested; `spindle_target::place` raises each zero to
  the ceiling and the launcher applies those limits, so a placed instance never runs unbounded
  on a target that bounds it (`target_ceiling_is_stated`).

- **`Browser` is a target kind without a launcher.** Beside the kinds in § 3's transport table,
  `TargetKind` declares `Browser`, a browser tab, as `spindle_target::TargetKind` does. A
  target of that kind can be declared and planned onto; no launcher starts one yet, and § 3 gives
  it no telemetry endpoint (`browser_target_kind_is_stated`).

- **An exit reported for a planned instance changes nothing.** `ReportExit` moves only a `Placed`
  or `Running` instance to `Exited`. For a `Planned` instance, as for a `Refused` or `Exited` one,
  it is accepted as `already-ended` and changes nothing: no launcher ran it, so the report carries
  nothing to record (`report_exit_of_a_planned_instance_is_stated`).

## 6. The UI

- Vue 3, TypeScript, Vite, pnpm, in `ui/` (the browser rule in the workspace `CLAUDE.md`). The
  built bundle is embedded in `b10x-spindle` at compile time, so `deploy` needs nothing installed.
- TypeScript types for the API are generated from the Rust types and drift-checked by
  `task check`.
- Three screens:
  1. **Topology**: one card per component, edges for each binding with live rate and redelivery
     count, state and target per instance. A sparkline for CPU and memory of quota.
  2. **Instance**: per command and outcome, rate and count; latency histogram; events appended;
     store bytes; channel per peer; timer on-time / late / dropped; the recent answers; the console
     tail from the launcher.
  3. **View**: any view the component declares, rendered as a table; for rally, `MatchState` is
     also drawn as the board.
- No chart library at first: SVG sparklines and bars.

## 7. Export (later)

`/metrics` in Prometheus text and an OTLP exporter, both fed from the same samples. Not in the
first stories.

A served run (`b10x-spindle run` over `http-loopback`, not `deploy`) gets its own Prometheus text,
request log and topology under the reserved prefix `/__spindle/`; § 10 specifies their answers.
Exposing the deployment's samples as Prometheus text stays a later story.

## 8. Order

| # | story | needs |
|---|---|---|
| 1 | `telemetry-counters`: the `Telemetry` port, counters in kernel, store, channel and timer, the derived catalogue, `b10x-spindle metrics` | wave 1 |
| 2 | `telemetry-transport`: `spindle-telemetry/1` frames over a Unix socket and vsock; guest resource readings | 1 |
| 3 | `launcher-resource-sampling`: host-side CPU and memory per instance for process, QEMU, Firecracker | the target contract |
| 4 | `control-process-serves-the-api`: `b10x-spindle deploy`, collection, the read-only API with token and Host check | 2, 3 |
| 5 | `observe-ui`: Vue topology, instance and view screens, embedded | 4 |

The first visible result is story 1 plus the terminal view: `b10x-spindle play` can print the
counters under the board before any of the transport exists.

## 9. Open

- Sample period: 1 s fits rally (one tick per second). A busy service may want 250 ms or
  per-command tracing; tracing individual envelopes end to end (delivery id across components)
  is a later story.
- Whether `spindle.observe` is the right home for the API nouns or whether they belong in
  `spindle.placement` as views. Answered for the nouns of § 10: they live in `spindle.observe`.
  A request record, a metric family and a topology edge are what a served run answered or
  exposes, not state any command of `spindle.placement` decides, so there is no entity whose view
  they could be. Still open for the deploy API's nouns.

## 10. The reserved routes of a served run

`epic:runtime-observability`. `b10x-spindle run`, serving a specification over `http-loopback`,
reserves the prefix `/__spindle/` on its own listener. With `--observe` it answers the routes
below; without it every path under the prefix answers `404`. A specification route under the
prefix is refused at start, flag or not. No server code answers them yet: the nouns come first,
and later stories of the epic serve them.

| route | answer | format | noun in `spindle.observe` |
|---|---|---|---|
| `GET /__spindle/requests` | the totals per class since start and the last N requests, newest first; `?class=` keeps only the classes it names | `spindle-requests/1` | `RequestLog` (`RequestTotals`, `RequestRecord`, `RequestClass`, `OperationKind`, `ExternalAnswer`, `AnswerSource`, `UnmetInput`) |
| `GET /__spindle/metrics` | Prometheus text, `text/plain; version=0.0.4` | the exposition format | `MetricFamilies` (`MetricFamily`, `MetricKind`): the families exposed, by name, type and labels |
| `GET /__spindle/ui` | the observe panel: one HTML page, script and style inline, framable by any page; its `Content-Security-Policy` allows inline script and style and `connect-src 'self'` only; it reads `./requests` beside itself; `503` from a binary built without `ui/dist-panel/panel.html` | `text/html` | none: it renders `RequestLog` |
| `GET /__spindle/topology` | the graph of the served specifications, with live numbers per edge | `spindle-topology/1` | `Topology` (`TopologyNode`, `NodeKind`, `TopologyEdge`, `EdgeKind`, `EdgeTraffic`, `StatusCount`, `SourceCount`) |

- **The class follows who answered, not the status.** A declared outcome of a command, or a
  view's rows, is `served`, whatever status the binding gives it: `410 already-done`, and also a
  declared outcome bound to `501` or `503`. `not-served` is only spindle's own `501`, for an
  operation the ingress cannot route, the kernel does not run or the host does not read, or for an
  external cause nothing answered. `failed` is only a `5xx` spindle itself answered other than
  that `501`. `403` "not granted" is `not-granted`; any other `4xx` the ingress or ESS's standard
  refusal answered before the model is `refused`; no route or wrong method is `unrouted`. A
  record's `reason` is the answer's `refused` text, absent for `served`.
- **Requests under `/__spindle/` are neither recorded nor counted**, so polling does not move a
  number. A reset clears no record and no counter.
- **The metric families are a contract.** The exposition's `# TYPE` lines equal
  `ess/fixtures/observe/metrics/families.json` in name, kind and order. The counters of
  `spindle-telemetry`'s catalogue (§ 2) are exposed under these families, with their dotted names
  split into labels.
- **The topology is derived, never authored**: from the loaded specifications and the routes the
  ingress serves. A `contains` edge carries no traffic. Node ids are unique within one
  specification's topology: `ingress`; the specification by its system identifier prefixed by
  its kind (`specification:tasks`), so a system called `ingress` is not the ingress; a domain,
  command, view or event by its qualified name prefixed by its kind
  (`command:tasks.board.OpenTask`); an external cause by its command and outcome
  (`external:tasks.ledger.Settle/settled`); a binding by its binding id,
  which is not a qualified name (`binding:record-closure`). A binding id is unique only within its
  specification, so ids across several specifications behind one ingress are
  `story:one-ingress-several-specifications`'s to settle.
- **Formats are enforced.** `format` is a one-value enum in each answer (`spindle-requests/1`,
  `spindle-topology/1`), so the schema refuses any other; a status class is one of `2xx`, `3xx`,
  `4xx`, `5xx`; a request log's `capacity` is 1 to 10 000.

The nouns are projected and checked as the deploy API's are (§ 5): `OBSERVE_ROOTS` in
`Taskfile.yml` names `RequestLog`, `MetricFamilies` and `Topology`, `task generate:observe`
writes their JSON Schema, Rust (`spindle-observe-api`) and TypeScript (`ui/src/api/generated`),
and `task check` fails on drift. The fixtures are `ess/fixtures/observe/requests/scripted.json`
(the epic's scripted sequence on the `ingress` test specification), `requests/declared-5xx.json`
(declared outcomes bound to `501` and `503`, each `served`), `metrics/families.json` and
`topology/tasks.json` (the same specification after that sequence); each validates against its
schema, and `ui/src/api/types.test.ts` reads each through its generated type.
