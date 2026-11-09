# Wave 17

**Running.** Run by the coordinator under `approval-record:wave17-approval`. The operator named the
ESS review's high findings as one wave, and the standing acceptance in
`approval-record:wave16-approval` covers it. Skill `aep:implementing` 0.26.0, wave mode. The
stories decompose `epic:spec-states-runtime` and serve `vision:spindle`. The source is
`review-result:ess-review-2026-11-09`.

Integration branch `wave/17`, forked from `main` at 98212306, in managed tree `spindle-wave17`.
The ESS pin 0.90.0 stays.

Width: 8 units, one wave, as the operator asked. `aep plan artifact waves --kind story --status
active` (aep 0.79.0) puts these stories in waves 1 to 3. It reports 17 collisions among them, 2
cited and 15 inferred, listed below. The wave runs anyway with a split by section per file
(*Surfaces*). The coordinator runs `git merge-tree --write-tree` before each merge and owns every
shared file listed under *Coordinator's*. Scope was written by the coordinator from the review's
citations, without `story-scoper` runs.

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:admission-decision-checked-by-model` | `unit/w17-admission` | `spindle-w17-admission` | `aep:implementor` | planned |
| U2 | `story:placement-refusals-match-target` | `unit/w17-placement` | `spindle-w17-placement` | `aep:implementor` | planned |
| U3 | `story:unknown-component-refused-at-deploy` | `unit/w17-unknown-component` | `spindle-w17-unknown-component` | `aep:implementor` | planned |
| U4 | `story:environment-implementations-specified` | `unit/w17-environment` | `spindle-w17-environment` | `aep:implementor` | planned |
| U5 | `story:store-reset-and-world-specified` | `unit/w17-world` | `spindle-w17-world` | `aep:implementor` | planned |
| U6 | `story:periodic-timing-specified` | `unit/w17-periodic` | `spindle-w17-periodic` | `aep:implementor` | planned |
| U7 | `story:binding-delivery-specified` | `unit/w17-delivery` | `spindle-w17-delivery` | `aep:implementor` | planned |
| U8 | `story:ingress-caller-identity-specified` | `unit/w17-caller` | `spindle-w17-caller` | `aep:implementor` | planned |

Each tree builds into its own `target/`. Scratch for each unit is `w17/<unit>` under the
coordinator session's scratch directory. Each unit gets one `aep:adversary` pass when its
implementor reports green.

## Surfaces

- `ess/domains/deployment.yaml`: U4 owns `Environment` (lines 40-59) and `RegisterEnvironment`
  (140-162). U8 owns the actor block (104-118), attributes and summaries only. U1 owns the rest.
- `ess/domains/placement.yaml`: U8 owns the actor block (130-145), attributes and summaries only.
  U2 owns the rest.
- `ess/domains/observe.yaml`: U6 owns `Binding` and `BindingKind` (50-74). U5 owns `AnswerSource`
  (273-280) and appends after the last type. U7 appends after the last type.
- `ess/components.yaml`: U8. U7 writes a patch to its scratch instead of editing.
- `crates/spindle-control/src/control.rs`: U3. U1 writes a patch to its scratch instead of editing.
- `crates/spindle-target/src/placement.rs`: U2.
- Kernel and CLI tests that send `OfferBundle` or `DecideAdmission`: U1.

## Coordinator's

- `ess/system.yaml` `domains:` and `ess/ess-inputs.yaml` `specification:`. A unit may append a
  line, and the coordinator resolves the merge.
- `conformance-known-failures.json`, `conformance.toml` and the generated observe output
  (`ess/generated/`, `crates/spindle-observe-api/`, `ui/src/api/generated/`). The coordinator
  re-pins or regenerates these after each merge.
- `CHANGELOG.md` `[Unreleased]`: each unit may add an entry, and the merge keeps all of them.

## Collisions among the wave's stories, verbatim

```
collision: story:admission-decision-checked-by-model story:environment-implementations-specified ess/domains/deployment.yaml
collision: story:admission-decision-checked-by-model story:ingress-caller-identity-specified ess/domains/deployment.yaml (inferred)
collision: story:admission-decision-checked-by-model story:placement-refusals-match-target conformance-known-failures.json (inferred)
collision: story:admission-decision-checked-by-model story:unknown-component-refused-at-deploy crates/spindle-control/src/control.rs (inferred)
collision: story:binding-delivery-specified story:environment-implementations-specified ess/ess-inputs.yaml (inferred)
collision: story:binding-delivery-specified story:environment-implementations-specified ess/system.yaml (inferred)
collision: story:binding-delivery-specified story:ingress-caller-identity-specified ess/components.yaml (inferred)
collision: story:binding-delivery-specified story:periodic-timing-specified ess/domains/observe.yaml (inferred)
collision: story:binding-delivery-specified story:store-reset-and-world-specified ess/domains/observe.yaml (inferred)
collision: story:binding-delivery-specified story:store-reset-and-world-specified ess/ess-inputs.yaml (inferred)
collision: story:binding-delivery-specified story:store-reset-and-world-specified ess/system.yaml (inferred)
collision: story:environment-implementations-specified story:ingress-caller-identity-specified ess/domains/deployment.yaml (inferred)
collision: story:environment-implementations-specified story:store-reset-and-world-specified ess/ess-inputs.yaml (inferred)
collision: story:environment-implementations-specified story:store-reset-and-world-specified ess/system.yaml (inferred)
collision: story:ingress-caller-identity-specified story:placement-refusals-match-target ess/domains/placement.yaml (inferred)
collision: story:periodic-timing-specified story:store-reset-and-world-specified crates/spindle-observe-api (inferred)
collision: story:periodic-timing-specified story:store-reset-and-world-specified ess/domains/observe.yaml
```

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run), a `verification`
and, where the adversary found something, a `review_outcome`. A story that ends in a recorded
decline (a `specification` artifact) leaves the wave and is reported as such.

## Commits

The unit commits, the merges into `wave/17`, review fix commits, the closing store commit, and one
PR to `main` through the bot, merged when CI's `Gate` is green. No release.
