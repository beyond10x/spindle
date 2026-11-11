---
format: aep.planning-md/3
id: story:release-dependency-list
kind: story
status: implemented
title: A released binary carries its dependency list for CVE scanners
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:52:55Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T09:52:55Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:04Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

Every released `b10x-spindle` binary carries its dependency list in a form that `trivy` and `grype`
read, so an image scan of a deployment reports CVEs in Spindle's crates.

## Why

Reported by a downstream consumer's image review on 2026-11-11: `trivy` scanning an image that holds
the 0.13.0 binary lists no Rust crates for it, so no dependency of Spindle is ever checked for a
known vulnerability in that deployment.

## Acceptance

1. The release workflow builds the four target binaries with `cargo auditable build`, so each
   carries the `.dep-v0` section; the workflow fails when the tool is missing.
2. A test or CI step on the release build runs `rust-audit-info` (or `cargo audit bin`) on the
   linux x86_64 binary and finds at least the workspace crates and `eventlog-postgres`.
3. The release page also carries one CycloneDX SBOM per release (`b10x-spindle-<v>.cdx.json`),
   listed in `SHA256SUMS`.
4. `website/docs` states where the dependency list and the SBOM are found.

## Scope

- `.github/workflows/release.yml`, inferred
- `website/docs` install or release page, inferred
