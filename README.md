# Spindle

Spindle runs ESS specifications. A specification says what an application means — entities,
commands, guards, events, bindings, views. Spindle supplies everything technical underneath: where
state is stored, how a timer fires, how an event of one component reaches a command of another.

Those technical needs are **capabilities**, and Spindle derives them from the specification. An
author never lists them. An **environment** says which implementations provide which capabilities
where Spindle runs. **Admission** binds each required capability to one implementation, or refuses
and names every capability the environment lacks. The same specification runs on a laptop with an
in-process channel and in a microVM with a TCP one; admission is the only step that differs.

Status: early. Loading, capability derivation and admission work. The executor, channels, timers
and the microVM deployment are planned (see [`docs/design/spindle.md`](docs/design/spindle.md) §12).

## Try it

```console
cargo build --release -p spindle-cli
target/release/b10x-spindle check ess
target/release/b10x-spindle capabilities ess
target/release/b10x-spindle admit ess --environment examples/environments/laptop.toml
target/release/b10x-spindle admit ess --environment examples/environments/microvm.toml
```

The last command is refused: Spindle's own specification has an outcome decided outside the
model, and the `microvm` environment provides nothing for it. Exit status 3 means refused.

## Capabilities

| capability | derived from |
|---|---|
| `store.durable` | an entity in a domain the deployment owns |
| `timer`, `clock` | a periodic binding |
| `channel` | a binding from one component's event to another component's command |
| `ingress.network` | a component `reached_by: network` |
| `ingress.command_line` | a component `reached_by: command_line` |
| `projection` | a view with `consistency: eventual` |
| `external.<command>.<outcome>` | an outcome whose condition is `external:` |

## Environment

```toml
format = "spindle-environment/1"
name = "laptop"
os = "linux"
arch = "x86_64"

[provides]
"store.durable" = ["sqlite", "memory"]
channel = ["in-process"]
"external.*" = ["operator-prompt"]
```

Each list is in order of preference. A key ending in `.*` covers every capability under that
prefix; an exact key wins over it.

## Layout

- `ess/` — Spindle's own specification: environments, deployments, admission.
- `crates/spindle-spec` — loads and compiles a specification with `ess-compiler`.
- `crates/spindle-capability` — derivation, environments, admission.
- `crates/spindle-cli` — the `b10x-spindle` binary.
- `examples/` — environments and the `rally` example system.
- `docs/design/spindle.md` — the design.

Agents start at [`AGENTS.md`](AGENTS.md).

## License

Apache-2.0.
