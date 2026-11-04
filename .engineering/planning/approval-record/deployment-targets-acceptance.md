---
format: aep.planning-md/3
id: approval-record:deployment-targets-acceptance
kind: approval-record
status: accepted
title: Operator accepts the deployment-target contract, QEMU and Firecracker launchers
relations:
- decides: story:deployment-target-contract
- decides: story:qemu-microvm-launcher
- decides: story:firecracker-launcher
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-04T09:17:18Z", actor: "human:timo", revision: 2}
---
# Operator acceptance of the deployment-target stories

## Words

The operator, 2026-11-04:

> We should start just with QEMU, Firecracker, and then go from there. So substrat could be too limited, and we might not want to open it just for this.

and, after being told the target contract and the QEMU and Firecracker launchers were drafted:

> speed up impl by dispatching more sub-agents

## What it accepts

- `story:deployment-target-contract`
- `story:qemu-microvm-launcher`
- `story:firecracker-launcher`

QEMU before Firecracker. Docker, Compose, Kubernetes and browser targets are not accepted. Substrate
is not opened for this.
