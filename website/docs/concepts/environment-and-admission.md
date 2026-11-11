---
title: Environment and admission
sidebar_position: 3
description: An environment names the implementations available where Spindle runs; admission binds one to every capability or refuses.
lede: The environment, not the specification, chooses the implementations, and admission is the only step that differs between a laptop and a VM.
source: crates/spindle-capability/src/environment.rs, crates/spindle-capability/src/admission.rs, crates/spindle-capability/tests/spindle-capability/admission.rs, crates/spindle-cli/tests/spindle-cli/admit.rs, crates/spindle-host/src/host.rs, crates/spindle-kernel/src/postgres.rs, crates/spindle-cli/src/database.rs, crates/spindle-cli/tests/spindle-cli/postgres.rs
---

## The environment

An environment document (`spindle-environment/1`, TOML) says where Spindle runs and which
implementations provide which capabilities there:

```toml
format = "spindle-environment/1"
name = "microvm"
os = "linux"
arch = "x86_64"

[provides]
"store.durable" = ["sqlite"]
"store.ephemeral" = ["memory"]
"identity.mint" = ["uuid-v4"]
clock = ["system"]
timer = ["tokio"]
channel = ["tcp"]
projection = ["in-process"]
"ingress.network" = ["http"]
```

- Each list is in order of preference.
- A key ending in `.*` provides every capability under that prefix; an exact key wins over it.
- A document in another format, an unknown field, or a capability with an empty list is refused.

The repository ships two: `examples/environments/laptop.toml` and
`examples/environments/microvm.toml`.

## Admission

Admission takes the capabilities a specification (or one component) requires and the environment,
and answers one of two things:

- **admitted**, with exactly one implementation bound to every required capability: the first one
  the environment lists for it;
- **refused**, naming every capability the environment has no implementation for.

The specification never chooses an implementation. `b10x-spindle admit` prints the result and
exits 0 when admitted, 3 when refused; `--json` prints it as JSON.

```console
$ b10x-spindle admit examples/rally/ess --environment examples/environments/microvm.toml
admitted on microvm
  channel = tcp
  clock = system
  ingress.network = http
  store.durable = sqlite
  timer = tokio
```

## What the runtime does with a binding

Admission binds names. The parts that run today act on these bindings:

| Capability | Implementations the runtime runs today |
|---|---|
| `store.durable` | `memory`, `sqlite` (one SQLite file per component; `run --state` names it), `postgres` (one set of tables per component in an operator's PostgreSQL, served by `run`; below), `indexeddb` (one IndexedDB database per component, in the Service Worker host) |
| `channel` | `in-process` in the one-process host (`play`, `run`); `tcp` between processes, one component each (`play --component`) |
| `timer` | `tokio`, the name the shipped environments give `spindle-timer`'s schedule |
| `clock` | `system` |
| `identity.mint` | `uuid-v4` |
| `ingress.network` | `http-loopback`: `run` serves the routes and the console routes over HTTP on 127.0.0.1; `service-worker`: the Service Worker host (`spindle-wasm`, the site in `web/`) answers the specification's routes and the console routes in the browser, through the router the loopback listener calls |
| `external.*` | answered by `run` from the line, `--decisions` or `--answer-default`, whatever the binding names, unless it names an HTTP call the environment declares (below); `decisions-file` in the Service Worker host: each external outcome answered from the decisions file it is opened with, as `run --decisions` answers it |

A host refuses, as unsupported, a binding it cannot run: the one-process host refuses a `tcp`
channel, and a host opened with nothing to answer external causes refuses every `external.*`
outcome. `run` serves `ingress.network` only where it is bound to `http-loopback`, and the Service
Worker host only where it is bound to `service-worker`; any other binding of it is refused before
anything is served.

## An external cause bound to an HTTP call

An environment may declare HTTP calls, each an `[http.<name>]` table, and bind an external cause
to one by naming it as the implementation of the cause's `external.<command>.<outcome>` key:

```toml
[provides]
"external.shop.order.Place.approved" = ["approvals"]
"external.*" = ["operator-prompt"]

[http.approvals]
method = "POST"
url = "http://127.0.0.1:8080/approvals"
timeout_ms = 2000
secret_env = "APPROVALS_TOKEN"
```

Each time `run` asks that cause, on a stdin line or a served request, it sends one request to the
call: a `spindle-external/1` JSON body with `format`, `capability`, `command`, `outcome`, `cause`
and the command's `input`. A `2xx` reply `{"holds": true}` or `{"holds": false}` answers the
cause, and the outcome it selects is committed. A timeout, a failure, another status, a reply
body over 64 KiB or another body fails the command as `undecided` and writes nothing; served, it
answers `501`. A call holds the thread that runs every command until it is answered or times out,
so a served run answers nothing else meanwhile.

The document never holds the secret: `secret_env` names an environment variable, read when `run`
starts and sent as `Authorization: Bearer <value>`. An unset variable stops `run` before the first
request. A table no `external.` key names, a `timeout_ms` outside 1 to 30000, a `url` carrying
userinfo (`user:password@`), or a `secret_env` that is not a variable name is refused when the
document is read, naming the table and never repeating the value. This build calls `http://` URLs
only; a served run whose every external cause is bound to a call needs neither `--decisions` nor
`--answer-default`. With `--observe`, a cause a call answers is listed and counted with the source
`call`, and one whose call failed as `unanswered`.

## State in PostgreSQL

An environment that binds `store.durable` to `postgres` (`examples/environments/postgres.toml`)
keeps each component's state in an operator's PostgreSQL, 13 or later, through `eventlog-postgres`.
Rows and events are encoded as the SQLite store encodes them. Each component has its own tables,
`spindle_<component>_*` (each `-` as `_`), so the components of one deployment share a database
without reading each other's rows. The document names only the implementation; the database comes
from settings, read as `SPINDLE_SETTING_<NAME>`:

| Setting | Meaning |
|---|---|
| `database_url` | `postgres://user:password@host:port/database`; required, never printed |
| `database_ca` | a PEM CA bundle; the server is reached over TLS (`sslmode=require`) and verified against it. Required for a host that is not loopback or a Unix socket |
| `database_schema` | the schema holding the tables; `public` by default |
| `database_pool` | connections per component; 4 by default |
| `database_connections` | the connection budget of the serving role; required with `database_ca` |

Without `database_ca`, a loopback or Unix-socket database is opened isolated, over plaintext, and
its tables are created when `run` opens it: for tests and a local database. With `database_ca` the
database is hosted, and two roles take part:

- The role that owns the schema creates the tables with `b10x-spindle store migrate <spec>
  --environment <env>`, the only command that runs DDL. A second migrate leaves the tables as they
  are. One line per component:

  ```console
  $ b10x-spindle store migrate crates/spindle-cli/tests/fixtures/served-review --environment examples/environments/postgres.toml
  {"migrated":"probe","schema":"downstream_1794331581","tables":"spindle_probe_*"}
  ```

- `run` opens with a serving role that can run no DDL and holds `SELECT`, `INSERT`, `UPDATE` and
  `DELETE` on every `spindle_<component>_*` table, `USAGE` on their sequences and `TEMP` on the
  database:

  ```sql
  CREATE ROLE serving LOGIN PASSWORD '...' CONNECTION LIMIT 4;
  GRANT USAGE ON SCHEMA app TO serving;
  GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA app TO serving;
  GRANT USAGE ON ALL SEQUENCES IN SCHEMA app TO serving;
  ```

`run` exits 1 before listening when the database or the settings would not be admitted:
`database_url` unset; a host off loopback without `database_ca`; `database_connections` below
components x `database_pool` + 1 (one connection reserved); a role that can run DDL (`CREATE` on
the schema, an object it owns there, membership of another role, `SUPERUSER`, `CREATEROLE`,
`CREATEDB`, `REPLICATION` or `BYPASSRLS`), such as the owner role; and a role whose `CONNECTION
LIMIT` is not between 1 and components x `database_pool`. The last is eventlog's rule: a serving
role may open no more connections than its pools admit. With one component and the default pool of
4, a role limited to 8 connections is refused:

```text
b10x-spindle: opening the PostgreSQL store of probe: store: PostgreSQL refused the serving role for the tables spindle_probe_*: hosted application role requires a finite connection limit within its admitted replica pool share. A serving role holds no DDL right (no CREATE on the schema downstream_1794331581, no object it owns there, no membership of another role, none of SUPERUSER, CREATEROLE, CREATEDB, REPLICATION and BYPASSRLS: the owner role's URL is for `store migrate`, never `run`); has a CONNECTION LIMIT from 1 to 4 (components 1 x database_pool 4); holds SELECT, INSERT, UPDATE and DELETE on every spindle_probe_* table, USAGE on the sequence of spindle_probe_events and TEMP on the database; and finds the tables `store migrate` created
```

The same role limited to 4, with `database_connections` 8, is admitted and served. `run --state`
and `run` reading stdin refuse `postgres`; `run` serves it over HTTP.

PostgreSQL shows a committed row to a reader of the log only once every older write transaction
on the server has ended. A scan of an entity (a list view, a reset, a command decided on a scan)
waits for that, up to 10 s, and fails when an older write transaction stays open longer.
