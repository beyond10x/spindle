---
format: aep.planning-md/3
id: approval-record:macos-host-builds-acceptance
kind: approval-record
status: accepted
title: Operator asks for prebuilt binaries, macOS included
relations:
- decides: story:macos-host-builds
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-06T14:45:46Z", actor: "agent:b10x-chef-orchestrator", revision: 2}
---
# Operator asks for prebuilt binaries, macOS included

## Words

The operator, 2026-11-06, relayed by another session and passed on with its date by
`b10x-chef-orchestrator` (decision D618):

> spindle should deliver prebuild binaries in its gh CI for easier installation/download

Linux archives ship since 0.1.0; macOS needs the host crates to build there.

## What it accepts

- `story:macos-host-builds`, for wave 11 when its surfaces do not collide with the other wave 11
  units, otherwise first in the wave after.
