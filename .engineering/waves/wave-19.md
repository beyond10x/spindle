# Wave 19

**Running.** Run by the coordinator under `approval-record:wave19-approval`, on the operator's
standing acceptance of future waves (`approval-record:wave16-approval`) and his acceptance of the
PostgreSQL stories for this wave (`approval-record:postgres-wave19-acceptance`). Skill
`aep:implementing` 0.26.0, wave mode.

Integration branch `wave/19`, forked from `main` at 2d78103e (release 0.10.0), in managed tree
`spindle-wave18-plan`. The ESS pin 0.90.0 stays. Release 0.11.0 follows the merge.

## Selection

The operator put the PostgreSQL store first. `story:http-caller-proves-actor` and
`story:binding-where-condition` were announced to the downstream consumer for this wave. The verb
places `story:postgres-store` in its wave 3 for inferred collisions in
`crates/spindle-cli/src/{main,serve}.rs`, split below. `story:state-moves-to-postgres` depends on
`story:postgres-store` and is dispatched once U1 is merged.

Left for later waves: every other active story (`aep plan artifact waves --kind story --status
active`).

## Units

| unit | story | branch | tree (managed id) | agent | stage |
|---|---|---|---|---|---|
| U1 | `story:postgres-store` | `unit/w19-postgres` | `spindle-w19-postgres` | `aep:implementor` | dispatched |
| U2 | `story:http-caller-proves-actor` | `unit/w19-caller-proof` | `spindle-w19-caller-proof` | `aep:implementor` | dispatched |
| U3 | `story:binding-where-condition` | `unit/w19-binding-where` | `spindle-w19-binding-where` | `aep:implementor` | dispatched |
| U4 | `story:state-moves-to-postgres` | `unit/w19-state-move` | `spindle-w19-state-move` | `aep:implementor` | after U1 merges |

Each tree builds into its own `target/`. Each unit gets one `aep:adversary` pass when its
implementor reports green, at most two.

## Surfaces

- `crates/spindle-kernel/src/{postgres,sqlite}.rs`, `crates/spindle-kernel/Cargo.toml`,
  `Cargo.toml`, `crates/spindle-host/src/host.rs` (store choice), `crates/spindle-capability/src/environment.rs`
  (`store.durable` implementations): U1.
- `crates/spindle-cli/src/serve.rs`: U1 owns the store choice (`serve.rs:193-220`); U2 owns the
  caller proof options and their wiring into the surface. Neither edits the other's lines.
- `crates/spindle-cli/src/main.rs` and `cli.rs`: U1 adds the `store migrate` subcommand and the
  store choice at `main.rs:521-544`; U2 adds the trusted caller header option beside
  `--fixed-actor`. Each appends its own items.
- `crates/spindle-ingress/src/{router,listener,surface,answer}.rs`, `docs/design/spindle.md` § 11,
  `ess/components.yaml:37-43`: U2.
- `crates/spindle-channel/src/route.rs` and its tests: U3.
- U4 builds on U1's `postgres.rs` after the merge and adds the `state move` subcommand.

## Coordinator's

- `crates/spindle-cli/tests/spindle-cli/main.rs` and the kernel test `main.rs` `mod` lines.
- `CHANGELOG.md` `[Unreleased]`: each unit adds an entry, the merge keeps all.
- `Cargo.lock`, generated output (`ess/generated/`, `crates/spindle-observe-api/`,
  `ui/src/api/generated/`, `website/docs/reference/cli.md`): regenerated after each merge.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run), a `verification`
and, where the adversary found something, a `review_outcome`.

## Commits

The approval commit, the unit commits, the merges into `wave/19`, review fix commits, the closing
store commit, one PR to `main` through the bot merged when CI's `Gate` is green, and release 0.11.0
within 4 h of the merge.
