---
title: Environment and admission
sidebar_position: 3
description: An environment names the implementations available where Spindle runs; admission binds one to every capability or refuses.
lede: The environment, not the specification, chooses the implementations, and admission is the only step that differs between a laptop and a VM.
source: crates/spindle-capability/src/environment.rs, crates/spindle-capability/src/admission.rs, crates/spindle-cli/src/calls.rs, crates/spindle-cli/tests/spindle-cli/tls_cause.rs, crates/spindle-capability/tests/spindle-capability/admission.rs, crates/spindle-cli/tests/spindle-cli/admit.rs, crates/spindle-host/src/host.rs, crates/spindle-kernel/src/postgres.rs, crates/spindle-cli/src/database.rs, crates/spindle-cli/tests/spindle-cli/postgres.rs, crates/spindle-cli/src/seed.rs, crates/spindle-cli/tests/spindle-cli/seed.rs
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

`b10x-spindle admit` prints the result and exits 0 when admitted, 3 when refused, and `--json`
prints it as JSON.

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

| Capability | Implementations the runtime runs today |
|---|---|
| `store.durable` | `memory`; `sqlite`; `postgres`, served by `run` ([below](#state-in-postgresql)); `indexeddb`, one IndexedDB database per component, in the Service Worker host; `memory` only, in the Node export ([below](#the-node-export)) |
| `channel` | `in-process` in the one-process host (`play`, `run`); `tcp` between processes, one component each (`play --component`) |
| `timer` | `tokio`, the name the shipped environments give `spindle-timer`'s schedule |
| `clock` | `system` |
| `identity.mint` | `uuid-v4` |
| `ingress.network` | `http-loopback`: `run` serves the routes and the console routes over HTTP on 127.0.0.1; `service-worker`: the Service Worker host (`spindle-wasm`, the site in `web/`) answers the specification's routes and the console routes in the browser, through the router the loopback listener calls; `node`: the Node export (`spindle-node`) answers them in-process, each request handed to it by the Node program that loaded it ([below](#the-node-export)) |
| `external.*` | `operator-prompt`: `run` answers from the line, `--decisions` or `--answer-default`; an HTTP call the environment declares ([below](#an-external-cause-bound-to-an-http-call)); `decisions-file`: `run` answers from the file the environment's `[decisions]` table names, or `--decisions` ([below](#external-causes-answered-from-a-decisions-file)); `run` refuses any other name before anything runs. `decisions-file`: the Service Worker host answers each external outcome from the decisions file it is opened with, as `run --decisions` answers it; the Node export answers from its decisions file and the answer default, as `run --decisions` and `--answer-default` do |

The SQLite store keeps one `<component>.sqlite` file per component in the directory `run --state`
gives. It keeps each entity's rows
as they rest, read from the file once when it opens and then from each write, so a view read costs
the rows it reads, not the file's history. On Unix a state directory it creates is `0700` and each
database, lock, journal and WAL file it creates is `0600`, whatever the umask. A file that exists
keeps its mode.

A host refuses, as unsupported, a binding it cannot run: the one-process host refuses a `tcp`
channel. A host opened with nothing to answer external causes refuses every `external.*` outcome.
`run` refuses an `ingress.network` binding other than `http-loopback`, the Service Worker host one
other than `service-worker`, and the Node export one other than `node`, before anything is served.

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

`run` sends one request to the call each time it asks that cause, on a stdin line or a served
request. The body is `spindle-external/1` JSON with `format`, `capability`, `command`, `outcome`,
`cause` and the command's `input`.

| Reply | Result |
|---|---|
| `2xx` with `{"holds": true}` or `{"holds": false}` | answers the cause, and the outcome it selects is committed |
| a timeout, a failure, another status, a body over 64 KiB or another body | the command fails as `undecided` and writes nothing; a served request is answered `501` |

A call holds the thread that runs every command until it is answered or times out. Commands and
the reset run one at a time in arrival order, so a served run runs no other command meanwhile.
Reads do not wait: a view read, `GET /api/model`, `GET /api/events` and every `GET /__spindle/`
route are answered beside that thread, from the state the last finished command left.

The document does not hold the secret. `secret_env` is the name of an environment variable, read
when `run` starts and sent as `Authorization: Bearer <value>`. An unset variable stops `run` before
the first request.

A call leaves the host only over verified TLS:

| `url` | How the call is made |
|---|---|
| `https://` | over TLS. The server's certificate is verified against the platform's trust roots, or, where the table has `ca_file`, against the PEM certificates in that file only. A certificate that does not verify fails the call before the request is written, and the failure names the certificate |
| `http://` on loopback | in plain text. Loopback is `localhost`, an address in `127.0.0.0/8` or `[::1]` |

`ca_file` is a path, relative to the directory `run` starts in. It is read when `run` starts: a
file that cannot be read or holds no certificate stops `run` before the first request.

```toml
[http.approvals]
method = "POST"
url = "https://approvals.example/approvals"
timeout_ms = 2000
secret_env = "APPROVALS_TOKEN"
ca_file = "approvals-ca.pem"
```

These are refused when the document is read. The message gives the table's name, not the value:

- a table that no `external.` key and no `[egress]` entry uses
- a `timeout_ms` outside 1 to 30000
- a `url` carrying userinfo (`user:password@`)
- an `http://` `url` whose host is not loopback
- a `ca_file` on an `http://` `url`
- a `secret_env` that is not a variable name

A served run whose every external cause is bound to a call needs neither `--decisions` nor
`--answer-default`. With `--observe`, a cause a call answers is listed and counted with the source
`call`, and one whose call failed as `unanswered`.

## Published events posted to a webhook

The `[egress]` table binds a published event, by its qualified name, to an `[http.<name>]` call
of the same document. A served `run` posts each such event to its call. The call's method must be
`POST`; its `url`, `timeout_ms`, `secret_env` and `ca_file` work as for an external cause.

```toml
[egress]
"shop.order.OrderPlaced" = "orders-hook"

[http.orders-hook]
method = "POST"
url = "https://hooks.example/orders"
timeout_ms = 2000
secret_env = "ORDERS_HOOK_TOKEN"
```

Each request carries one event as `spindle-egress/1` JSON:

```json
{"format": "spindle-egress/1", "event": "shop.order.OrderPlaced", "component": "shop",
 "log": 7311250318264215037, "position": 42, "fields": {"order_id": "o-1"}}
```

`position` is the event's store position, its own place in the component's log; it rises with
each event, the events of one write included. `log` is the id of that log: a reset that creates
the store's SQLite file again starts a new log, whose positions start over, so `component`,
`log` and `position` together name one event. `fields` are the event's fields as published.

| Case | What happens |
|---|---|
| order | one component's events reach a call in the order their commands committed |
| a failure | a timeout, a failure or a status other than `2xx` is sent again after a pause that doubles up to 5 s, without end; the component's later events to that call wait for it. A `2xx` is decided on its status: its body is not read |
| a restart | the position up to which the call answered `2xx` is kept in the component's store, and `run` on the same state posts the events after it |
| at least once | an event answered just before the process is killed may be posted again |
| a failover | with `database_lease`, a process that takes over the leases ([several processes on one store](#several-processes-on-one-store)) posts from the last position the old leader saved, so it may post again the events the old leader posted after its last save, up to its last unsaved page; the old leader's saves are refused once its lease is taken. A repeat carries the same `component`, `log` and `position`, so a consumer de-duplicates on those three |
| a reset | `POST /api/reset` stops the posting while it empties the stores; the events after it are posted, from a SQLite store under a new `log` |

The events are posted beside the thread that runs commands, so a slow webhook holds no request
back. The kept position is no published event: `GET /api/events` does not list it.

These are refused:

- an `[egress]` entry naming a call the document does not declare, or one whose method is not
  `POST`, when the document is read
- an event no component of the specification publishes, before `run` listens (exit 3)
- an event of a component whose state is kept in `memory`, before `run` listens (exit 3)
- an `[egress]` in a `run` that reads stdin

## A seed submitted at start

A served run can submit a list of commands every time it starts, such as the built-in roles of an
access model. The list is a seed file, named by
[`run --seed`](../reference/cli.md#run--seed) or by the environment's `[seed]` table:

```toml
[seed]
file = "seed/roles.json"
```

`file` is a path, relative to the directory `run` starts in. `--seed` wins over the table. A run
that reads stdin, where nothing requires `ingress.network`, stops on either.

The file is one JSON array of entries, submitted in the file's order:

```json
[
  {
    "command": "vault.access.GrantRole",
    "input": {"role_id": "admin", "title": "Administrator"},
    "actor": "vault.access.Operator",
    "caller": {"tenant": "north"},
    "request_id": "seed-admin"
  }
]
```

| Key | Holds |
|---|---|
| `command` | the command's qualified name; a component must accept it |
| `input` | the command's input, every required field of it; `{}` when left out |
| `actor` | the actor the command is sent as; left out, it is sent as no caller |
| `caller` | the caller's attributes, every required one of the actor's; only with `actor` |
| `request_id` | a non-empty id. Within one start, an entry whose id an earlier applied or already-there entry carried is not submitted again |

`run` reads and checks the file before listening, against the specification as the kernel checks
a command before it reads anything. It stops with exit 1, naming the file and the entry (counted
from 1) and its request id, on:

- a file that is not JSON or not an array, or an entry with another key or no `request_id`
- a command the specification does not declare or no component accepts
- an input field the command does not declare, a value its type does not admit, or a required
  field left out
- an actor the specification does not declare, a caller that is not one of its callers, or a caller
  without an actor
- one request id on two entries that differ

Once listening, `run` submits the entries one at a time, each between the requests the listener
hands over, so a request does not wait for the seed to finish. An entry does not come through the
listener: on a run with `--trusted-caller-header` it needs no proxy secret, and with
`--fixed-actor` it is still sent as its own actor and caller. Each entry counts as one of these:

| Count | An entry |
|---|---|
| `applied` | whose outcome took effect |
| `already_there` | whose outcome the specification declares `existing_instance:`, or whose request id an earlier entry carried that was applied or already there |
| `refused` | answered by any other outcome the specification declares with an error. It is logged as `seed <file> entry <n>: <command> (request id <id>) refused: <error>` and the next entry is submitted |
| `rejected` | that did not run: the host or the kernel refused it before an outcome, such as `not_granted` for a caller its command does not grant. It is logged as `seed <file> entry <n>: <command> (request id <id>) rejected: <error>` and the next entry is submitted |

`sent` counts every entry taken from the file. An entry that repeats the request id of a refused
or rejected entry is submitted again and counted by what it becomes. A refusal does not stop the
run. When the last entry has run, `run` prints one line on stderr:

```text
b10x-spindle: seed seed/roles.json ended: 3 sent, 2 applied, 1 already there, 0 refused, 0 rejected
```

A seed that ends with a rejected entry stops the run: the listener has served beside it until
then, and once the end line is printed `run` names the seed file on stderr and exits 1. An entry
the file does not declare correctly never gets that far: it exits 1 before listening.

With `--observe`, `GET /__spindle/metrics` has the same counts as
`spindle_seed_commands_total{counted="sent"}` and so on, from the first scrape. The metric counts a
rejected entry under `refused`.

With or without `--observe`, `GET /__spindle/seed` answers whether the seed is `running` or has
`ended`, with the entries left and the counts of the end line, and `GET /__spindle/ready` answers
`503` until the seed has ended and `200` from then on, so a readiness probe waits for the seed
without counting view rows ([the seed and readiness routes](observe.md#a-served-runs-seed-readiness-and-liveness)).

### A seed on a kept state

On a `--state` directory the run records the seed in `seed.json` beside the state's files, readable
by its owner alone: the digest of the seed file, `sha256:` and its hexadecimal SHA-256, how many of
its entries were sent, and which of those were rejected, each by its place in the file. It is
written after each entry, into a file beside it that then replaces it, so a write cut short leaves
the record as it was. On the next start:

- With the same seed file and every entry done, `run` prints that the seed already ran and sends no
  entry, so an update or a move in the seed is applied once.
- With the same seed file and some entries not done, as after a run stopped mid-seed, it sends the
  entries rejected before again, then the entries after those sent. An entry rejected again stops
  the run with exit 1 once the seed ends, as on the first start. An entry the run was stopped in
  may have been applied without being counted, and is then sent again.
- With another seed file, `run` exits 1 before listening, naming both digests, unless the state
  holds no row; then the new file runs from its first entry.
- On a state written without `seed.json`, the whole seed is sent, as before, and `seed.json` is
  written.

A request id is remembered by the start that sends its entry; a start that resumes the seed also
remembers those of the entries sent before it that were not rejected, so a repeat of one is
already there. `POST /api/reset` on a run without `--seed` removes `seed.json`: the next start with
a seed treats the state as one written without it.

### A seed on PostgreSQL with a lease

A run on PostgreSQL given `database_lease` keeps the same record in the store: the row `seed` of
the lease table of its first component in name order (`<prefix>_p_lease`, which `store migrate`
already creates), holding the digest, the entries done and those rejected, as `seed.json` does.
The process that leads writes it after each entry, on the connection that holds its leases, and a
write lands only while the lease row still names that process's token and database session. Every
process that leads decides from it, whatever lease token it took: a restart of the only process, a
standby that takes over, and a process that leads after one that took the leases and never seeded
(one that exited 3 on a misfit) alike. A start looks for the record in every component's lease
table, so a record kept before a later specification added a component that sorts first is still
found; the next write moves it to the first component's table, in one statement under both leases.
Records found alike in several tables are one record: a rollback to a release with other
components, and a roll forward again, can leave the same record in two tables, and the next write
keeps the one in the first component's table, in one statement under every lease involved. Records
that differ stop the start with exit 1, naming the tables; keep the row `seed` of one of them and
remove the others.

A record kept by a component that a later release removed is found too. Where no current
component's lease table holds a record, the start looks once, with one query of `pg_tables`, for
lease tables of no current component: by their table name (`spindle_…_p_lease`), since a hashed
prefix cannot be turned back into a component name, and read only where the table holds a `lease`
row. A record found there is used as one found in a current table: records alike are one, records
that differ stop the start with exit 1 naming the tables. The next write moves it to the first
component's table in one statement under two leases: the first component's, and the old table's
own, which the process takes for the move and releases after. While another process holds that
table's lease, as a process of the old release still running does, the record is read and not
moved: the next write goes to the first component's table, and from then on the record there wins,
since the old tables are looked at only when no current table holds a record. So one schema holds
one leased deployment: lease tables of another deployment in the same schema would be read as
those of removed components. The rules above hold, with
the store in place of the state:

- With the same seed file and every entry done, `run` prints that the seed already ran on the
  PostgreSQL store, naming the row `seed` and its table, and sends no entry.
- With some entries not done, as after a leader killed mid-seed, the next process to lead sends the
  rest.
- With another seed file, the process exits 1 before it serves a request, naming both digests,
  unless the store holds no row; a standby that takes over exits so too, and a supervisor that
  restarts it meets the same exit until the file or the store changes.
- A store that holds rows and no record, as 0.26.0 or `state move` leaves one, is taken as seeded by
  the file given: one line says so, no entry is sent, and the record is written with every entry
  done. A store with no row and no record gets the whole seed.

A reset writes the record as it writes `seed.json`, and one by a leased run without `--seed`
removes the row, also from the lease tables of removed components, each under its own lease,
which the run takes before it empties any store. While another process holds one of those leases,
as a process of the old release still running may, the reset is refused `409`, naming the table
and that another process holds its lease, and changes nothing: no store is emptied and every
record stays, since a store emptied with the old record left would read as seeded at the next
start. Once that process has ended, or the row `seed` of that table is removed, the reset can be
sent again. Any other store, in PostgreSQL without `database_lease` or in memory, records
nothing: every start sends the whole seed, so a second start applies nothing new only where every
entry is a create whose command declares `existing_instance:`.

### What a seed does not do

- Two `run` processes on one store kept in PostgreSQL without `database_lease` each submit the
  whole file; only `existing_instance:` outcomes keep that from writing twice. Given
  `database_lease`, only the process that leads sends it, deciding from the store's record
  ([a seed on PostgreSQL with a lease](#a-seed-on-postgresql-with-a-lease)). An entry the leader
  was killed in may have been applied without being recorded, and is then sent again.
- `POST /api/reset` drops the seed's entries not yet sent, so a reset that arrives while the seed
  runs ends that run with the counts it reached. With no world it then empties every store,
  records 0 entries done in `seed.json`, and submits the seed again, from its first entry to its
  end, before it answers: the request ids the run remembered are forgotten, and the end line
  printed again counts that run alone. `spindle_seed_commands_total` is a counter and goes on
  across resets. A reset whose body gives a world loads that world alone, does not submit the seed
  and records every entry done, so a restart sends none; put the seeded rows into the world to
  keep them. SIGINT or SIGTERM during a reset's seed stops the seed between two entries: the reset
  answers `200` once the entry under way is done, `seed.json` counts the entries done, and the run
  exits once the requests already waiting are answered, for up to `--drain` seconds, each still
  waiting then answered `503` ([draining on SIGTERM](observe.md#draining-on-sigterm)).
- Only a served `run` submits a seed. `play` and `deploy` stop with exit 1 on an environment
  document with a `[seed]` table, as `run` does where nothing requires `ingress.network`.

## External causes answered from a decisions file

An environment can bind external causes to `decisions-file` and name the file in a `[decisions]`
table:

```toml
[provides]
"external.*" = ["decisions-file"]

[decisions]
file = "decisions/answers.toml"
```

`file` is a path, relative to the directory `run` starts in, to a decisions file in the shape
[`run --decisions`](../reference/cli.md#run) reads. `run`, on stdin or serving HTTP,
answers each cause bound to `decisions-file` from it as it answers from `--decisions`: a request
line's own `external` object first, then the file, then `--answer-default`. A cause bound to an
`[http.<name>]` call is still answered by its call first. The file answers no other cause: one
bound to `operator-prompt` is answered from the line, then `--answer-default`, even where the file
answers it (served, a request nothing answers is refused `501`). `--decisions` wins over the
table: the environment's file is then not read, and `--decisions` answers every cause.

`run` stops with exit 1 before anything runs, naming the capability:

- on a cause bound to `decisions-file` with neither `--decisions` nor a `[decisions]` table
- on a cause bound to an implementation `run` does not provide, naming it too: `run` provides
  `operator-prompt` (the request line, `--decisions` and `--answer-default` answer the cause),
  `decisions-file` and each `[http.<name>]` call the environment declares

An environment document with a `[decisions]` table is refused when it is read unless an
`external.` key lists `decisions-file` first, the implementation admission binds. `play`,
`play --component` and `deploy` stop with exit 1 on a document with a `[decisions]` table, naming
the table and its file: they answer no external cause from a file.

## The Node export

`spindle-node` serves a specification inside a Node program, as an in-process fake backend: no
process, port, file or thread of its own. It is the host compiled to WebAssembly and bound by
`wasm-bindgen --target web`, an ES module whose glue imports nothing; Node 22 loads it from bytes
the program reads.

```console
cargo build --locked --release --target wasm32-unknown-unknown -p spindle-node
wasm-bindgen --target web --out-dir target/spindle-node target/wasm32-unknown-unknown/release/spindle_node.wasm
```

```js
import { readFileSync } from "node:fs";
import { initSync, Spindle } from "./target/spindle-node/spindle_node.js";

initSync({ module: readFileSync("./target/spindle-node/spindle_node_bg.wasm") });
const spindle = Spindle.create({
  sources: [["system.yaml", systemYaml], ["domains/review.yaml", reviewYaml]],
  environment: readFileSync("examples/environments/node.toml", "utf8"),
  decisions: decisionsToml,
});
const answer = await spindle.handle({
  method: "POST",
  path: "/review/commands/Submit",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ submission_id: "s1", amount: 12 }),
});
// answer.status, answer.headers ([name, value] pairs), answer.body (Uint8Array)
spindle.dispose();
```

- `Spindle.create(config)` takes a JSON string or a plain object: `sources` (each
  `[name, text]`), `environment` (TOML), and optionally `decisions` (TOML), `answerDefault`
  (`true` answers what the decisions file does not by each command's default branch, as
  `run --answer-default`), `binding` (an `ess-http/1` binding, YAML or JSON) and `fixedActors`.
  Any other key is refused. It throws an `Error` naming the reason for what `run` refuses before
  listening, and for a component whose state is bound to anything but `memory` or whose
  `ingress.network` is bound to anything but `node` (`examples/environments/node.toml`).
- `fixedActors` is a list of `{ actor, caller }`: an actor's qualified name and its caller
  attributes (`{}` where left out), as `run --fixed-actor` with `--fixed-caller` takes them, in
  order. Each served command is sent as the first fixed actor whose `may:` names it, and a
  read-granted view read as the first that may read it; one no fixed actor may run answers `403`
  "not granted", naming the first. A test-backend switch, not authentication. `create` refuses an
  actor given twice, an actor the specification does not declare, and a caller its binding
  refuses, with `run`'s reason:
  `the fixed actor bureau.desk.Nobody: the specification declares no such actor`.
- `spindle.notServed()` lists what `run` prints on stderr before it listens: each operation not
  served and each outcome answered `501` where a request selects it, with why.
- `spindle.handle({ method, path, query, headers, body })` answers through the router the loopback
  listener calls, the console routes included. `path` may carry its query; only `content-type` is
  read from `headers`; `body` is a string or a `Uint8Array`. Each call is answered before it
  returns, so requests are answered one at a time in the order they arrive.
- `spindle.reset(world)` answers as `POST /api/reset` with `world` as its body: every store
  emptied, then the world's rows, decision table and mint seed loaded, as `run` loads them.
- `spindle.dispose()` drops the host; every later call throws. Instances share nothing.

The clock is the system clock, and periodic bindings do not fire; a clock under the program's
control, seeds and event delivery are planned (`epic:node-export`).

## State in PostgreSQL

An environment that binds `store.durable` to `postgres` (`examples/environments/postgres.toml`)
keeps each component's state in an operator's PostgreSQL, 13 or later, through `eventlog-postgres`.
Rows and events are encoded as the SQLite store encodes them. Each component has its own tables,
`spindle_<component>_*` (each `-` as `_`), so the components of one deployment share a database
without reading each other's rows.

The environment document holds only the implementation. The database comes from settings, read as
`SPINDLE_SETTING_<NAME>`:

| Setting | Meaning |
|---|---|
| `database_url` | `postgres://user:password@host:port/database`; required, never printed |
| `database_ca` | a PEM CA bundle; the server is reached over TLS (`sslmode=require`) and verified against it. Required for a host that is not loopback or a Unix socket |
| `database_schema` | the schema holding the tables; `public` by default |
| `database_pool` | connections per component; 4 by default. Reads run at most `database_pool` - 1 at a time (1 with a pool of 1), so one connection is always left for writes; a read over that waits its turn |
| `database_connections` | the database's connection budget available to this deployment: at least components x `database_pool` + 1. Not the serving role's `CONNECTION LIMIT`. Required with `database_ca` |
| `database_lease` | how long, in whole seconds, a process holds the lease of each component's tables from its last renewal, a whole number from 1 to 86400 (a day); not given by default. Only with it does a run take the leases, lead or stand by ([several processes on one store](#several-processes-on-one-store)): two processes on one store without it both write, so replicas must set it. The lease takes one connection of the serving role beyond components x `database_pool`. A process standing by asks for the leases every third of it |

Without `database_ca`, a loopback or Unix-socket database is opened isolated, over plaintext, and
its tables are created when `run` opens it: for tests and a local database. With `database_ca` the
database is hosted, and two roles take part:

- The role that owns the schema creates the tables with `b10x-spindle store migrate <spec>
  --environment <env>`, the only command that runs DDL. A second migrate leaves the tables as they
  are. Tables an earlier release created are brought to this release's shape, and `run` refuses
  them until then: after an upgrade, and after restoring the database from a backup, run `store
  migrate` before `run`. One line per component:

  ```console
  $ b10x-spindle store migrate crates/spindle-cli/tests/fixtures/served-review --environment examples/environments/postgres.toml
  {"migrated":"probe","schema":"downstream_1794331581","tables":"spindle_probe_*"}
  ```

- `run` opens with a serving role that can run no DDL and holds `SELECT`, `INSERT`, `UPDATE` and
  `DELETE` on every `spindle_<component>_*` table, `USAGE` on their sequences and `TEMP` on the
  database. On the three tables only `store migrate` writes, `spindle_<component>_schema_version`,
  `_projection_registry` and `_settlement_floor`, it holds `SELECT` and no `INSERT`, `UPDATE`,
  `DELETE` or `TRUNCATE`. Run the grants after `store migrate`, with one `REVOKE` per component:

  ```sql
  CREATE ROLE serving LOGIN PASSWORD '...' CONNECTION LIMIT 4;
  GRANT USAGE ON SCHEMA app TO serving;
  GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA app TO serving;
  REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON app.spindle_probe_schema_version,
    app.spindle_probe_projection_registry, app.spindle_probe_settlement_floor FROM serving;
  GRANT USAGE ON ALL SEQUENCES IN SCHEMA app TO serving;
  ```

The serving role's `CONNECTION LIMIT` is from 1 to components x `database_pool`, by eventlog's
rule: a serving role may open no more connections than its pools admit. `database_connections`
holds every component's pool and one reserved connection, so it is above the role's limit.

With `database_lease` each process also holds one connection of the serving role for its leases,
beyond components x `database_pool`, so a role limit must leave room for it: the pools and the
lease connection together need components x `database_pool` + 1 of the role's connections, and
each process standing by needs one more. A role limited to exactly components x `database_pool`,
the most eventlog admits, leaves the pools one connection short once they are all in use; at
`database_pool` 1 the run cannot open its last store. Without `database_lease` a run opens no lease
connection.

Reads served beside the commands take no connection of their own: each takes one from the
component's pool for a statement and gives it back, so a pool of 4 is shared by the writer and
every read.

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

- `database_url` unset, not a PostgreSQL URL, with no host, or with an `@` other than the one
  ending its user info
- `database_schema` not a schema name: lowercase ASCII, digits and underscores, not starting with
  a digit
- a host off loopback without `database_ca`
- `database_connections` below components x `database_pool` + 1
- `database_lease` that is not a whole number of seconds from 1 to 86400 (a day)

It exits 1 the same way for a serving role that would not be admitted:

- a role that can run DDL, such as the owner role: `CREATE` on the schema, an object it owns
  there, membership of another role, `SUPERUSER`, `CREATEROLE`, `CREATEDB`, `REPLICATION` or
  `BYPASSRLS`
- a role whose `CONNECTION LIMIT` is not between 1 and components x `database_pool`
- a role that can write `spindle_<component>_schema_version`, `_projection_registry` or
  `_settlement_floor`
- tables an earlier release's `store migrate` created, until `store migrate` runs again; with
  `database_lease` that includes tables without the lease table `spindle_<component>_p_lease`,
  which a release before leases did not create

Percent-encode an `@` in the user name, the password or the query string as `%40`. A role limited
to 8 connections, the same number as `database_connections`, is refused at the default pool of 4:

```text
b10x-spindle: opening the PostgreSQL store of probe: store: PostgreSQL refused the serving role for the tables spindle_probe_*: hosted application role requires a finite connection limit within its admitted replica pool share. A serving role holds no DDL right (no CREATE on the schema app, no object it owns there, no membership of another role, none of SUPERUSER, CREATEROLE, CREATEDB, REPLICATION and BYPASSRLS: the owner role's URL is for `store migrate`, never `run`); has a CONNECTION LIMIT from 1 to 4 (components 1 x database_pool 4), which is not database_connections, the database's budget for this deployment, at least 5; holds SELECT, INSERT, UPDATE and DELETE on every spindle_probe_* table but spindle_probe_schema_version, spindle_probe_projection_registry and spindle_probe_settlement_floor, on which it holds SELECT and no INSERT, UPDATE, DELETE or TRUNCATE, USAGE on the sequence of spindle_probe_events and TEMP on the database; and finds the tables of this release's `store migrate`
```

A server that cannot be reached fails the same way, and the message gives its host and port. With
`database_ca`, so does a certificate the bundle did not issue, or one that does not name the host.
`run --state` and `run` reading stdin refuse `postgres`. `run` serves it over HTTP.

A scan of an entity, such as a list view, a reset or a command decided on a scan, reads that
entity's own stream, where each write lists the instances it left. It then reads one event per
instance, up to 1,000 instances and about 32 MiB of rows to one batch of reads. The scan holds
only the entity's rows in memory. A log that `state move` marked is read whole, at one snapshot.

The scan's reads are bounded statements of committed events, so they take no lock. A transaction left
open elsewhere on the server does not delay it, and another process's commits on the same tables
(a rolling deploy) do not wait for it.

One command's writes land in one transaction, which locks each instance it writes, so they stay
all or nothing. PostgreSQL's lock table is sized by its `max_locks_per_transaction` (64 by
default) times `max_connections` (100). With those defaults a command that writes more than about
7,000 instances at once, such as a set outcome over a large entity, is refused with SQLSTATE 53200
(out of shared memory) and writes nothing.

Raise `max_locks_per_transaction` on the server for such commands, which takes a restart. At 256,
one command writing 20,000 instances lands.

### Several processes on one store

Two or more `run` processes of one specification may start on one PostgreSQL store, as replicas of
a service do, when each is given `database_lease`. One leads and is the store's one writer; the
others stand by. Leases, fencing and standing by apply only with `database_lease`: two processes
on one store without it both write, as before leases, so replicas must set it.

Each component's tables hold a lease, one row of `spindle_<component>_p_lease`, which `store
migrate` creates beside the other tables (a database opened isolated creates it too). The row
names a token, the database session of the process that holds it, and when it ends, by the
database's clock. Each process takes the lease of every component in name order through one
connection of its own, one of the serving role's beyond its pools
([the role's limit](#state-in-postgresql)), and leads once it holds them all. The leader renews
them every fifth of `database_lease` on that same session, and retries a renewal that did not land
at once; a clean exit, on SIGINT or SIGTERM, releases them once every egress cursor is saved, each
release waiting at most 1 s whatever `database_lease` is, so a standby takes over at its next poll.
A renewal lands only on the session
that took the lease: a leader whose lease connection was closed (a terminated backend, a dropped
connection, an idle-session timeout) has given its leases up, and exits 1 at its next renewal.

A process that finds a lease held stands by. It binds its listener and prints its listening line,
answers `GET /__spindle/live` `200`, `GET /__spindle/ready` `503` with `standby: true`, and every
other request `503`. It opens no store, writes nothing, posts no event to a webhook and keeps no
dead letter. Every third of `database_lease` it asks for the leases again; an ask that waits out
its third of the lease, as behind a leader's write that holds the lease row for longer, reads the
lease as held, and the process stands by on. So does an ask whose connection was closed or
refused: the process connects again for its next ask, and prints a warning naming the leases and
the connection error, once each time the error changes, and a line once it connects again; its
readiness reads as any standby's. A lease is free once its end has passed, or once the database
session that holds it has ended, as a killed process's does. On taking them all the
process opens the stores, builds its host and serves, and decides the seed from the store's record
([a seed on PostgreSQL with a lease](#a-seed-on-postgresql-with-a-lease)). Its stderr shows the
takeover:

```text
b10x-spindle: standing by: another process holds the lease of board, so this run serves nothing until it takes the leases of board, ledger
b10x-spindle: took the lease of board
b10x-spindle: took the lease of ledger
```

Taking a lease moves its token on. Every write of a process runs, inside its own transaction, a
check that the lease row still names the token that process took. So a leader that was paused past
its lease, while another process took over, lands nothing once it resumes: the write is refused,
naming the lease, and the rows stay as the new leader left them. A leader paused inside a write holds its
lease row, which the write locked, until the store's idle timeout of 10 s ends its transaction:
the takeover comes at most one `database_lease` plus 10 s after the pause, and one ask of the
standby. Until then the locks it holds may also hold up the new leader's writes, as where it was
paused before it locked the row. A store's operations keep their own 10 s deadline whatever the
lease. Before it runs a request, a
leader whose last renewal began two thirds of the lease ago or longer, as after a pause, waits
until it has renewed its leases or found one taken. A write the check refuses counts as a lease
found taken, as when the session holding the lease ended and another process took it before the
leader's next renewal; so does a write that failed committing nothing where the leader then finds
a lease taken. A leader that finds its lease taken answers the request it took and those
already queued `503`, prints a line naming the lease on stderr, as below, and exits 1, for its
supervisor to start it again as a standby. While its lease is not known still held, its egress
posts nothing. The process that takes over posts from the last position the old leader saved, so
an event may be posted twice across a failover, each time with the same `component`, `log` and
`position` ([posted to a webhook](#published-events-posted-to-a-webhook)). The line it prints:

```text
b10x-spindle: the lease of board was taken by another process, or its session ended, so this one writes nothing more and exits
```

A leader whose renewals all fail for a whole `database_lease`, as when the database is unreachable,
exits 1 the same way, since another process may hold the lease by then. A session outside Spindle
that locks a lease row stops the takeover while it holds it: a standby's ask waits out its lock and
reads the lease as held, and the leader, whose renewals wait out the same lock, exits 1 once one
`database_lease` passed without a renewal.

The lease covers a store kept in PostgreSQL only. A `--state` directory stays locked to one
process, and a connection pooler that hands one database session to several clients in turn is not
supported for the lease's connection.

### A specification change over stored rows

Each durable store records the specification it was last written under: the model's digest (the
`spec_digest` of a conformance suite's provenance) and, for each entity the component owns, every
field's type and every lifecycle state. A PostgreSQL store, the SQLite file of each component of a
served run and the one SQLite file of `run` reading stdin all keep it, as one event of the stream
`spindle:log`/`specification`, so it needs no table of its own and `state move` carries it. It is
no published event: `GET /api/events` never answers it.

`run` opens a store under a specification with another digest, such as a new version of the
consumer's service, only where every stored row stays readable. It compares the recorded shapes
with the new ones, for the entities that hold rows only. A change fits when every stored value
reads under the new shape: types are compared by structure first, and where the structure alone
would refuse, each stored value is read under the new type as `run` admits a command's value. A
row does not fit when:

- its entity is removed
- its entity's domain moved to another component, whose store does not hold the row; the line
  names that component
- a field it holds a value of is removed (a field held only as `null` is not)
- a field it holds a value of changed type so that the stored value no longer reads: an enum value
  removed that a row holds, a struct member made required that a value lacks, a newtype prefix or
  alphabet a value breaks, or a newtype or struct invariant a value breaks. Every row holding the
  field is counted
- a non-optional field is added, or a field is made required, that it lacks
- the lifecycle state it rests in is removed

Otherwise `run` records the new specification and starts. Examples are an optional field added, a
field made optional, or made required where every row holds it, an enum value added, a struct
member added as optional, a newtype prefix, alphabet or invariant every stored value meets, a new
entity or state, and any change to an entity that holds no row. A store with the same digest
starts without a check. An invariant added to the entity itself is not checked against the rows.

Where a row does not fit, `run` exits 3 before listening, or before reading a stdin line, and
prints on stdout the store and each entity and field (or state) that does not fit, with how many
rows hold it. Nothing is written, so the rows and the record stay as they were, and every store of
the run is checked before any records. No migration is offered: serve the earlier specification,
or move the rows by hand.

```text
refused: the store of archive holds rows this specification cannot read
  archive.shelf.Card.code: the field is removed; stored rows holding it: 1
```

A store an earlier release wrote holds rows and no record. `run` records the specification it
serves without a check and logs one line saying so; from then on each start is checked.

### Moving a state directory into PostgreSQL

`b10x-spindle state move --from <dir> <spec> --environment <env>` moves a `run --state` directory
into the tables of every component the environment binds to `postgres`, with the settings `run`
reads. On a hosted database the
tables come from `store migrate` first, and `database_url` connects as the serving role.

The move reads each file without creating anything. Its events are appended in their order, the events of
one write in one transaction, so every stream keeps its versions. `run` on PostgreSQL answers every
view as the run on the directory did, and a command decided after the move continues from the
version the directory's last write left. Event ids and recording times are new. One line per
component:

```console
$ b10x-spindle state move --from target/rally-state examples/rally/ess --environment examples/environments/postgres.toml
{"component":"rally-player-east","events":6,"streams":2}
{"component":"rally-player-west","events":6,"streams":2}
{"component":"rally-table","events":12,"streams":2}
```

A directory written by 0.7.0 to 0.10.0 arrives in this release's form, so a scan of the moved
tables reads the entity's own stream. A directory holding writes of a release before 0.7.0, which
recorded no entity version, arrives with its log marked, so its scans read the whole log and miss
no row.

The move takes the directory's lock, `spindle.lock`, the one `run --state` holds, before it reads
a file, and holds it to its end. While a run holds it, the move exits 1 before it connects, and the
message gives the directory and the run's process id. Stop a Spindle running on PostgreSQL first:
the move does not keep it off the tables.

A move holds a PostgreSQL advisory lock on its schema, which any role may take, from before it
reads the tables to after its last append. A second move into the same schema waits up to 5
seconds, then exits 1 and writes nothing, and its message gives the other move.

A move exits 1 and writes nothing when one of these holds, and the message gives the component:

- the directory lacks a component's file
- a file holds a JSON string or object key with U+0000, which the SQLite store keeps and
  PostgreSQL refuses. Every file is read before the move connects, and the message also gives the
  stream, the event and where in its body the value is.
- a component's tables already hold the stream of an entity the specification declares or the
  directory holds, or the mark of a partial move

The check of the tables reads each stream's last committed version, so it writes nothing, does not
grow with the log and sees every commit while a transaction is open elsewhere on the server. On a
loopback database, where opening a store creates its tables, a component without tables is opened
only after every check passed, so a refused move creates no table.

A move holds two connections of its role at once: the session holding the schema's lock, which
also reads which tables the schema holds, and one to the tables of the component it reads or
copies, each component's in turn. `run` holds at least one connection per component, so a role
that `run` serves two components or more with moves them too.

With a role whose `CONNECTION LIMIT` is 1, which `run` admits for one component, the move exits 1
before it reads the tables and writes nothing. The message gives the two connections. Move with the
role limited to 2 and `database_pool` 2 or more. A connection PostgreSQL refuses as one too many is
tried again for 5 seconds, then the move exits 1 saying so.

A move that fails partway leaves a partial copy, which the next move refuses: drop the component's
tables (or the schema), run `store migrate` and move again.
