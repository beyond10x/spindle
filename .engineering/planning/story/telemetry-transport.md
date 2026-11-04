---
format: aep.planning-md/3
id: story:telemetry-transport
kind: story
status: active
title: Telemetry transport
relations:
- serves: vision:spindle
- decomposes: epic:observe
- depends_on: story:telemetry-counters
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:52Z", actor: "human:timo", revision: 3}
---
# Telemetry transport

## Outcome

Each instance sends a `spindle-telemetry/1` sample every second to the endpoint given as `SPINDLE_TELEMETRY`: a Unix socket for a process, vsock for QEMU and Firecracker. Guest resource readings ride in the sample. Design § 3.

## Acceptance

- `a_lost_frame_loses_no_count`
- `a_missing_collector_drops_frames_and_counts_the_drops`
- `a_qemu_instance_reports_over_vsock` (skipped without KVM)
