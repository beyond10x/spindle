# Wave 15

**Delivered.** PR https://github.com/beyond10x/spindle/pull/32 (head 2bda8cd8) merged as 82c4fb1a; released as 0.7.0 (`verification-report:release-0.7.0`). The five stories (U1 to U5) moved to `implemented` on CI run 42465501089 (Gate and macOS, attempt 5).

Run by the coordinator under `approval-record:wave15-approval` (the operator's standing acceptance
of future waves). Skill `aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/15`, forked from `main` at 61e946c, managed tree `spindle-wave15`. The ESS
pin 0.90.0 is the newest ESS release; no dependency move.

Width: 5 units. U1 and U2 started as wave 1 of the derived plan. Five drafts then passed the
plan-critic panel (`approval-record:drafts-panel-w15`): U4 and U5 share no file with U1 or U2 and
joined at once; U3 is the operator's accepted `story:kernel-row-set-commit-conflict`, which depends
on U2 and starts on `wave/15` after U2 merges. `story:kernel-creation-takes-unknown-row` and
`story:kernel-guard-reads-now` share `crates/spindle-kernel/src/execute.rs` with U2 and U3, so they
wait for the next wave; every other active story collides with a unit on
`crates/spindle-kernel/src/{execute,store,supported}.rs` or `crates/spindle-ingress/src/observe.rs`.
Free disk on the host was 146G at start (`df -h /`); builds are admitted one by one through the
host's build queue.

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:ingress-prometheus-metrics` | `unit/w15-prom` | `spindle-w15-prom` | `aep:implementor` | delivered |
| U2 | `story:kernel-set-outcomes` | `unit/w15-setout` | `spindle-w15-setout` | `aep:implementor` | delivered |
| U3 | `story:kernel-row-set-commit-conflict` | `unit/w15-rowset` | `spindle-w15-rowset` | `aep:implementor` | delivered |
| U4 | `story:links-second-hello-race` | `unit/w15-hello` | `spindle-w15-hello` | `aep:implementor` | delivered |
| U5 | `story:route-literal-segment-precedence` | `unit/w15-route` | `spindle-w15-route` | `aep:implementor` | delivered |

Trees live under the worktree manager's `trees/b10x/spindle/`; each builds into its own `target/`.
Each unit gets one `aep:adversary` pass when its implementor reports green.

## Surfaces

- U1: `crates/spindle-ingress` (`observe.rs`, new `observe/metrics.rs`, `console.rs`),
  `crates/spindle-host/src/host.rs` (rows per entity), `crates/spindle-telemetry` read only,
  spindle-cli tests and a dev-dependency.
- U2: `crates/spindle-kernel` (`supported.rs`, `execute.rs`, `store.rs`, `values.rs`, a new test
  module).

- U3: `crates/spindle-kernel` (`store.rs`, `execute.rs`, `sqlite.rs`, `contract.rs`, tests),
  `crates/spindle-host/src/host.rs` and `crates/spindle-store-browser` (every `impl Store` and
  `Change` site). Starts from `wave/15` after U2; merged after U1 (shared `host.rs`).
- U4: `crates/spindle-host/tests/spindle-host/adversary_links.rs` (one test); no host code.
- U5: `crates/spindle-ingress/src/surface.rs` (`Surface::resolve`), a new spindle-cli test module
  and fixture.

U1 touches `Cargo.lock` through its dev-dependency; the others do not. U1 and U5 may both add a
`mod` line to `crates/spindle-cli/tests/spindle-cli/main.rs`; the merge resolves it.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run), a `verification`
and, where the adversary found something, a `review_outcome`.

## Derived plan

`aep plan artifact waves --kind story --status active` at the wave's start:

```
wave 1
  story:ingress-prometheus-metrics (inferred)
  story:kernel-set-outcomes (inferred)
wave 2
  story:external-answers-observed (inferred)
  story:response-field-value-source (inferred)
wave 3
  story:live-topology (inferred)
  story:state-change-conditions (inferred)
wave 4
  story:kernel-secondary-effects (inferred)
```

## Commits

The unit commits, the merges into `wave/15`, review fix commits, the release preparation commit,
the closing store commit, one PR to `main` through the bot, and a release after the PR merges.
