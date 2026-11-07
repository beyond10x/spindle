# Wave 13

Run by the coordinator under `approval-record:wave13-approval` (the operator's standing acceptance
of future waves). Skill `aep:implementing`, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/13`, forked from `store/wave12-close` f1c16c9 (wave 12's store close rides
this wave's PR, `approval-record:wave12-close-branch`), managed tree `spindle-wave13`. ESS pin
0.88.0 moves to 0.89.0, the newest ESS release, in U4.

Width: 6 units. 4 at start; the operator accepted `story:browser-store-several-tabs` and
`story:links-hello-test-timing` from draft into the wave (U5, U6). Wave 1 of the derived plan holds three
stories, and every other active story collides with one of them (verb lines below). Free disk on
the host was 22G at start (`df -h /`), so builds are admitted one by one through the host's build
queue.

## Units

| unit | story | branch | tree (managed id) | model | stage |
|---|---|---|---|---|---|
| U1 | `story:ingress-request-log` | `unit/w13-reqlog` | `spindle-w13-reqlog` | opus | building |
| U2 | `story:kernel-related-row-values` | `unit/w13-relvalues` | `spindle-w13-relvalues` | opus | building |
| U3 | `story:service-worker-serves-a-specification` | `unit/w13-sworker` | `spindle-w13-sworker` | opus | building |
| U4 | ESS pin 0.88.0 to 0.89.0 | `unit/w13-esspin` | `spindle-w13-esspin` | sonnet | building |
| U5 | `story:browser-store-several-tabs` | `unit/w13-tabs` | `spindle-w13-tabs` | opus | building |
| U6 | `story:links-hello-test-timing` | `unit/w13-hello` | `spindle-w13-hello` | sonnet | building |

Trees live under the worktree manager's `trees/b10x/spindle/`.

## Surfaces

- U1: `crates/spindle-ingress`, `crates/spindle-cli` (`cli.rs`, `serve.rs`, tests), `README.md`.
- U2: `crates/spindle-kernel` (`supported.rs`, `values.rs`, tests).
- U3: new `crates/spindle-wasm` and `web/`; the workspace member list, `Taskfile.yml` and CI for
  the bundle build. It uses `crates/spindle-store-browser` and does not edit it.
- U5: `crates/spindle-store-browser` (src and its adversary test).
- U6: `crates/spindle-host/tests/spindle-host/adversary_links.rs`.
- U4: the ESS tags in `Cargo.toml`, `Cargo.lock`, the workflows' `ess` version, regenerated
  output, `conformance-known-failures.json`.

U3 and U4 both touch `Cargo.toml`, `Cargo.lock` and `.github/workflows/ci.yml`, on different lines;
the merge resolves the lock by regeneration.

The condition of `verification-report:release-0.3.0`: the next wave that touches
`crates/spindle-store-browser` takes `story:browser-store-several-tabs` and removes the ignore at
`crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs:131`. This wave takes
it as U5, which owns `crates/spindle-store-browser/src`; U3 leaves that crate untouched.

## Derived plan

`aep plan artifact waves --kind story --status active`, aep 0.79.0, at the wave's start:

```
wave 1
  story:ingress-request-log (inferred)
  story:kernel-related-row-values (inferred)
  story:service-worker-serves-a-specification (inferred)
wave 2
  story:ingress-observe-panel (inferred)
  story:kernel-row-set-guards (inferred)
wave 3
  story:ingress-prometheus-metrics (inferred)
  story:kernel-set-outcomes (inferred)
wave 4
  story:external-answers-observed (inferred)
  story:response-field-value-source (inferred)
wave 5
  story:live-topology (inferred)
  story:state-change-conditions (inferred)
wave 6
  story:kernel-secondary-effects (inferred)
6 wave(s), 50 collision(s), 0 unassessed
```
