---
format: aep.planning-md/3
id: story:telemetry-transport
kind: story
status: implemented
title: Telemetry transport
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:telemetry-counters
- depends_on: story:observe-api-contract
- depends_on: story:qemu-microvm-launcher
- depends_on: story:firecracker-launcher
scope:
- confidence: cited
  path: crates/spindle-firecracker
- confidence: cited
  path: crates/spindle-qemu
- confidence: cited
  path: crates/spindle-target
- confidence: cited
  path: crates/spindle-telemetry
revision: 10
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-05T07:49:23Z", actor: "human:timo", revision: 10, decided_on: {"recorded":{"test_result":2,"review_outcome":3}}}
---
# Telemetry transport

## Outcome

An instance that starts the sender sends a `spindle-telemetry/1` frame every sample period (1 s by default) to the
endpoint its launcher gives it as `SPINDLE_TELEMETRY`: a Unix socket for a process, vsock for QEMU
and Firecracker. A frame carries the instance id, a monotonic sequence, the cumulative counters
and histogram buckets of `spindle-telemetry`, the last answers, and the guest's own resource
readings (`/proc/stat`, `/proc/meminfo`, `/proc/self/io`). Its shape is the `spindle.observe`
`Sample` already specified (`ess/domains/observe.yaml`, schema
`ess/generated/jsonschema/schema/types/spindle.observe.Sample.schema.json`). An instance never
blocks a command on telemetry: with no collector, or a slow one, it drops frames and counts the
drops. Design `docs/design/observe.md` § 3 and § 4.

## Acceptance

Each is a named test; VM cases are `#[ignore]`d and run by `task check:vm`.

- `a_lost_frame_loses_no_count` — frames 1 and 3 delivered, frame 2 dropped: the collector's
  totals after frame 3 equal the instance's counters (cumulative counters).
- `a_missing_collector_drops_frames_and_counts_the_drops` — `SPINDLE_TELEMETRY` names a socket
  nobody listens on: recording into the counters still returns at once, and the next frame a collector receives reports the
  dropped count.
- `a_frame_validates_against_the_sample_schema` — a frame the sender writes validates against
  `spindle.observe.Sample` (the schema `task check:every_fixture_answer_validates_against_its_schema`
  uses).
- `a_process_instance_reports_over_a_unix_socket` — the process launcher sets `SPINDLE_TELEMETRY`,
  and a collector on that socket receives frames with increasing sequence numbers.
- `the_frame_carries_the_guest_resource_readings` — CPU seconds and memory in use read inside the
  instance are present and non-zero.
- `a_qemu_instance_reports_over_vsock` — on a host with `/dev/kvm` and `/dev/vhost-vsock`, a
  collector on the host vsock endpoint receives frames from the guest with increasing sequence
  numbers; skipped with its reason without them (run by `task check:vm`).
- `a_firecracker_instance_reports_over_vsock` — on a host with `/dev/kvm` and the Firecracker
  binary, a collector on the vsock UDS receives frames from the guest with increasing sequence
  numbers; skipped with its reason without them (run by `task check:vm`).

## Scope

- `crates/spindle-telemetry` — cited: the frame and the sender sit beside the `Telemetry` port
  and counters (`src/port.rs`, `src/counters.rs`).
- `crates/spindle-target` — cited: the process launcher sets the `SPINDLE_*` variables
  (`src/process.rs`) and gains `SPINDLE_TELEMETRY` and the socket.
- `crates/spindle-qemu` — cited: the vsock device in the QEMU argv and the guest init.
- `crates/spindle-firecracker` — cited: the vsock device in the machine configuration and the
  guest init (`src/bin/spindle-guest-init.rs`).
Not in scope: `crates/spindle-cli` and `crates/spindle-host`. Decided after the round-1 critics,
2026-11-04: this story ships the sender as a hook in `crates/spindle-telemetry` (start a sender
from a `Counters` and the `SPINDLE_TELEMETRY` endpoint; it reads `Counters::snapshot()`,
`src/counters.rs:150`) and does not wire it into `b10x-spindle`. Its process test drives the hook
from the existing stand-in instance `crates/spindle-target/src/bin/spindle-target-fixture.rs`; the
VM tests drive it from the guest probe and init binaries. The per-component entry point that a
launcher starts is `story:rally-runs-as-three-processes`'s, and that story wires the hook in
(`a_component_process_reports_telemetry_when_given_an_endpoint`) and depends on this one. So this
story shares no file with `story:replay-ess-scenarios` or `story:run-over-http-ingress`, which edit
`crates/spindle-cli/src/{cli.rs,main.rs}`.

Shared with `story:rally-runs-in-three-microvms`: the guest init binaries
(`crates/spindle-qemu/src/bin/init.rs`, `crates/spindle-firecracker/src/bin/spindle-guest-init.rs`)
gain the vsock endpoint here; that story runs after this one (`depends_on`) and starts the
per-component mode from the same init.

Shared, not typed: `crates/spindle-docs/src/status.rs` and the generated `website/` pages move the
"Telemetry transport" item from planned to shipped; `README.md` and `AGENTS.md` change in the same
commit (AGENTS.md § Documentation). The wave's coordinator resolves them at integration.

## Depends on

`story:telemetry-counters` (implemented), `story:observe-api-contract` (the `Sample` shape,
implemented), `story:qemu-microvm-launcher` and `story:firecracker-launcher` (the crates it
extends, implemented). Not on `story:replay-ess-scenarios`: no shared file (see Scope).
