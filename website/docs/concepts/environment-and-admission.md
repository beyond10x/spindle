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
- A key ending in `.*` provides every capability under that prefix, and an exact key wins over it.
- A document in another format, an unknown field, or a capability with an empty list is refused.

The repository ships four: `examples/environments/browser.toml`,
`examples/environments/laptop.toml`, `examples/environments/laptop-processes.toml` and
`examples/environments/microvm.toml`.

## Admission

Admission takes the capabilities a specification (or one component) requires and the environment,
and answers one of two things:

- **admitted**, with exactly one implementation bound to every required capability: the first one
  the environment lists for it
- **refused**, with every capability the environment has no implementation for

The specification does not choose an implementation. `b10x-spindle admit` prints the result and
exits 0 when admitted, 3 when refused, and `--json` prints it as JSON.

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

Admission binds names, and the parts that run today act on these bindings:

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
channel. A host opened with nothing to answer external causes refuses every `external.*` outcome.
`run` serves `ingress.network` only where it is bound to `http-loopback`, and the Service Worker
host only where it is bound to `service-worker`. Any other binding of it is refused before anything
is served.

## An external cause bound to an HTTP call

An environment may declare HTTP calls, one `[http.<name>]` table each. It binds an external cause
to a call by listing the call as the implementation of the cause's `external.<command>.<outcome>`
key:

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
call. The body is `spindle-external/1` JSON with `format`, `capability`, `command`, `outcome`,
`cause` and the command's `input`.

| Reply | Result |
|---|---|
| `2xx` with `{"holds": true}` or `{"holds": false}` | answers the cause, and the outcome it selects is committed |
| a timeout, a failure, another status, a body over 64 KiB or another body | the command fails as `undecided` and writes nothing; a served request is answered `501` |

A call holds the thread that runs every command until it is answered or times out. A served run
answers nothing else meanwhile.

The document never holds the secret. `secret_env` is the name of an environment variable, read
when `run` starts and sent as `Authorization: Bearer <value>`. An unset variable stops `run` before
the first request.

These are refused when the document is read. The message gives the table's name, never the
value:

- a table that no `external.` key uses
- a `timeout_ms` outside 1 to 30000
- a `url` carrying userinfo (`user:password@`)
- a `secret_env` that is not a variable name

This build calls `http://` URLs only. A served run whose every external cause is bound to a call
needs neither `--decisions` nor `--answer-default`. With `--observe`, a cause a call answers is
listed and counted with the source `call`, and one whose call failed as `unanswered`.

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
| `database_connections` | the database's connection budget available to this deployment: at least components x `database_pool` + 1. Not the serving role's `CONNECTION LIMIT`. Required with `database_ca` |

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

Two numbers bound the connections, and they are different numbers:

- The serving role's `CONNECTION LIMIT`, set on the role in PostgreSQL: from 1 to components x
  `database_pool`. This is eventlog's rule: a serving role may open no more connections than its
  pools admit.
- `database_connections`, a setting: the database's connection budget available to this
  deployment. It must hold every component's pool and one reserved connection, at least
  components x `database_pool` + 1, so it is always above the role's limit.

For one component and the default pool of 4, the role above has `CONNECTION LIMIT 4` and `run`
gets `database_connections` 8:

```console
$ SPINDLE_SETTING_DATABASE_URL=postgres://serving:...@db.internal:5432/app \
  SPINDLE_SETTING_DATABASE_CA=/etc/spindle/db-ca.pem \
  SPINDLE_SETTING_DATABASE_SCHEMA=app \
  SPINDLE_SETTING_DATABASE_POOL=4 \
  SPINDLE_SETTING_DATABASE_CONNECTIONS=8 \
  b10x-spindle run <spec> --environment examples/environments/postgres.toml
```

`run` exits 1 before listening when the database or the settings would not be admitted:
`database_url` unset, not a PostgreSQL URL, naming no host, or holding an `@` other than the one
ending its user info before the host (percent-encode an `@` in the user name, the password or the
query string as `%40`); `database_schema` not a schema name
(lowercase ASCII, digits and underscores, not starting with a digit); a host off loopback without
`database_ca`; `database_connections` below components x `database_pool` + 1 (one connection
reserved); a role that can run DDL (`CREATE` on the schema, an object it owns there, membership of
another role, `SUPERUSER`, `CREATEROLE`, `CREATEDB`, `REPLICATION` or `BYPASSRLS`), such as the
owner role; and a role whose `CONNECTION LIMIT` is not between 1 and components x `database_pool`.
A role limited to 8 connections, the same number as `database_connections`, is refused at the
default pool of 4:

```text
b10x-spindle: opening the PostgreSQL store of probe: store: PostgreSQL refused the serving role for the tables spindle_probe_*: hosted application role requires a finite connection limit within its admitted replica pool share. A serving role holds no DDL right (no CREATE on the schema app, no object it owns there, no membership of another role, none of SUPERUSER, CREATEROLE, CREATEDB, REPLICATION and BYPASSRLS: the owner role's URL is for `store migrate`, never `run`); has a CONNECTION LIMIT from 1 to 4 (components 1 x database_pool 4), which is not database_connections, the database's budget for this deployment, at least 5; holds SELECT, INSERT, UPDATE and DELETE on every spindle_probe_* table, USAGE on the sequence of spindle_probe_events and TEMP on the database; and finds the tables `store migrate` created
```

A server that cannot be reached names its host and port and, with `database_ca`, that a
certificate the bundle did not issue, or one that does not name the host, fails the same way.
`run --state` and `run` reading stdin refuse `postgres`; `run` serves it over HTTP.

A scan of an entity (a list view, a reset, a command decided on a scan) reads that entity's own
stream, where each write names the instances it left, and then one event per instance, up to
1,000 instances and about 32 MiB of rows to one batch of reads: every read a bounded statement
of committed events, so it takes no lock, a transaction left open elsewhere on the server does
not delay it, another process's commits on the same tables (a rolling deploy) do not wait for it,
and it holds the entity's rows in memory, not the log. Only a log `state move` marked, holding
writes of a release before 0.7.0, which record no entity version, is read whole, at one snapshot,
instead. The SQLite store keeps reading a scan from its file's whole log, page by page.

One command's writes land in one transaction, which locks each instance it writes, so they stay
all or nothing. PostgreSQL's lock table is sized by its `max_locks_per_transaction` (64 by
default) times `max_connections` (100): with those defaults a command that writes more than about
7,000 instances at once, such as a set outcome over a large entity, is refused with SQLSTATE
53200 (out of shared memory) and writes nothing. Raise `max_locks_per_transaction` on the server
for such commands (it takes a restart): at 256, one command writing 20,000 instances lands.

### Moving a state directory into PostgreSQL

A `run --state` directory keeps one `<component>.sqlite` file per component. `b10x-spindle state
move --from <dir> <spec> --environment <env>` moves that state into the tables of every component
the environment binds to `postgres`, with the settings `run` reads: on a hosted database the
tables come from `store migrate` first, and `database_url` names the serving role. Each file is
read without creating anything, and its events are appended in their order, the events of one
write in one transaction, so every stream keeps its versions: `run` on PostgreSQL answers every
view as the run on the directory did, and a command decided after the move names the version the
directory's last write left. Event ids and recording times are new. One line per component:

```console
$ b10x-spindle state move --from target/rally-state examples/rally/ess --environment examples/environments/postgres.toml
{"component":"rally-player-east","events":6,"streams":2}
{"component":"rally-player-west","events":6,"streams":2}
{"component":"rally-table","events":12,"streams":2}
```

A directory written by 0.7.0 to 0.10.0 arrives in this release's form, each write naming the
instances it left, so a scan of the moved tables reads the entity's own stream. A directory holding
writes of a release before 0.7.0, which recorded no entity version, arrives with its log marked, so
its scans read the whole log and miss no row.

Stop Spindle first: the move does not keep a running Spindle off the directory or the tables.
Another move is kept off: a move holds a PostgreSQL advisory lock on its schema (any role may take
one) from before it reads the tables to after its last append, and a second move into the same
schema waits up to 5 seconds, then exits 1 naming the other move, writing nothing. A move exits 1,
writing nothing, naming the component, when:

- the directory lacks a component's file;
- a file holds a JSON string or object key with U+0000, which the SQLite store keeps and
  PostgreSQL refuses. Every file is read before the move connects, and the refusal names the
  stream, the event and where in its body the value is;
- a component's tables already hold the stream of an entity the specification declares or the
  directory holds, or the mark of a partial move: each read is of a stream's last committed
  version, so it writes nothing, does not grow with the log and sees every commit while a
  transaction is open elsewhere on the server. On a loopback database, where opening a store
  creates its tables, a component without tables is opened only after every check passed, so a
  refused move creates no table.

A move holds two connections of its role at once: the session holding the schema's lock, which
also reads which tables the schema holds, and one to the tables of the component it reads or
copies, each component's in turn. `run` holds at least one connection per component, so a role
`run` serves two components or more with moves them. A role whose `CONNECTION LIMIT` is 1, which
`run` admits for one component, exits 1 before the move reads the tables, writing nothing and
naming the two connections: move with the role limited to 2 and `database_pool` 2 or more. A connection
PostgreSQL refuses as one too many is tried again for 5 seconds, then the move exits 1 saying so:
stop every `run` on the database first.

A move that fails partway leaves a partial copy, which the next move refuses: drop the component's
tables (or the schema), run `store migrate` and move again.
