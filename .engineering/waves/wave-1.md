# Wave 1 of [plan 1](plan-1.md)

Coordinator: the spindle session. Skill `aep:implementing` 0.26.0. Integration branch `wave/1`,
managed tree `spindle-wave1`. Operator approval: `approval-record:wave-1-acceptance` ("go ahead …
you can use up to 3 impl agents"), which waives the stage-1 stop.

Commits this wave makes: one per unit (3), their merges into `wave/1`, the closing store commit,
and the merge of `wave/1` into `main`, pushed through `b10x-gates bot`. Nothing else.

Pre-flight 2026-11-04: `main` clean at `fb90b36`; 123G free on `/`; no earlier wave trees; one
measured workspace build: `cargo test --workspace` about 9 min cold in CI.

## Units

Paths are under `~/.local/state/worktree/trees/b10x/spindle/`; scratch under the session's scratch
directory, `wave1/<unit>`. Each tree builds into its own `target/`.

| unit | story | tree | base | agent | stage |
|---|---|---|---|---|---|
| kernel | `story:execute-commands` | `spindle-kernel` | `fb90b36` (detached) | `aep:implementor` | implementing (dispatched before the wave was formed; same brief shape) |
| timer | `story:periodic-bindings-fire` | `spindle-timer` | `wave/1` at `fb90b36` | `aep:implementor` | implementing |
| channel | `story:bindings-cross-components` | `spindle-channel` | `wave/1` at `fb90b36` | `aep:implementor` | implementing |

Adversary per green unit: `aep:adversary`.

## Shared files

Units do not edit the root `Cargo.toml`; each declares its dependencies with versions in its own
crate. The coordinator resolves `Cargo.lock` at merge by regenerating it.

## Gate

`task check` once on `wave/1` after the three merges.
