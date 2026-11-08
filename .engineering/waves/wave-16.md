# Wave 16

**Delivered.** PR https://github.com/beyond10x/spindle/pull/35 (head d5254a15) merged as 1a3879aa; released as 0.8.0 (`verification-report:release-0.8.0`). The three stories moved to `implemented` on CI run 42499657344 (Gate and macOS). Units merged into `wave/16`: U3 as aa432f2e, U1 as 0dd405ed
(`review-result:w16-u1-adversary-1`), U2 as cd444acf (`review-result:w16-u2-adversary-1`).

Run by the coordinator under `approval-record:wave16-approval` (the operator's standing acceptance
of future waves). Skill `aep:implementing`, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/16`, forked from `wave/15` at 6b5d5342 (all five wave 15 units merged)
while wave 15's full gate runs; it is merged with `main` once wave 15 lands. Managed tree
`spindle-wave16`. The ESS pin 0.90.0 stays.

Width: 3 units. Every other active story collides with one of them: `story:kernel-guard-reads-now`,
`story:kernel-secondary-effects`, `story:state-change-conditions` and
`story:response-field-value-source` share `crates/spindle-kernel/src/execute.rs` with U1;
`story:live-topology` shares `crates/spindle-ingress/src/observe.rs` with U2 and `surface.rs` with
U3.

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:kernel-creation-takes-unknown-row` | `unit/w16-unknown` | `spindle-w16-unknown` | `aep:implementor` | delivered |
| U2 | `story:external-answers-observed` | `unit/w16-external` | `spindle-w16-external` | `aep:implementor` | delivered |
| U3 | `story:surface-cross-component-template-collision` | `unit/w16-collision` | `spindle-w16-collision` | `aep:implementor` | delivered |

Each unit gets one `aep:adversary` pass when its implementor reports green.

## Surfaces

- U1: `crates/spindle-kernel` (`execute.rs`, `execute/delete.rs`, a new test module).
- U2: `crates/spindle-cli` (`decisions.rs`, `serve.rs`, tests), `crates/spindle-ingress/src/observe.rs`,
  `ess/` (spindle.observe) and its generated types.
- U3: `crates/spindle-ingress/src/surface.rs` and its adversary test.

All three may add a CHANGELOG entry; the merge resolves it.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run), a `verification`
and, where the adversary found something, a `review_outcome`.
