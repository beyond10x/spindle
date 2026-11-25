---
format: aep.planning-md/3
id: story:node-export-release-artifact
kind: story
status: draft
title: The release builds the Node module as an artifact
relations:
- serves: vision:spindle
- depends_on: story:node-export-serves-a-specification
- decomposes: epic:node-export
scope:
- confidence: inferred
  path: .github/workflows/release.yml
- confidence: inferred
  path: AGENTS.md
revision: 4
---
# The release builds the Node module as an artifact

## Outcome

Each release uploads the Node module (the `wasm-bindgen --target web` output of `spindle-node`:
the `.wasm`, its ES module glue and its TypeScript declarations) as one archive beside the
`b10x-spindle` archives, covered by `SHA256SUMS`, and the release notes state the `.wasm` size in
bytes, so a consumer can depend on a released module instead of building one.

## Acceptance

- `release.yml` builds the module once per release and uploads `spindle-node-<version>.tar.gz`;
  a pull request that changes the workflow builds it and publishes nothing (the rule the other
  archives follow).
- The release workflow runs the Node lane's tests (`crates/spindle-node/tests/node/*.test.mjs`)
  with `SPINDLE_NODE_MODULE` set to the extracted archive's directory, the input
  `story:node-export-serves-a-specification` gives the lane, and uploads only when they pass.
- The release notes carry one line `spindle-node wasm: <bytes> bytes`.
- `sha256sum --check SHA256SUMS` passes for the downloaded `spindle-node-<version>.tar.gz`, and
  the archive lists exactly `spindle_node_bg.wasm`, `spindle_node.js` and `spindle_node.d.ts`.

## Depends on

- `story:node-export-serves-a-specification`.

## Scope

- `.github/workflows/release.yml`, `AGENTS.md` (§ Cutting a release) (inferred).
