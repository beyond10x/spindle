---
format: aep.planning-md/3
id: story:node-export-release-artifact
kind: story
status: draft
title: The release builds the Node module as an artifact
relations:
- serves: vision:spindle
- decomposes: epic:browser-host
- depends_on: story:node-export-serves-a-specification
revision: 1
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
- The archive's `.wasm` loads in the Node lane's first test (`check:node_serves_a_specification`)
  when pointed at the extracted archive.
- The release notes carry one line `spindle-node wasm: <bytes> bytes`.

## Depends on

- `story:node-export-serves-a-specification`.

## Scope

- `.github/workflows/release.yml`, `AGENTS.md` (§ Cutting a release) (inferred).
