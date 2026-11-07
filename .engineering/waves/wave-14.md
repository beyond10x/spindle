# Wave 14

**Delivered.** PR https://github.com/beyond10x/spindle/pull/29 (head 64e3d59) merged as 8930f63; released as 0.6.0 (assets uploaded by the backfill run 42398200165 after PR 30 renamed the panel artifact; `verification-report:release-0.6.0`). The four stories (U1 to U4) moved to `implemented` on CI run 42393316015 (Gate and macOS); U5 moved the ESS pin to 0.90.0.

Run by the coordinator under `approval-record:wave14-approval` (the operator's standing acceptance
of future waves). Skill `aep:implementing`, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/14`, forked from `main` at 081bee7, managed tree `spindle-wave14`. ESS
pin 0.89.0 moves to 0.90.0, the newest ESS release, in U5.

Width: 5 units. Wave 1 of the derived plan holds four stories with no collision among them; U5 is
the dependency move. Every other active story collides with one of the four (verb lines below).
Free disk on the host was 116G at start (`df -h /`), so builds are admitted one by one through the
host's build queue.

## Units

| unit | story | branch | tree (managed id) | model | stage |
|---|---|---|---|---|---|
| U1 | `story:ingress-error-object-bodies` | `unit/w14-errobj` | `spindle-w14-errobj` | opus | merged |
| U2 | `story:ingress-observe-panel` | `unit/w14-panel` | `spindle-w14-panel` | opus | merged |
| U3 | `story:kernel-row-set-guards` | `unit/w14-rowset` | `spindle-w14-rowset` | opus | merged |
| U4 | `story:view-field-literal-filters` | `unit/w14-viewfilt` | `spindle-w14-viewfilt` | opus | merged |
| U5 | ESS pin 0.89.0 to 0.90.0 | `unit/w14-esspin` | `spindle-w14-esspin` | sonnet | merged |

Trees live under the worktree manager's `trees/b10x/spindle/`.

## Surfaces

- U1: `crates/spindle-ingress` (`surface.rs`, `answer.rs`), a new spindle-cli fixture and test
  module.
- U2: `ui/` (a second Vite config, `panel.html`, `src/panel/`), `crates/spindle-cli/build.rs`,
  the `ui` arm of `crates/spindle-ingress/src/observe.rs`, `Taskfile.yml`, a new spindle-cli test.
- U3: `crates/spindle-kernel` (`supported.rs`, `execute.rs`, `store.rs`, a new test).
- U4: `crates/spindle-host/src/view.rs`, a new spindle-cli fixture and test module.
- U5: the ESS tags in `Cargo.toml`, `Cargo.lock`, the workflows' `ess` version, regenerated
  output, `conformance-known-failures.json`.

U1, U2 and U4 each add a module to the spindle-cli test binary's root, and U5 touches
`Cargo.lock`; the merges resolve those lines by hand and the lock by regeneration.

## Derived plan

`aep plan artifact waves --kind story --status active`, aep 0.79.0, at the wave's start:

```
wave 1
  story:ingress-error-object-bodies (inferred)
  story:ingress-observe-panel (inferred)
  story:kernel-row-set-guards (inferred)
  story:view-field-literal-filters (inferred)
wave 2
  story:ingress-prometheus-metrics (inferred)
  story:kernel-set-outcomes (inferred)
wave 3
  story:external-answers-observed (inferred)
  story:response-field-value-source (inferred)
wave 4
  story:live-topology (inferred)
  story:state-change-conditions (inferred)
wave 5
  story:kernel-secondary-effects (inferred)
5 wave(s), 35 collision(s), 0 unassessed
```
