# Wave 15

**Building.**

Run by the coordinator under `approval-record:wave15-approval` (the operator's standing acceptance
of future waves). Skill `aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/15`, forked from `main` at 61e946c, managed tree `spindle-wave15`. The ESS
pin 0.90.0 is the newest ESS release; no dependency move.

Width: 2 units. Wave 1 of the derived plan holds two stories with no collision between them; every
other active story collides with one of them on `crates/spindle-kernel/src/{execute,store,supported}.rs`
or `crates/spindle-ingress/src/observe.rs` (verb lines below). Free disk on the host was 146G at
start (`df -h /`); builds are admitted one by one through the host's build queue.

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:ingress-prometheus-metrics` | `unit/w15-prom` | `spindle-w15-prom` | `aep:implementor` | dispatched |
| U2 | `story:kernel-set-outcomes` | `unit/w15-setout` | `spindle-w15-setout` | `aep:implementor` | dispatched |

Trees live under the worktree manager's `trees/b10x/spindle/`; each builds into its own `target/`.
Each unit gets one `aep:adversary` pass when its implementor reports green.

## Surfaces

- U1: `crates/spindle-ingress` (`observe.rs`, new `observe/metrics.rs`, `console.rs`),
  `crates/spindle-host/src/host.rs` (rows per entity), `crates/spindle-telemetry` read only,
  spindle-cli tests and a dev-dependency.
- U2: `crates/spindle-kernel` (`supported.rs`, `execute.rs`, `store.rs`, `values.rs`, a new test
  module).

U1 touches `Cargo.lock` through its dev-dependency; U2 does not.

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
