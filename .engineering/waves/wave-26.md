# Wave 26

**Planned.** Run by the coordinator under `approval-record:wave26-approval`, on the host operator's
order of 2026-11-26, relaying the operator's words "resolve all missing spindle features". Skill
`aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/26`, forked from `main`, in managed tree `spindle-wave-21`. The ESS pin
0.99.1 stays.

## Selection

The missing features consumers reported against 0.21.0, each confirmed with a reproduction or a
cited line, and the release story for the Linux glibc floor. View filters come first, as promised
to the consumer whose journeys they block. `story:seed-once-on-durable-state` and
`story:stdin-arrange-and-line-references` follow in wave 27: they edit `serve.rs`, `seed.rs` and
the stdin loop after U3.

## Units

| unit | stories, in order | branch | tree |
|---|---|---|---|
| U1 | `story:related-selector-rows-quantifier`, `story:guard-over-absent-row-field` | `unit/w26-kernel-guards` | `spindle-w26-guards` |
| U2 | `story:view-filter-parameter-literal` | `unit/w26-filters` | `spindle-w26-filters` |
| U3 | `story:run-fails-on-rejected-line`, `story:clock-and-mint-in-every-mode` | `unit/w26-run` | `spindle-w26-run` |
| U4 | `story:scenario-act-without-outcome`, `story:scenario-without-components` | `unit/w26-scenario` | `spindle-w26-scenario` |
| U5 | `story:release-linux-glibc-floor-and-musl` | `unit/w26-release` | `spindle-w26-release` |

Each unit is built by one `aep:implementor` and gets one `aep:adversary` pass when green. Builds and
tests run through the shared pool; a step that writes into the tree runs as `slot --fetch <path>`.

## Surfaces

Each story's `## Scope` and `## Shared files` sections hold its surface. Shared, resolved by the
coordinator at merge: the `mod` lines of the test `main.rs` files, the `run` help in
`crates/spindle-cli/src/cli.rs` (U3, U4) with the generated `website/docs/reference/cli.md`,
`crates/spindle-docs/src/status.rs` with its generated pages (U3, U4), and `CHANGELOG.md`
`[Unreleased]`.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run) and, where the
adversary found something, a `review_outcome`.

## Commits

The unit commits, the merges into `wave/26`, review fix commits, the closing store commit, one PR to
`main` through the bot, and a release on the host operator's word after the PR merges.
