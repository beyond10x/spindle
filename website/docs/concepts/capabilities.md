---
title: Capabilities
sidebar_position: 2
description: The technical needs of a specification, derived from what it declares and never written down by its author.
lede: Capabilities are derived from the specification and are not authored by hand.
source: crates/spindle-capability/src/derive.rs, crates/spindle-capability/tests/spindle-capability/admission.rs, crates/spindle-capability/tests/spindle-capability/mint.rs, crates/spindle-cli/src/cli.rs, crates/spindle-cli/tests/spindle-cli/decisions.rs, crates/spindle-cli/tests/spindle-cli/served_decisions.rs, README.md
---

A capability is something technical a specification needs from the runtime: a durable store, a
timer, a channel between components. Spindle reads the compiled specification and lists them, so
the list cannot claim less than the specification needs.

## The derivation rules

| Capability | Derived from |
|---|---|
| `store.durable` | an entity in a domain the deployment owns |
| `timer`, `clock` | a periodic binding |
| `channel` | a binding from one component's event to another component's command |
| `ingress.network` | a component with `reached_by: network` |
| `ingress.command_line` | a component with `reached_by: command_line` |
| `projection` | a view with `consistency: eventual` |
| `external.<command>.<outcome>` | an outcome whose condition is `external:` |
| `identity.mint` | a `creates:` outcome whose identity is neither an input nor a payload source |

Each capability carries its reasons: the kind of declaration (`entity`, `binding`, `component`,
`view`, `outcome`), its name, and what about it needs the capability. `b10x-spindle capabilities`
prints them, `--json` as JSON.

## Per component

Without `--component`, the whole system is one deployment, and only a binding between two
components needs a `channel`. With `--component <name>`, the list is what that component needs
when it runs on its own: the entities of the domains it owns, the commands it accepts, and the
bindings that start or end at it.

```console
$ b10x-spindle capabilities examples/rally/ess --component rally-table
channel
  binding east-moves: rally.east.PaddleMoveRequested crosses to rally.table.MovePaddle in another component
  binding east-watches: rally.table.BallMoved crosses to rally.east.ObserveBall in another component
  binding west-moves: rally.west.PaddleMoveRequested crosses to rally.table.MovePaddle in another component
  binding west-watches: rally.table.BallMoved crosses to rally.west.ObserveBall in another component
clock
  binding tick: a periodic cause is placed on the clock
ingress.network
  component rally-table: reached_by: network
store.durable
  entity rally.table.Match: its instances outlive the command that changed them
timer
  binding tick: a periodic cause invokes rally.table.Tick
```

## Adding one

A new capability is a new derivation rule in `crates/spindle-capability/src/derive.rs` with a test.
A specification does not gain a field that lists capabilities.

## Answering external outcomes

An `external.*` capability is an outcome the model cannot decide on its own. `b10x-spindle run`
answers it per command, from these sources in order:

1. the line's `"external"` object
2. a `--decisions` TOML file: the first of its `[[rules]]` that matches the request, then
   `<outcome> = true | false` under a `["<command>"]` table for one command, then at the top
   level for every command
3. with `--answer-default`, "does not hold" on a command that declares a default branch

A line that nothing answers fails with the outcome in its message, and the run goes on to the
next line; once every line is answered it names that line on stderr and exits 1.

A rule answers one outcome of one command for the requests whose input holds every value in its
`when`:

```toml
[[rules]]
command = "helpdesk.tickets.Escalate"
outcome = "refused"
answer = true
when = { "input.target" = "R" }
```

- `when` maps an input path, `input.<field>` with dots for the members of a struct, to a string,
  an integer or a boolean. A rule matches when every entry equals the request's value. A request's
  `13.0` equals the integer `13`, and a number with a fraction equals no integer.
- `{ input = { target = "R" } }` says the same as `{ "input.target" = "R" }`. One path written
  twice, in both forms, is refused at load like a duplicate key, and so is one answer written under
  both `["a.b.C"]` and `[a.b.C]`.
- An optional input that the request leaves out, or sends as `null`, matches no entry.
- Rules are read in file order. The first rule that matches answers.
- A request that no rule matches is answered by the command's table, then the top-level answer.
- At load, `run` refuses a rule that names an undeclared command, an outcome that is not external,
  an input that the command does not declare, or a value of another type, an empty table or a JSON
  `null` in a reset's table. The message names the rule as `rules[<index>]`, counted from 0, and
  names the entry.

A `run` serving `ingress.network` answers a request's external causes from the decisions file and
then `--answer-default`. A request carries no `"external"` object. A request
that nothing answers is refused `501`, with the outcome in the refusal. A reset keeps the
decisions file. The world of a reset can carry a decision table of the same shape under
`decisions`, its rules under `decisions.rules`, and that table answers first until the next reset.
Without either flag, a served specification that declares an external outcome is refused before
anything listens.

## Serving `ingress.network` over HTTP

Where a component requires `ingress.network` and the environment binds it to `http-loopback`,
`b10x-spindle run` runs every component in one process. It serves those components over HTTP on
`127.0.0.1` instead of reading stdin, unless `--stdin` is given. Any other binding, such as `http` in `microvm.toml`, is
refused before anything listens. Once listening it prints
`{"listening": "127.0.0.1:<port>", "components": [...]}`. `--port` picks the port, and its
default, `0`, lets the system choose one. It serves until SIGINT or SIGTERM.

`--address <IP>` serves on that IPv4 or IPv6 address instead, such as the address a container
port forwards to, and the start line names it (`[::1]:<port>` for `--address ::1`). Without it
`run` binds `127.0.0.1` only. A value that is not an IP address, such as a host name or one with
a port, exits 2 before anything listens. An address off loopback, anything but `127.0.0.0/8`
and `::1`, is served only behind an authenticating proxy: it requires `--trusted-caller-header`
with `--trusted-caller-secret`, or `--caller-token` over TLS (below), and without either `run`
exits 2 before listening. `--fixed-actor`
is not enough, since it proves nobody. On such an address every request that does not carry the
proxy's secret is refused `403` `{"refused": "not granted", "actor": null}` before anything of it
is read: every command and view, `GET /api/model`, `GET /api/events`, `POST /api/reset`,
`GET` and `POST /api/clock` and every route under `/__spindle/`, and also a path no route has, a
method a route does not take and a body of any size. On a loopback address `run` answers exactly as without `--address`.

`--caller-token <FILE>` has the run verify each request's caller itself, from the signed token
the request carries as `Authorization: Bearer <JWT>`. The TOML file names the token's `issuer` and
`audience`, the issuer's key set `jwks` (`https://`, or `http://` to a loopback host), and maps a
role claim to the model's actors (`[actor] claim` with `[actor.values]`) and claims to each
actor's attributes (`[attributes."<actor>"]`); `algorithms` allows asymmetric ones only. A token
verifies where a key of the set that its `kid` names checks its signature and its issuer, audience
and expiry hold. A request without one, or with one that does not verify, answers `401` before
anything of it is read and is neither recorded nor counted; one whose claims map to no caller of
the model answers `403` "not granted". The key set is fetched before listening, and a run that
cannot fetch it exits 1; it is fetched again every `refresh_seconds`, so a rotated key verifies
without a restart. On such a run only `/__spindle/live`, `/__spindle/ready` and
`/__spindle/seed` answer without a token; every other route under `/__spindle/` needs a token
whose claims map to a caller of the model and answers any other `403`. The console routes are
off, and `--console on` exits 2. A mapping naming an actor or attribute the specification does
not declare, or leaving a required attribute of a mapped actor unmapped, exits 1 before
listening, and a refreshed set that lists no usable key refuses every
token until a refresh lists one.
It takes no `--trusted-caller-header` or `--fixed-actor` beside it (exit 1), and off loopback it
needs `--tls-cert` and `--tls-key`, exiting 2 without them.

`--tls-cert <FILE>` with `--tls-key <FILE>` serves HTTPS on the listener instead of plain HTTP:
the PEM certificate chain, the server's own certificate first, and its PEM private key, TLS 1.2
and 1.3. Each requires the other. A plain HTTP request on that port gets no HTTP answer.
`--tls-client-ca <FILE>`, given with both, requires every client, the proxy in front, to present
a certificate the PEM CAs of that file signed (mTLS); a client without one, or with one another CA
signed, fails the handshake. This holds on every route, `/__spindle/live` and `/__spindle/ready`
included. A platform's `httpGet` probe presents no client certificate, so with a client CA it
cannot connect, and the probes are `tcpSocket` probes on the port, which show that the listener
accepts but not what `/__spindle/live` or `/__spindle/ready` answer. The published image holds
the binary alone, with no shell and no probe command, so an `exec` probe has nothing to run.
With TLS alone an `httpGet` probe sets `scheme: HTTPS`. Off loopback mTLS adds to `--trusted-caller-header` and
`--trusted-caller-secret`, or to `--caller-token`, one of which is still required. A file that cannot be read, a file holding
no certificate or key, and a key that does not match the certificate exit 2 before anything
listens, with one line naming the flag and the file. Each file is read once at start: a renewed
certificate is served from the next restart. The start line is the same with TLS as without.

What is served is what ESS derives: each command the component accepts and, for a
`reached_by: network` component, each view of a domain it owns. Each is served exactly where
`ess generate --kind openapi` publishes it:

- Without a binding: at the route ESS derives, `POST /<domain>/commands/<command>` and
  `GET /<domain>/views/<view>`, from their wire names. Each outcome answers at the status ESS
  derives.
- With an `ess-http/1` binding, given as `--http <file>` and checked by
  `ess specify http validate`: an operation the binding lists is at its method, path and path
  variables, compiled by ESS. It answers the status the binding lists per outcome, as
  `ess generate --kind openapi --http <file>` publishes it. An operation the binding does not
  list keeps its derived route.

A command's request body is its input as ESS publishes it, less any field the path carries. A
field the command does not declare is refused. Where the binding says `request: {unknown: ignore}`,
such a member or key is dropped instead, and one the path carries is dropped when it holds the
path's value and refused `400` when it holds another. Fields the binding lists under
`request.query` are read from the query string beside the body, under their wire names, a `List`
field from its key repeated (`tags=a&tags=b`); a body naming one of them is refused `400`. A
`multipart` request, a request body limit other than 1 MiB and a view with `representations` are
not served; `run` names each such route at startup.

A view's parameters are read from the query string under their wire names, and each row it
answers holds its fields under their wire names, an enum variant in its wire spelling and a
struct's members under their wire names, as the row schema ESS projects declares: a field
declared `{name: job_id, wire: jobId}` is answered as `jobId`.

A path variable carries a `String`, `Integer`, `Boolean`, `Decimal`, `Timestamp`, `Duration` or
`Uuid` field, an enum, or a newtype of one without a prefix, an alphabet or another constraint,
such as an id type over `Uuid`. The segment is percent-decoded and read as the field's type, as
ESS's generated servers read it: a segment the type refuses, such as `not-a-uuid` for a `Uuid` or
text that is not an RFC 3339 instant for a `Timestamp`, is refused `400`. ESS refuses a binding
that puts a constrained newtype in the path (`ESS-HTTP-009`), and `run --http` prints that
refusal and exits 1.

A view answers `{"rows": [...]}` unless its route declares another `body`: `array` answers the
rows bare, `single` the first row bare, or the route's `empty` status and body when the view
holds none. `{<member>: array}` and `{<member>: single}` answer the same value as the one member
of an object, `{"<member>": [...]}`; the `empty` answer is not wrapped.

The command answers its outcome's status with the envelope ESS publishes for that outcome:

```json
{
  "outcome": "<declared outcome>",
  "published": [{"event": "<qualified event name>", "payload": {}}],
  "error": "<qualified error name>",
  "payload": {}
}
```

`error` appears only on a refusing outcome, `payload` only where the error carries fields.

| The binding answers the outcome with | The response |
|---|---|
| `body: response` | the command's response alone, as `application/json` |
| `body: response.<field>` | the value of that one field alone, as `application/json` |
| `body: {<member>: response}` or `{<member>: response.<field>}` | an object whose one `<member>` holds what `response` or `response.<field>` answers |
| `body: none` | no body and no `Content-Type` |
| an error object, `body: {<member>: error}`, on a refusing outcome | its bound status with an object whose `<member>` is the error's wire name, beside the error's fields under their wire names. Nothing is written. |
| `headers:` beside any of the above or the envelope | the same answer with each declared header, in name order: a literal as declared, a source as the text of the scalar it reads (left out where that is absent), a `WWW-Authenticate` challenge with each param a quoted string whose `"` and `\` are escaped. A value that holds a control character other than a tab, or a plain header's value that starts or ends with a space or a tab, is never written: the request answers `501` naming the header, without its headers. A declared header never replaces one the listener writes itself. |
| a redirect or the `text` encoding | `501` naming the outcome and which of those it is. Nothing is written. |

A view answers `200` with the body ESS publishes for it, `{"rows": [...]}`. Refusals the surface
makes, as ESS's served application makes them, are `{"refused": "<why>"}`:

| Status | Request |
|---|---|
| `400` | a request it cannot read: an undeclared field, a broken path escape, a body that breaks off |
| `404` | an unknown path |
| `405` | a method other than the route's, compared exactly |
| `408` | a body that does not arrive in time |
| `411` | a chunked body |
| `413` | a body announced over 1 MiB |

A command the runtime cannot complete answers `501` with `{"refused", "committed"}`. A command
whose PostgreSQL store did not answer within its deadline, as a write held behind a lock another
database session holds, answers `503` with `{"refused", "committed": false}`: nothing was written,
and the same request is safe to send again. A SQLite store reports no deadline: a command whose
write it finds busy answers `501`. Outside a drain, a read held so is not answered `503`: a view
answers `501` `{"refused"}`, and `GET /api/events` and `GET /api/history` answer `500`
`{"refused"}`; during a drain after SIGINT or SIGTERM, an events or history read still waiting for
the store is cut off and answered `503`
([draining on SIGTERM](observe.md#draining-on-sigterm)). A
`POST /api/reset` held so answers `500` and leaves the component's store lost until the run
restarts (`story:reset-on-a-busy-store-keeps-the-store`).
