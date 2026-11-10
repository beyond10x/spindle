# Wave 18

**Running.** Run by the coordinator under `approval-record:wave18-approval`, on the operator's
standing acceptance of future waves (`approval-record:wave16-approval`: "Timo accepted all waves
and the next 10 future waves"). Skill `aep:implementing` 0.26.0, wave mode.

Integration branch `wave/18`, forked from `main` at 244fb60e (release 0.9.0), in managed tree
`spindle-wave18-plan`. The ESS pin 0.90.0 stays.

## Selection

`aep plan artifact waves --kind story --status active` (aep 0.79.0), after `story-scoper` runs on
the 10 unscoped active stories on 2026-11-10, places three stories in wave 1:

- `story:external-cause-bound-to-http-call`
- `story:kernel-existing-instance`
- `story:live-topology`

`story:kernel-guard-reads-now` joins them: `approval-record:downstream-gaps-acceptance-4` asks for
it in the first wave after wave 17. The verb puts it in wave 2 for two inferred collisions with
`story:kernel-existing-instance`, both split below (*Surfaces*).

Left for later waves: `story:http-caller-proves-actor` waits on the design decision in
`docs/design/spindle.md` § 11 ("how an HTTP caller proves its actor": undecided), which the scope
found still open (`ess/components.yaml:37-43` holds it as `UNMAPPED:`).

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:kernel-guard-reads-now` | `unit/w18-guard-now` | `spindle-w18-guard-now` | `aep:implementor` | dispatched |
| U2 | `story:kernel-existing-instance` | `unit/w18-existing` | `spindle-w18-existing` | `aep:implementor` | dispatched |
| U3 | `story:external-cause-bound-to-http-call` | `unit/w18-http-cause` | `spindle-w18-http-cause` | `aep:implementor` | dispatched |
| U4 | `story:live-topology` | `unit/w18-topology` | `spindle-w18-topology` | `aep:implementor` | dispatched |

Each tree builds into its own `target/`. Each unit gets one `aep:adversary` pass when its
implementor reports green, at most two.

## Surfaces

- `crates/spindle-kernel/src/execute.rs`: U1 owns the guard reads and the clock (`facts.rs`, the
  guard evaluation). U2 owns `Run::select`'s `Phase::Existence` and `Phase::RelatedRow` arms and
  `Run::truth`. Neither edits the other's functions.
- `crates/spindle-kernel/src/supported.rs`: U2 only.
- `crates/spindle-capability/src/environment.rs`, `crates/spindle-kernel/src/decider.rs`,
  `crates/spindle-cli/src/{serve,decisions,main,cli}.rs`: U3. An HTTP client is not added to
  `spindle-host` or `spindle-capability`, which compile into the browser worker.
- `crates/spindle-host/src/host.rs`, `crates/spindle-ingress/src/{surface,observe}.rs`,
  `crates/spindle-ingress/src/observe/`, `ui/`: U4.

## Coordinator's

- `crates/spindle-kernel/tests/spindle-kernel/main.rs` and `crates/spindle-cli/tests/spindle-cli/main.rs`
  `mod` lines: a unit may append one, and the coordinator resolves the merge.
- `CHANGELOG.md` `[Unreleased]`: each unit adds an entry, and the merge keeps all of them.
- `Cargo.lock`, `conformance.toml` and generated output (`ess/generated/`,
  `crates/spindle-observe-api/`, `ui/src/api/generated/`, `website/docs/reference/cli.md`): the
  coordinator regenerates these after each merge.
- `docs/design/spindle.md`: U3 adds its decision row; the coordinator resolves the merge.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run), a `verification`
and, where the adversary found something, a `review_outcome`.

## Commits

The approval commit, the unit commits, the merges into `wave/18`, review fix commits, the closing
store commit, one PR to `main` through the bot merged when CI's `Gate` is green, and a release
within 4 h of the merge.
