# Wave 25

**In progress.** Run by the coordinator under `approval-record:wave25-approval`, on the host
operator's order of 2026-11-26 for bigger waves (one integration branch, one PR). Skill
`aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/25`, forked from `main` at 14297803 (PR 89 merged), in managed tree
`spindle-wave-21`. The ESS pin 0.99.1 stays. The release that follows carries `story:view-paging`
(PR 88), `story:command-with-unpublished-event-served` (PR 89) and this wave.

## Selection

The host operator's story order after 0.20.1 named `story:planning-only-prs-skip-tests` and
`story:decisions-rules-on-input-predicates` (with a reset that carries the decisions). The open
consumer gaps of the store joined them: `story:guard-over-absent-optional-input`,
`story:identity-mint-types-and-fallback`, `story:reset-reapplies-seed`,
`story:world-row-integer-fields`; and the two test-budget defects the organisation rules put ahead
of features: `story:test-step-within-budget`, `story:wasm-lanes-within-budget`. Each passed the four
plan critics in up to five rounds (`review-result:wave25-*-r1` to `-r5`) before its move to
`active`.

The three gate stories share `Taskfile.yml` (task `check`) and `.github/workflows/ci.yml` (job
`Gate`), so one unit builds them in order on one branch (`depends_on` edges). The reset-carries-
decisions ask already holds for `run` (`served_decider_survives_a_console_reset`); the decisions
story adds the Node proof.

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:planning-only-prs-skip-tests`, `story:test-step-within-budget`, `story:wasm-lanes-within-budget` | `unit/w25-gate` | `spindle-w25-gate` | `aep:implementor` | planned |
| U2 | `story:decisions-rules-on-input-predicates` | `unit/w25-decisions` | `spindle-w25-decisions` | `aep:implementor` | planned |
| U3 | `story:identity-mint-types-and-fallback` | `unit/w25-mint` | `spindle-w25-mint` | `aep:implementor` | planned |
| U4 | `story:guard-over-absent-optional-input` | `unit/w25-guard` | `spindle-w25-guard` | `aep:implementor` | planned |
| U5 | `story:reset-reapplies-seed` | `unit/w25-reseed` | `spindle-w25-reseed` | `aep:implementor` | planned |
| U6 | `story:world-row-integer-fields` | `unit/w25-worldint` | `spindle-w25-worldint` | `aep:implementor` | planned |

Trees live under the worktree manager's `trees/b10x/spindle/`. Builds and tests run through the
shared pool on the build node; a step that writes into the tree runs as `slot --fetch <path>`.
Each unit gets one `aep:adversary` pass when its implementor reports green.

## Surfaces

Each story's `## Scope` and `## Shared files` sections hold its surface. Shared, resolved by the
coordinator at merge: `CHANGELOG.md` `[Unreleased]`, the `mod` lines of the kernel and CLI test
`main.rs`, the `crates/spindle-cli/src/cli.rs` help hunks (U2, U5) and the generated
`website/docs/reference/cli.md`.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run) and, where the
adversary found something, a `review_outcome`.

## Commits

The unit commits, the merges into `wave/25`, review fix commits, the release preparation commit,
the closing store commit, one PR to `main` through the bot, and a release after the PR merges.
