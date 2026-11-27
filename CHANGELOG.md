# Changelog

This file follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and Spindle
follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Bare tags such as `0.1.0`
are Spindle's releases.

## [Unreleased]

### Added

- Every event a store commits records what caused its command, read back with it
  (`Store::events_after`, `Origin::cause`, `spindle_kernel::Provenance`): a request's caller,
  its actor, its attributes and the request id the ingress mints for each HTTP request; a seed
  entry's actor, caller and `request_id`; a tick's binding; or the binding that ran it with what
  caused the command whose event it carried, recorded as not known in a run split over
  processes. The memory, SQLite, PostgreSQL and IndexedDB stores keep it, SQLite and PostgreSQL
  in each write's `spindle.written` origin and as the append's actor; an origin written before
  this release reads back with no cause, without a migration. `Host::submit_caused` sends a
  command with its cause (`story:events-record-their-caller`).

## [0.22.0] - 2026-11-27

### Added

- A view filter that compares a parameter with a literal of its type (`param.label == ""`,
  `param.assigned == false`, `param.direction == all`), reads the count of a list parameter
  (`param.kinds.count == 0`), tests a text field, a text parameter or an element of a list
  parameter with `equals_ignore_case` or `in_ignore_case`, alone or under `trimmed`, or reads a
  member of an element of a list field (`exists g in groups: (g.id == param.group)`,
  `defined(g.name)`, `g.id == w` for an outer list-parameter binder `w`) is served, where the view
  was refused whole and its route answered `501`. Each answers the rows ESS 0.99.1's interpreter
  answers, with each parameter left out and given; a comparison with a parameter the read left
  out is unknown and refuses the read unless a guard such as `not defined(param.x)` decides it.
  Any other leaf still refuses the view whole, naming its filter
  (`story:view-filter-parameter-literal`).
- `run` reading stdin fires the specification's event bindings, routed as a served run routes
  them, on its one store: each command a binding maps a line's events to runs in the same step and
  prints its result after the line's own, with `"caused_by": {"line": <n>, "binding": "<id>"}` and
  `"attempts"`, and its events fire bindings in turn. A bound command that is refused or does not
  run fails the run after the last line. `--drop-retries` is accepted on stdin, where it was
  refused. A binding a served run refuses (such as `on_failure: escalate`) now stops a stdin run
  with exit 1 before any line is read, where it was ignored; a specification that declares no
  component routes its bindings as the one implied component (`story:stdin-run-fires-bindings`).
- `run` reading stdin takes an arrange line, `{"arrange": [{"entity", "identity", "state",
  "fields"}]}`, that writes those rows before the next line, read as a reset's world reads them
  and checked against each entity's invariants, all or none; a seeded mint never mints an
  identity one wrote. A command's input may name a value an earlier line answered with
  `{"$line": <n>, "pointer": "<JSON pointer>"}`, such as the identity line `n` minted; a reference
  to a line not answered before it, or to a pointer its answer lacks, rejects the line
  (`story:stdin-arrange-and-line-references`).
- A command whose `ess-http/1` binding reads some input fields from the query string beside its
  `json` or `form` body (`request: {query: [input.<field>, ...]}`) is served, where its route
  answered `501`. Each listed field is read from the query string under its wire name as ESS
  0.99.1's served application reads it, a `List<T>` field from its key repeated
  (`tags=a&tags=b`), and joined with the body's fields; the command runs on the joined input. A
  body member or key naming a listed field is refused `400`, whether the query carries the field
  or not, and so is a query key no listed field names under `unknown: refuse`, or one given twice
  (`story:query-input-beside-body`).
- A served `run` that names a command reading caller attributes not served, because the run
  neither fixes a caller nor has each request prove one, prints one more stderr line naming both
  ways to serve it: `--fixed-actor` with `--fixed-caller`, or `--trusted-caller-header` with
  `--trusted-caller-secret`. The route's reason, its answer and `GET /api/served` are unchanged
  (`story:caller-attributes-on-loopback`).
- A command an event binding invokes that is refused, by an outcome that declares an error or by
  the kernel, is listed in the record of the request that led to it, under `refused_bound`
  (`spindle.observe.RefusedBound`): the binding, the component, the command, the command whose
  event the binding carried (`caused_by`), the refusing outcome and its error, or the kernel's
  error kind, and the attempts, never a value of the error's fields; and counted as
  `spindle_bound_refusals_total{binding, command, refusal}` in `GET /__spindle/metrics`. The log
  stays `spindle-requests/1`, and `GET /api/events` still lists only the events the run committed.
  `spindle_host::Failed` and `RefusedCommand` carry `caused_by`, and `Failed` the kernel's error
  `kind`; `spindle_channel::Sender::next_delivery` reads the id the next envelope carries
  (`story:refused-bound-command-recorded`).
- A value (`{related: …}`), a `when_related:` guard or a row set over an entity another component
  owns is read from the owner's store where both components run in one process (`run` reading
  stdin, a served `run`), and answers what ESS 0.99.1's interpreter answers, where it was refused
  as unsupported and `admit` listed it as not served. `admit --component` and `play --component`,
  where the owner runs in another process, still refuse it by name, naming the entity, its owner
  and the running component. A write of another component's entity, an `affects:` entry, a
  subject or the rows of `instances:`, is refused by name in every run, where nothing refused
  it and it wrote the running component's store, where no read of that entity looks. The
  kernel's `takes_reaching`, `runs_reaching` and `execute_reaching` and the ingress's
  `Surface::reaching` take the components the store reaches
  (`story:cross-component-related-reads`).

### Fixed

- A command names an `Integer` identity whatever its spelling: one sent as `1.0` finds the row
  created as `1`, and the reverse, as ESS 0.99.1's interpreter reads both as one value, where it
  answered that no row has the identity. The kernel writes every `Integer` of a command's input as
  the integer once ESS admitted it (through `Optional`, newtypes, lists, maps, struct members and
  union payloads), so the row, its identity and the event payload hold `1`, and writes a caller's
  `Integer` attribute read by `{caller: …}` the same way; the walk a reset's world uses is the
  kernel's, `spindle_kernel::integers`. A durable row a command created with `1.0` before this
  change is keyed `1.0` and is no longer found; it is not migrated
  (`story:integer-identity-any-spelling`).
- The `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` archives run on glibc 2.17 and
  newer, where 0.21.0's needed `GLIBC_2.39`: `release.yml` links them through `cargo auditable
  zigbuild` against glibc 2.17, fails the row on any newer `GLIBC_` symbol version (`objdump -T`)
  and runs `--version` in `debian:bookworm`. A new archive,
  `b10x-spindle-<version>-x86_64-unknown-linux-musl.tar.gz`, carries a static binary, checked for
  no `INTERP` and no `NEEDED` and run in Alpine; the SBOM covers the five targets and
  `SHA256SUMS` the five archives (`story:release-linux-glibc-floor-and-musl`).

- A guard that reads a present `Map` input, stored field or caller attribute with `.count` or a
  quantifier (`scores.count > 0`, `exists: {in: scores, as: s, that: s > 5}`, `forall:`) is
  decided as ESS 0.99.1's interpreter decides it, where the kernel refused the command as
  undecidable and the served route answered `501`: a value whose declared type is a `Map` binds
  `<path>.count` and its values at `<path>.<index>` in key order, as a list does, and no key at
  any path (`story:map-input-guard-facts`).

- A `when_related:` row-set guard whose `where:` or `forall:` walks the rows of another entity,
  `exists: {in: {entity: …}, as: …, that: …}`, is decided as ESS 0.99.1's interpreter decides it,
  where it was refused at start: every stored row of the walked entity is read with the
  candidates, and a write to it between that read and the commit refuses the command as a
  conflict, as a write to the guard's own entity does. A rows quantifier in an `affects:` or
  `instances:` filter stays refused, as ESS refuses it (`story:related-selector-rows-quantifier`).

- The six wasm lanes of `task check` share one compile-cache key and each ends within 120 s on an
  emptied `target/`, where the Node lane ran 626 s: the `wasm-bindgen-test-runner` runner moves
  from the three test lanes' environment to `.cargo/config.toml`
  (`[target.wasm32-unknown-unknown]`), the workspace `serde_json` takes the `alloc` feature that
  `wasm-bindgen-test` asks for, so `cargo test` and `cargo build` compile the ESS crates once, and
  `check:the_static_bundle_builds` builds `spindle-wasm` in `wasm-gate` itself (`task web` still
  builds the release profile). A new step, `check:the_wasm_lanes_share_one_cache_key`
  (`scripts/wasm-cache-key.sh`), fails when the Taskfile sets a `CARGO_*` variable, when a lane
  builds in a profile other than `wasm-gate` (the lint: `wasm-lint`), when one of the six lanes is
  not run under `scripts/within.sh 120` or holds no wasm `cargo` command the script reads, or when
  `.cargo/config.toml` names no wasm runner (`story:wasm-lanes-within-budget`).

- `run --seed` on a `--state` directory applies the seed once, where every start sent every entry
  again and an entry that updates, moves or removes a row changed it a second time. `seed.json` in
  the state (`0600`, replaced whole on each write) records the seed file's `sha256:` digest, the
  entries sent and those rejected, after each entry: a restart with the same file sends the
  rejected entries again, exiting 1 again where one still is, then the entries not yet sent,
  remembering the request ids of those sent before; it prints that the seed already ran once every
  one is done. One with another file exits 1 before listening, naming both digests, unless the
  state holds no row. A state without `seed.json` gets the whole seed, then the record. A reset
  with no world records 0 entries done before it sends the seed again; one with a world records
  every entry done; one by a run without `--seed` removes the record
  (`story:seed-once-on-durable-state`).

- A served `run` that receives SIGINT or SIGTERM, or whose seed ends having rejected an entry, takes
  no further request and answers every client before it exits, where it took one more exchange
  after a seed entry, a reset taken there emptied a `--state` and was never answered, and an
  answer handed over just before the exit could reach its client as a closed connection. A
  request still waiting is answered `503`; a reset whose seed the signal cuts answers `200` once
  the entry under way is done, `seed.json` counting the entries done. `Listener::close` in
  `spindle-ingress` answers each queued request `503` and writes every answer handed back before
  its thread ends (`story:serve-loop-signal-exit-answers`).

### Fixed

- A command whose `when_related:` guard reads the rows of an entity another component owns (a row
  set selecting it, a rows quantifier walking it, or a row an input names) is refused by name at
  start, naming the entity, its owner and the running component, as a value read from such an
  entity is. It was decided over the running component's store, which keeps none of those rows,
  where ESS's interpreter reads them (`story:related-selector-rows-quantifier`).
- A guard over an `Optional` field the held instance, a `when_related:` row or the input never
  wrote is still `undecidable` where ESS 0.99.1's interpreter selects no outcome, and its refusal
  now names the guard, the absent field and the way out, such as "`label == mine` is unknown on a
  `desk.ticket.Ticket` row where `label` is absent; write `defined(label)` in the guard to rule
  such a row out", where it read "…is undecidable over the held instance and the input". The
  error code and `run`'s status are unchanged (`story:guard-over-absent-row-field`).

- `run --clock <RFC 3339>` starts a run reading stdin, serving HTTP or replaying `--scenario` on
  a manual clock at that instant, where every mode read the system clock; reading stdin, a line's
  `"at": "<RFC 3339>"` moves the clock forward before its command runs, and an instant before the
  clock's, or `"at"` without `--clock`, rejects the line and leaves the clock where it stood.
  `run --mint-seed <n>` mints every identity and generated value from that seed from the first
  command on, reading stdin, serving HTTP or replaying, so the same commands mint the same
  identities, never one an instance of the kept state already holds; a served reset whose world
  carries `mint` mints from that until the next reset, and one whose world has none mints from the
  run's seed again, from its first draw. An environment that binds `identity.mint` to `seeded`
  runs on stdin with seed 0 without the flag, and serves HTTP given the flag, where it was
  refused. `Host::seeded` holds the run's seed, and `HostTarget::clocked` opens every host on a
  given clock and seed. No `run` fires a periodic binding (`story:clock-and-mint-in-every-mode`).

### Fixed

- `run` reading JSON lines on stdin exits 1 when a line was rejected (not JSON, naming no
  declared command, its input not admitted, its external cause unanswered, or refused by the
  kernel before an outcome), after it answered every line, and names each rejected line on stderr
  by its place in the input; it exited 0 whatever the lines did. A line answered by a declared
  outcome, a refusing one included, is not rejected. A seed entry the host or the kernel refuses
  before an outcome, such as `not_granted`, is counted `rejected` in the seed's end line, apart
  from the declared refusals still counted `refused`, and a seed that ends with one stops the
  served run with exit 1; `spindle_seed_commands_total` counts it under `refused`
  (`story:run-fails-on-rejected-line`).

### Fixed

- `run --scenario` prints on each timeline act's line what its command answered: `outcome
  <name>`, with `error <name>` where the outcome carries a declared error, `undeclared` or `not
  granted`, beside `holds` or `diverges`. An act that states no `outcome:` and is refused printed
  `holds` alone; it still holds and exits 0, as ESS 0.99.1's runner passes it, and now names the
  refusal. `HostTarget::answered` lists what each command of the scenario answered
  (`story:scenario-act-without-outcome`).
- `run --scenario` replays a specification that declares no component, the way `run` executes it
  from stdin, where every act diverged with `no component accepts <command>` and the run exited 1.
  It runs as one implied component, `implied`, that owns every domain, accepts every command and
  publishes every event, compiled by ESS beside the specification's files
  (`spindle_spec::load_with_implied_component`). Admission now derives that component's
  requirements, so an environment that lacks one refuses the replay with exit 3, printed as `run`
  prints it: the capabilities it lacks, naming no component. A specification that declares components replays on its unchanged model and still routes by
  `accepts:` (`story:scenario-without-components`).

## [0.21.0] - 2026-11-26

### Added

- A view that declares `paging:` is served, where it was refused at start. A read that sends
  neither the page nor the size answers every row; one that sends both answers that page of the
  rows in `order_by` order, ties in identity order, as ESS 0.99.1's interpreter cuts it, with
  `total` beside the rows only where the view declares `total: true`. A page without its size or
  a size without its page, a negative value, a page below `first_page` and an offset that
  overflows answer `400`, and the conformance target reports them unsupported. `Host::listed_with`
  and `Reader::listed_with` answer the rows with the total (`story:view-paging`).
- A creation whose identity is a bare `String`, an `Integer` or an `Integer` newtype without
  invariants mints it, where it was refused at start: a bare `String` as a UUID's text with no
  prefix, an `Integer` drawn from a UUID as a generated `Integer` is, drawn again where an
  instance already holds it (at most one draw more than the entity's instances, as ESS's
  interpreter draws), and never one a reset world's row holds when the world seeds the mint. A
  `Bytes`, `Timestamp`,
  `Decimal`, `Duration`, enum, struct, `Boolean` or `List` identity is still refused. A value
  source with a fallback, `{input: f, else: <literal>}` or `{input: f, else: input.g}`, answers
  the input, else the literal or the other input, as ESS 0.99.1's interpreter does, where it was
  refused (`story:identity-mint-types-and-fallback`).
- A decisions file holds `[[rules]]`, each naming a command, one of its external outcomes, the
  answer and, in `when`, values the command's input must hold (`"input.<field>"`, dots for a
  struct's members; a string, an integer or a boolean). The first rule in file order whose every
  entry holds answers the request, before the command's table and the top-level answers; an
  input the request leaves out holds no entry. A reset's world carries the same array under
  `decisions.rules`. A rule over a command, outcome or input the specification does not declare,
  or a value not of the input's type, is refused at load, naming `rules[<index>]` and the entry.
  `run`, the Node export and the Service Worker read rules alike, and a reset keeps the
  decisions file a Node instance was created with (`story:decisions-rules-on-input-predicates`).
- A pull request that changes only `.engineering/` and Markdown outside `website/` passes `Gate`,
  `macOS`, `PostgreSQL` and `Build documentation` without building or testing; one that also
  changes `website/` builds and checks the site and skips the build and test steps of the code
  jobs. `scripts/changed-paths.sh` holds the one path list every job reads, and the design pages
  under `docs/design/`, which tests read, count as code
  (`story:planning-only-prs-skip-tests`).
- The test step of `task check` and of CI's `Gate` runs under cargo-nextest: a test is killed at
  60 s (`.config/nextest.toml`), the step after its build fails over 120 s (`scripts/within.sh`)
  and prints its ten slowest tests. Dependencies build at `opt-level = 1` in the dev profile,
  which cut the summed test time of the conformance, kernel and CLI binaries from about 935 s to
  about 240 s; a kernel case compiles its fixture once rather than per command, the
  stuck-collector case sends 20,000 frames rather than 200,000, and the transport, metrics-read
  and concurrent-reads cases bound a second or more, not milliseconds
  (`story:test-step-within-budget`).
- The six wasm lanes of `task check` build in the `wasm-gate` profile (the dev profile without
  debug info or incremental compilation, dependencies at `opt-level = 0`) in place of the release
  profile and the dev profile, and each runs under `scripts/within.sh 120`. On an empty profile
  directory the Node lane took 212 s in the release profile and takes about 100 s; the build and
  the lint of `check:the_wasm_build_of_the_host_succeeds` run side by side, the lint in the
  `wasm-lint` profile. The `wasm-bindgen-test-runner` lanes time a test out at 60 s, not 120 s.
  `task web` still builds the release profile (`story:wasm-lanes-within-budget`).

### Fixed

- A command whose outcome emits an event no component lists under `publishes:` is served, where
  its route answered `501` "does not publish" after the row and the event were written. The event
  is committed with the write and listed by `Host::published` and the response's `published`, as
  ESS 0.99.1's interpreter lists every emitted event, and is not handed to the channel. A
  published event the same command emitted is sent with the command, where it waited in the
  outbox for a later one. An event its component does not publish but another component does is
  still refused by `Sender::publish` and not delivered
  (`story:command-with-unpublished-event-served`).
- A row a reset's world loads answers its `Integer` fields as integers, where `"amount": 1` was
  listed as `"amount":1.0`. `spindle_host::read_world` writes every value the model types `Integer`
  in a row's identity and fields as the integer, through `Optional`, newtypes, lists, maps, struct
  members and union payloads, so `1` and `1.0` name one row and a reference spelled either way
  finds it; every other value is held as the world spells it. An `Integer` that is not an integral
  number within 64 bits is refused, naming the row and the field, as before. `run`, the Node export
  and every host that loads a world answer alike. A command's input is still held as its caller
  spells it: a row `Submit` created from `"amount": 1.0` lists `"amount":1.0`
  (`story:world-row-integer-fields`).
- `POST /api/reset` with no world on a run started with a seed (`run --seed`, or the environment's
  `[seed] file`) submits the seed again, from its first entry to its end, before it answers, where
  it emptied the store and left the seeded rows gone. The end line then gives the counts of that
  seed run alone; `spindle_seed_commands_total` goes on counting across resets and never falls,
  and the reset is still counted in `spindle_resets_total`. A reset drops the entries of a seed
  run not yet sent, so a reset whose body gives a world loads that world alone and does not submit
  the seed. SIGINT or SIGTERM during a reset's seed stops the run between two entries
  (`story:reset-reapplies-seed`).

## [0.20.2] - 2026-11-25

### Added

- The Node export takes fixed actors: `Spindle.create` reads `fixedActors`, a list of
  `{ actor, caller }`, as `run --fixed-actor` with `--fixed-caller` takes them. Each served
  operation is sent or read as the first fixed actor its grant names, and one no fixed actor may
  run answers `403` "not granted", naming the first. `create` refuses an actor given twice, an
  actor the specification does not declare, and a caller its binding refuses, with `run`'s
  reason; `run` and the Node export check fixed callers with the same function
  (`Caller::fixed`) (`story:node-export-fixed-actor`).

## [0.20.1] - 2026-11-25

### Added

- A view filter may hold `always` and `never` anywhere a leaf is read, inside a quantifier
  among them (`not (exists t in param.tags: (always))`), and may read a member of a struct field
  of the row, the identity's own members among them (`key.integration == param.id`). Both are
  evaluated by ESS and answer the rows ESS's interpreter answers, where the view was refused whole
  and its route answered 501 (`story:view-filter-always-and-key`).

### Fixed

- A view route answers each row under its fields' wire names (`{name: job_id, wire: jobId}`
  answers `jobId`), an enum variant in its wire spelling and a struct's members under their wire
  names, as the row schema ESS projects declares and ESS's generated servers answer; 0.20.0
  answered the model names (`story:view-field-wire-names`). View parameters were already read
  under their wire names.

## [0.20.0] - 2026-11-24

### Added

- The Node export, `spindle-node`: the host compiled to WebAssembly and bound by
  `wasm-bindgen --target web` into an ES module that Node 22 loads from bytes, with no file,
  socket or thread of its own. `Spindle.create` opens a specification from its sources, an
  environment whose stores are all bound to `memory` (`ingress.network` bound to `node`,
  `examples/environments/node.toml`), a decisions file or `answerDefault`, and an optional
  `ess-http/1` binding, and refuses what `run` refuses before listening. `notServed()` lists the
  lines `run` prints before it listens; `handle` answers one request at a time through the router
  the loopback listener calls, console routes included, with plain objects in and out;
  `reset(world)` loads a world as `POST /api/reset` does; `dispose()` drops the instance.
  Several instances in one process share nothing. `task check:node_serves_a_specification`
  replays exchanges the loopback listener answers natively.
- `spindle-embed`: the opening of a host from texts and the mapping of an answer to its headers
  and body, shared by the Service Worker host and the Node export. `run`'s world reader is
  `spindle_host::read_world`, and the lines `run` prints before listening are
  `Surface::report`, so every host reads a world and reports what it does not serve the same way.
- `run --http` serves a view route whose `ess-http/1` body is `array`, `single`, `{<member>:
  array}` or `{<member>: single}` (ESS 0.99.0, beyond10x/ess#755), as ESS's generated servers
  answer it: the rows bare, the first row bare, or either as the one member of an object
  (`{"lockers": [...]}`); a `single` view holding no row answers the route's `empty` status and
  body, unwrapped. Each was named not served.

### Fixed

- A seed entry that repeats the request id of an earlier entry that was refused is submitted
  again and counted by what it becomes; it was counted already there though nothing was. A request
  id is recorded only for an entry that was applied or already there: a refusal left nothing, and
  what refused it may have changed since (two `RetireRole ghost` entries now end `2 sent,
  0 applied, 0 already there, 2 refused`).
- `play` and `deploy` stop with exit 1 on an environment document with a `[seed]` table, naming
  the table and the file, as `run` stops on a seed it would not submit. They submit no seed: `play`
  plays the rally match on a temporary state, and `deploy` hands the same document to every
  component's `play --component` process, where an entry for another component could not run.
  Before, both read the table and said nothing.
- 0.19.1 said the kernel runs `{related: {entity, where, field}}` "as ESS's interpreter reads
  it". That holds only for an entity the component running the command owns: the kernel reads only
  that component's store. Over an entity another component owns, ESS's interpreter reads the row
  and the kernel found none and answered `no_value` for every request. Such an outcome, and one
  reading `{related: {via: input.<field>, field}}` over such an entity, is now refused by name
  before anything is read (`unsupported: the value source … owned by component …`), and `admit`
  and `run` name it not served. A guard or an `affects:` entry over such an entity has the same
  limit and is not yet refused (`story:cross-component-related-reads`).
- The kernel reads what a `{related: {entity, where, field}}` selector selects only for the
  outcome selection took, once it acts. It scanned every selector's entity for every outcome
  before selecting, so a refusal paid a read of the rows (a feed read on SQLite) and an accepting
  outcome that reads no selector conditioned its commit on rows it never read.
- Spindle's own scenario `admission-refused` asserts that a refusal keeps no binding the decision
  sends (`satisfies: missing(bindings)`); it asserted nothing about bindings, so a refusal that
  stored them passed.

### Changed

- A path variable whose field is a `Decimal` or a `Timestamp`, or a newtype of one, is read as
  ESS 0.99.0's generated servers read it, now that ESS admits such a variable (beyond10x/ess#755):
  a `Decimal` segment's decimal text is the number it spells, where it was refused `400` as text
  (`"7.25" is not a Decimal`); a `Timestamp` segment that is not an RFC 3339 instant is refused
  `400`, where a command stored it and a view answered `501`. A `Uuid`, `Boolean`, `Duration` or
  `String` segment, and a newtype of one such as an id type over `Uuid`, was already read as the
  type, on a command route and a view route.
- ESS is pinned at 0.99.0 (the crates, the `ess` CLI CI installs and its checksum, and
  `requires: ess 0.99` in `ess/ess-inputs.yaml`); it was 0.98.0. The suite ESS synthesises from
  `ess/` is byte for byte the one 0.98.0 synthesised: 84 scenarios, all executed and passed,
  `conformance.toml` unchanged; the observe schema is unchanged and the observe types differ only
  in their header. What changes for an `ess-http/1` binding passed to `run --http`:
  - A route with `request: {unknown: ignore}` holds a member or key the path carries to the
    path's value: one holding another value is refused `400`
    (`query.<field>: the path carries this field with another value`), where a `query` or `form`
    route dropped it whatever it held. One holding the path's value is still dropped.
  - `unknown: ignore` beside a `json` body, which ESS 0.99.0 admits, drops a top-level member no
    input field names and holds a member the path carries the same way.
  - An outcome's `body: {<member>: response}` or `{<member>: response.<field>}` is answered as the
    object whose one member holds what `response` or `response.<field>` answers.
  - Named not served, each with its reason: a `multipart` request, `request.query`, a route's
    `limits` other than the 1 MiB Spindle reads, and a view with `representations` (CSV); a view
    answered under one member is served (Added). A binding-wide `limits` other than 1 MiB refuses the binding, as a
    binding-wide `refusals` entry does.
- ESS moves on to 0.99.1 (the crates, and the `ess` CLI CI installs and its checksum). Its fixes
  are in suite synthesis only, and nothing changes for a Spindle user: the suite ESS synthesises
  from `ess/` is byte for byte the one 0.99.0 synthesised, 84 scenarios, all executed and passed,
  and the observe types differ only in their header.
- For contributors: `ess/ess-inputs.yaml` requires exactly `ess 0.99.1`, and `Taskfile.yml` and
  `.cargo/config.toml` set `ESS_TOOLCHAIN` to it, so every `ess` a task or a test starts runs
  that release through the `ess` dispatcher, whatever release is installed. `task check` runs
  `scripts/check-ess-pin.sh`, which fails naming the file and both values when the requirement,
  either `ESS_TOOLCHAIN` or the CI workflow's `ESS_VERSION` differ.
- What a seed does not do: Spindle keeps no record of request ids, across starts or replicas, for
  a seed entry or any request. A restart applies nothing new only when every entry is a create
  guarded by `existing_instance:`; any other entry runs again on every start, so an update writes
  over later changes and a move is refused. `POST /api/reset` drops what the seed wrote and does
  not submit it again. README, the environment page and `run --help` say so; 0.19.1 said a restart
  on the same state applies nothing new. The gap is `story:request-id-idempotency`.
- TypeScript 7.0.2 and `@types/node` 26.6.6 in `ui/`, `web/`, `website/` and `website/ux/`.
  In `ui/` and `web/`, TypeScript 7's `tsc` checks only the build configs, `ui/dev`,
  `ui/src/api/types.ts` and `web/src/sw.ts`; the app sources, `.ts` and `.vue` alike, are checked
  by `vue-tsc` on TypeScript 6, through `typescript` aliased to `@typescript/typescript6`, because
  TypeScript 7.0 ships no compiler API. `website/tsconfig.json` drops `baseUrl`, which TypeScript
  7 removed, and `task website` and the documentation workflow now run the site's `typecheck`, so
  that change is checked.

## [0.19.1] - 2026-11-24

### Added

- The kernel runs a value `{related: {entity, where: <predicate>, field}}` (ess/22) in `sets:`
  and event payloads: the field of the one row of the entity the predicate selects, read from the
  store as it was before the outcome, as ESS's interpreter reads it. No row selected, or more than
  one, gives `no_value` and writes nothing; a row the predicate leaves unknown is undecidable.
  Before, every outcome carrying one was refused as unsupported. A predicate reading `payload.`
  or walking the rows of an entity, and the value in an error's fields, stay refused. Works on the
  memory, SQLite and PostgreSQL stores (`story:related-row-selected-by-predicate`).
- `run --seed <file>`, or `[seed] file` in the environment document, names a JSON array of
  entries `{"command", "input", "actor", "caller", "request_id"}` that a served run submits on
  every start, after its stores open, one at a time in the file's order between the requests the
  listener hands over, each as its own actor and caller and never through the listener. Each entry
  counts as applied, already there (an outcome declared `existing_instance:`, or a request id an
  earlier entry carried, which is not submitted again) or refused; a refusal is logged with the
  entry, command, request id and error name, and the next entry is submitted. One line on stderr
  gives the counts when the seed ends, and with `--observe` `GET /__spindle/metrics` has them as
  the new family `spindle_seed_commands_total{counted}`. A file that does not parse, or an entry
  naming a command, input field or actor shape the specification does not declare, stops `run` with
  exit 1 before listening, naming the file and the entry. A run that reads stdin refuses a seed.
- Spindle's own specification lists three more authored scenarios: `admission-missing-provided`
  and `admission-refused`, for the two `DecideAdmission` outcomes ESS's synthesis cannot arrange
  (`ESS-SYNTH-003`), and `admission-admitted`, an admission that binds a required capability, which
  no synthesized scenario sends; `spindle.deployment.EnvironmentState` publishes `os` and `arch`,
  which `DeclareEnvironment` records and no view showed; `task check:conformance` executes 84
  scenarios, all passed (it was 79). A seeded walk of 150 command sequences sends every sequence
  to `HostTarget` and to ESS's interpreter and requires the same answers and view rows, the same
  answers twice, and that a step answered `already-ended` changes nothing
  (`docs/hardening/2026-11-23.md`).
- A view filter that tests a text field with `contains`, `starts_with` or `ends_with` against a
  text parameter or literal, `address: {contains: {param: text}}`, is served on every store, with
  the rows ESS's interpreter answers: the match is byte for byte and case-sensitive, the empty text
  matches every row, an `Optional` parameter left out holds no row, and a field the row does not
  hold refuses the read. `admit` and `run` named such a view not served, and its route answered
  `501`.
- `conform` reads a view with the parameters a scenario gives, so a scenario that queries a view
  with parameters executes; it was counted unsupported.

### Fixed

- An `Optional` event field copied from a stored field the row holds as `null` (`{subject: …}`,
  a changed, related or selected row's field) is carried as `null`, as ESS's interpreter carries
  it, where Spindle left it out. An `establish` step's `null` is now held as `null` rather than
  dropped, so a replayed scenario stating `null` for such a field passes. An event field declaring
  `presence: null_when_absent` is carried as `null` where its source is absent; any other absent
  field is still left out, as ESS leaves it out. View rows are unchanged: they follow a different
  ESS rule, tracked in `story:view-rows-absent-optional-fields`
  (`story:event-payload-absent-optional-null`).

### Changed

- Spindle's own specification: `spindle.placement.Resources` declares that no resource is below
  zero and that `vcpus` fits 32 bits, as `spindle_target::Resources` holds them; a
  `DeclareTarget` or `PlanInstance` with a negative ceiling or request is refused before anything
  is written, where it was declared or planned. The outcomes that answer
  `DeploymentStateConflict` and `InstanceStateConflict` now declare that `state` is the held
  state, and the one that answers `TargetAlreadyRetired` that `target_id` is the input's. The
  kernel already filled them from the row; ESS's interpreter left them out, and no scenario
  checked them. The observe schema and types regenerate:
  `Resources.vcpus` is an `i64` in the Rust types, with the bounds in the schema.

## [0.19.0] - 2026-11-23

### Added

- An `external:` refusal declared `before_row: true` runs: the kernel asks its cause among the
  input refusals, in declaration order, before it reads the addressed row, and answers the refusal
  for a row that exists or not, reading no row, as ESS's interpreter answers it. It was refused
  as unsupported.
- A `when_related: {via: input.<list>, exists: false}` guard over a `List` of references runs: the
  kernel reads the row each element names and takes the branch where any names no row, an empty
  list naming none, and `{missing: input.<list>}` reads the first such element in list order, as
  ESS's interpreter decides and answers it. It was refused as unsupported.

### Changed

- ESS is pinned at 0.98.0 (the crates, the `ess` CLI CI installs and its checksum, and
  `requires: ess 0.98` in `ess/ess-inputs.yaml`); it was 0.97.3, and 0.97.4 on `main` since.
  ESS 0.98.0 refuses an ordering comparison over an enum and an `exists: false` branch declared
  below a refusal it answers first; neither occurs in `ess/`. The suite ESS
  synthesises still has 79 scenarios, all executed and passed, `conformance.toml` is unchanged, and
  the observe types differ only in their header.
- The PostgreSQL store builds on eventlog 0.10.0; it was 0.9.4. Its tables are a new schema
  edition: on a hosted database (`database_ca`), run `b10x-spindle store migrate` with the role that
  owns the schema before `run`, which refuses the tables of an earlier release until then, and
  again after restoring the database from a backup.
- The serving role on a hosted database holds `SELECT` and no `INSERT`, `UPDATE`, `DELETE` or
  `TRUNCATE` on `spindle_<component>_schema_version`, `_projection_registry` and
  `_settlement_floor`; `run` refuses a role that can write them. A deployment that granted DML on
  every table revokes it on these three after `store migrate`:
  `REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON <schema>.spindle_<component>_schema_version,
  <schema>.spindle_<component>_projection_registry, <schema>.spindle_<component>_settlement_floor
  FROM <serving role>`. `store migrate` adds two tables per component: grant the serving role
  `SELECT`, `INSERT`, `UPDATE` and `DELETE` on `spindle_<component>_group_batches` and only
  `SELECT` on `spindle_<component>_settlement_floor`.
- Dependencies: `toml` 1.2 (was 0.9), `signal-hook` 0.4 (was 0.3), `base64` 0.23 (was 0.22) and
  `ruzstd` 0.9 (was 0.8); every other crate at its latest compatible version, and the npm and pnpm
  lockfiles of `website/`, `website/ux/`, `ui/` and `web/` at the latest versions their ranges
  admit, Playwright at 1.65.1 in `website/ux/`.

## [0.18.0] - 2026-11-22

### Changed

- ESS is pinned at 0.97.3 (the crates, the `ess` CLI CI installs and its checksum, and
  `requires: ess 0.97` in `ess/ess-inputs.yaml`); it was 0.92.0. The suite ESS synthesises still
  has 79 scenarios, all executed and passed, `conformance.toml` is unchanged, and the observe types differ
  only in their header.
- `check`, `capabilities` and `admit` no longer read the authored scenarios `ess-inputs.yaml` lists
  under `scenarios:`; the manifest and every file it lists are still checked, so a listed scenario
  that is missing is still refused. `conform`, `run --scenario` and every other command read them
  as before (`spindle_spec::load_model`, `Model`).
- A write of several rows on the SQLite store reads the version of every row that rests from its
  folded rows, caught up once, instead of two blocking log reads per row: renewing 24 rows makes as
  many log calls as renewing one. A row another writer moved is still a conflict naming it.
- Checking a row against its entity's invariants builds no facts where the entity declares none.
- `conform --suite` runs the supplied suite without synthesising one first, and `HostTarget`
  admits the specification once and shares the compiled model with the host of every scenario,
  each with fresh stores.
- A decision call leaves the host only over verified TLS. An `[http.<name>]` call to an
  `https://` URL is made over TLS, the server's certificate verified against the platform's trust
  roots, or against the PEM file a new optional `ca_file` key names; a certificate that does not
  verify fails the call before the request, and with it the bearer token, is written. A plain
  `http://` URL is accepted only on loopback (`localhost`, `127.0.0.0/8`, `[::1]`); any other is
  refused when the environment document is read, naming the table, before any connection. Before,
  only `http://` was called, to any host.

### Fixed

- The control process bounds what one telemetry connection can make it hold. A line longer than
  `FRAME_LIMIT` (1 MiB) ends its connection after at most one byte past the limit is read; a
  connection with no complete frame for five send periods is closed; at most one reader per
  instance and eight more run, and a connection over that is closed at accept; the queue to the
  supervisor is bounded, and a frame it has no room for is dropped and counted
  (`Control::dropped_frames`). Before, each connection had a thread of its own, a line had no
  limit and no deadline, and the queue had no bound. The sender drops a frame longer than
  `FRAME_LIMIT` instead of sending it, and `Answers` keeps at most 16 answers (`RECENT_LIMIT`).

### Security

- A SQLite store creates a missing state directory `0700` and its database, lock, journal and WAL
  files `0600` on Unix, whatever the umask; a file or directory that exists keeps its mode. Before,
  under umask `0022`, the databases were created `0644`, readable by every local user.
- A record of `GET /__spindle/requests` keeps the route template as its `path` (`/loans/{id}`),
  never the raw path with its identifiers, and of a request no route serves the first segment
  only (`/loans/…`); a refusal's reason names the path the same way. Before, the raw path was kept
  beside the template.
- The startup line of `run --fixed-actor` names the fixed caller's attribute names, never their
  values. Before, it printed the caller as JSON.

## [0.17.0] - 2026-11-12

### Added

- A view filter that reads a field through a reference (`person_id.state == Active`), compares a
  field with another field of the row or of a referenced row (`person_id.name == holder`,
  `started_at > person_id.joined_at`), or a referenced field with a parameter
  (`person_id.team == param.team`), answers the rows ESS's interpreter answers for the same state,
  on the memory and SQLite stores, on the host's thread, through its reader and by identity.
  Before, such a view answered `501` "only a field compared with a literal of its type or ordered
  against `now`". A row whose reference is empty or names no row is left out where the filter is
  unknown only for that, as the interpreter leaves it out (`absent_navigation`). Each row a
  reference names is loaded once per read by its identity; the referenced entity is not scanned.

### Changed

- A `when_related:` row-set guard left open by stored rows its filter or `forall` test reads
  as unknown is still refused `501 undecidable`, as ESS's interpreter refuses it, but the refusal
  now says why: the predicate, the entity and each absent `Optional` field it reads, with the
  way out, `defined(<field>)` beside the comparison. It names no stored row and no count: the
  guard reads rows the request did not name. Before, it named only the outcome. `ExecuteError::Undecidable` carries
  this as `why`.

## [0.16.0] - 2026-11-12

### Added

- A view that declares computed fields (`expr:`) answers each row with the value ESS's
  interpreter computes over the row's own fields, by ESS's evaluator and the kernel's one
  computation of a value expression, the one an outcome's `{expr: …}` is written with: `null`
  where an `Optional` operand is absent or a divisor is zero outside `coalesce`, an `Integer` as
  a JSON integer and a `Decimal` as ESS writes it, beside fields read through a reference, on
  the host's thread, through its reader and by identity. A value ESS determines none of (an
  `Integer` past `i64`) refuses the read, as the interpreter refuses it. Before, such a view
  answered `501` "declares computed fields".
- An event payload that reads the command's own response (`{response: <field>}`), which the
  kernel refused as unsupported (`501`) "the value source response field", publishes the value the
  same request answers in that field, a struct member too. A creation whose identity reads it
  creates its row under the answered value, so a view lists the row under the id the request
  returned. The response is prepared where ESS's interpreter prepares it: before a creation takes
  its identity and its `sets:`, and after an existing row's `sets:` and `affects:`, before the
  events. A `sets:` value read through `{payload: <Event>.<field>}` from a published value that
  reads the response stays refused, as ESS's interpreter does not interpret it.
- A command outcome an `ess-http/1` binding declares response headers for answers them beside
  its declared status and body, as ESS's served application writes them, instead of `501`: a
  literal as declared, a source as the text of the scalar it reads (left out where that is
  absent), and a `WWW-Authenticate` challenge with each param a quoted string. A value read from
  the request or the outcome that a header cannot carry (a control character other than a tab,
  or a space or tab at either end of a plain header) is never written: the request answers `501`
  naming the header. A declared header never replaces one the listener writes itself.

### Fixed

- A PostgreSQL store reads back every event it committed, and its head is past them, while a
  transaction older than the write is open anywhere on the server. eventlog's feed withholds an
  event until every such transaction has ended, so `events_after` and `events_head` (and
  `GET /api/events`) answered none of the store's latest writes meanwhile; they now wait for the
  feed to answer the store's own last write, up to 30 s, and then fail naming the open
  transaction. Events of another writer of the same tables may still be answered late, never
  skipped.

## [0.15.0] - 2026-11-11

### Added

- The kernel runs an outcome that declares `when_state_changes: true` or `false`, which it refused
  as unsupported (`501`): the branch holds where its own move does, or does not, change the state
  the subject holds immediately before selection, and its `when:` holds too. It answers among the
  branches guarded by the held state, in declaration order, and a subject no row holds is answered
  by the command's `unknown_instance:` branch first, as in ESS's interpreter.
- A view that declares aggregate fields (`aggregate: {count: {}}`, `sum`, `avg` and the rest of
  ESS's functions), with or without `group_by`, behind a filter the host reads and with each
  measure's own `where:`, answers the rows ESS's interpreter answers for the same state: one row
  per group in the order of the group keys, one row without `group_by` even over no instance, a
  `count` or required `sum` `0` and an `avg` `null` over none, every field answered (`null` where
  absent, as ESS writes an aggregate row), integers answered as integers. It is read from the
  same one scan of the source's resting rows as any view, on the host's thread and beside it.
  Before, such a view answered `501` "declares an aggregation". A read of one row by identity of
  an aggregate view, and an aggregation beside a field read through a reference, stay refused.
- `run --address <IP>` serves HTTP on the IPv4 or IPv6 address given instead of `127.0.0.1`, so a
  container port can reach it; without it `run` still binds `127.0.0.1` only. A value that is not
  an IP address (a host name, a port, brackets) exits 2 before anything listens. The start line
  names the address bound, `{"listening": "<address>:<port>", ...}`. An address off loopback
  (anything but `127.0.0.0/8` and `::1`) requires `--trusted-caller-header` with
  `--trusted-caller-secret`, and without both exits 2 before listening; `--fixed-actor` is not
  enough. There every request without the proxy's secret is refused `403` before anything of it
  is read, on every route, the console routes and `/__spindle/` included, and is neither recorded
  in the request log nor counted. On loopback `run` answers as before. Instances `deploy` starts
  still bind loopback.
- A served `run` answers `GET /api/served`, a console route beside `/api/model`, with one row per
  command and view it routes: `component`, `name`, `kind`, `method`, `path`, `served` and, for
  one not served, the `reason` its start log prints, from the same place. The answer is
  `spindle.observe.ServedOperations`, with its schema and generated types. It is granted as
  `GET /api/model` is: a guarded listener asks for its token, and behind a trusted-caller proxy
  only a request that carries the proxy's secret is answered. The Service Worker answers it too.
  `admit` now prints, without serving, each view `run` would not serve, in the start log's words;
  it printed command refusals only. On one downstream specification that is 62 of 366 views
  (48 aggregations, 6 computed fields, 8 filters); all 520 of its commands are served.
- `GET /api/events?after=head` answers `{"events": [], "next": <cursor>}`, the cursor after the
  last event published, reading no event of the log (`story:events-head-cursor`). A downstream
  consumer with about 179,000 events had to page from the start to reach the end before it could
  poll. A read after that cursor answers what was published since. `limit=0` is still refused
  `400`. Each store answers the position it is at (`Store::events_head`, `Reading::events_head`):
  the memory and IndexedDB stores the position of their last write, SQLite and PostgreSQL the
  end of their feed, found in about twice the binary logarithm of its length single-event feed
  reads.

### Changed

- A page of `GET /api/events?after=<cursor>&limit=<n>` reads at most about `n` events from each
  store (`story:events-read-bounded`), where it read every event after the cursor's position and
  dropped all but `n`, so paging through a long log re-read the rest of it on every page.
  `Store::events_after` and `Reading::events_after` take a limit: the memory, SQLite, PostgreSQL
  and IndexedDB stores answer the first events after the position, write by write up to the write
  that brings them to the limit, which is read whole (`whole_writes`). A store that implements
  `events_after` adds the parameter. The range reads each store until it has `n` events after the
  cursor or none is left, so the pages still list every event in the order of the whole list.

## [0.14.0] - 2026-11-11

### Added

- The kernel runs an outcome that declares secondary effects (`affects:`), which it refused as
  unsupported (`501`): after the subject's own move, update or removal, each entry selects the rows
  of its entity whose `where:` holds in the store as it was before the outcome, the subject
  excepted, and moves them (a row outside the move's `from` states is skipped), removes them or
  writes its `sets:` into them, entries in the order written. Every row left in place is checked
  against its entity's invariants, and the subject and every row land in one write group on every
  store, or nothing is written. A row of an entity an entry selected from, written by another
  command between the selection and the write, refuses the command as a conflict and writes
  nothing, whatever the group's writes are: removals only, or the subject's removal alone where
  no row was selected. An entry over an input list (`each:`) and a filter walking another
  entity's rows stay refused.

- `store.durable` bound to `postgres` keeps each component's state in an operator's PostgreSQL
  (13 or later), through `eventlog-postgres` at the pinned eventlog tag, rows and events encoded as
  the SQLite store encodes them and under the table prefix `spindle_<component>`, so components
  share one database apart. `run` (served) reads the settings `database_url` (required, never
  printed), `database_ca` (a PEM CA bundle: TLS with `sslmode=require`, the server verified
  against it; required off loopback), `database_schema` (`public`), `database_pool` (4 per
  component) and `database_connections` (required with `database_ca`: the database's connection
  budget available to this deployment, at least components x `database_pool` + 1, and not the
  serving role's `CONNECTION LIMIT`) from `SPINDLE_SETTING_<NAME>`. Without `database_ca` a
  loopback or Unix-socket database is opened isolated and its tables created; with it, `run`
  opens with a DML-only serving role and exits 1 before listening on an owner role (naming the
  DDL rights it refuses), a role whose `CONNECTION LIMIT` is outside 1 to components x
  `database_pool`, and a budget below components x `database_pool` + 1: one component at pool 4
  takes a role limited to 4 and `database_connections` 8. A malformed `database_url` or
  `database_schema` is refused naming that setting, as is a `database_url` naming no host or
  holding an `@` other than the one ending its user info before the host (an `@` in the user
  name, the password or the query string not percent-encoded as `%40`), printing none of it; a
  server that cannot be reached is named by host and port, with `database_ca` and the certificate
  as a cause where it is set. A scan reads the entity's own stream and then one event per
  instance, up to 1,000 instances and about 32 MiB of rows to one `read_many` call of eventlog,
  each a bounded read that takes no lock, so a transaction
  left open elsewhere on the server never delays it, another process's commits on the same tables
  never wait for it, and it holds the entity's rows in memory, not the log. The rows the host
  counts when the store opens (`Store::counts`, for `spindle_entity_rows`) are read from each
  entity's own stream alone, no row read. Its reading handle (`Store::reading`) reads through the
  store's own pool beside the writer, one statement per connection taken and no lock held, and
  opens no connection beyond `database_pool`. Reads beside the writer are capped at
  `database_pool` - 1 (1 with a pool of 1), scans and reads of a related field alike, so a
  connection is always left for the writer; a read over the cap waits its turn, in arrival order,
  and fails with a store error only after 30 s. The connection budget is unchanged and `GET`
  views, `/api/model`, `/api/events` and `/__spindle/*` read a postgres-bound component off the
  host's thread as they read a SQLite one; a reset, which empties its tables in place, waits for
  the reads already running, and reads wait for the reset. The new `b10x-spindle store migrate <spec>
  --environment <env>` creates every postgres-bound component's tables with the owner role,
  idempotent (a second one started meanwhile waits for the first), one JSON line per component.
  The example environment is `examples/environments/postgres.toml`. `task check:postgres` runs the
  database cases against `SPINDLE_TEST_DATABASE_URL` and `SPINDLE_TEST_DATABASE_CA`, and CI's `PostgreSQL` job
  runs it on PostgreSQL 17 with TLS. `run` reading stdin refuses `postgres`.
- `b10x-spindle state move --from <dir> <spec> --environment <env>` moves the state a `run
  --state` directory holds into the PostgreSQL tables of every component bound to `postgres`, with
  the settings `run` reads: each `<component>.sqlite`, opened without creating anything, appended
  in commit order, one transaction per write it recorded, so every stream keeps its versions and a
  command decided after the move names the version the directory's last write left. It exits 1,
  writing nothing, when the directory lacks a component's file, when a file holds a JSON string or
  key with U+0000, which the SQLite store keeps and PostgreSQL refuses (every file is read before
  connecting; the refusal names the component, the stream and where), or when a component's
  tables already hold the stream of an entity the specification declares or the file holds, or
  the mark of a partial move: one read of each stream's last version, which writes nothing and
  does not grow with the log, also while a transaction is open elsewhere on the server; on a
  loopback database a component's missing tables are created only after every check passed, so a
  refused move creates no table. A move holds two connections of its role at once, the schema's
  lock and one component's tables in turn, so a role `run` serves two components or more with
  moves them; a role whose `CONNECTION LIMIT` is 1 exits 1 before the tables are read, naming the
  two connections, and a connection refused as one too many is tried again for 5 s. One move
  at a time writes into a schema: a move holds a PostgreSQL advisory lock on its schema from
  before that check to after its last append, and a second move into the schema waits up to 5 s,
  then exits 1 naming the other move, writing nothing. A file written by 0.7.0 to 0.10.0 arrives
  in this release's form, each write's `spindle.written` event naming the instances it left
  (event schema 2), so its scans read the entity's stream; a file holding writes of a release
  before 0.7.0, which recorded no entity version, arrives with its log marked (`spindle:log`
  `unnamed-writes`, before its first write), so its scans read the whole log and miss no row. It
  prints one line per component (`component`, `streams`, `events`). Spindle is stopped first; a
  move that fails partway leaves a partial copy the next move refuses.
- The kernel computes a value written as `{expr: "<expression>"}` in `sets:` and in an event's
  `payload:` (ess/27, `story:kernel-value-expressions`). Such an outcome was refused as an
  unsupported value source and answered 501; a downstream consumer's specification had 7. The value
  is computed by ESS's own evaluator over `input.<path>` and `subject.<field>` as the row held it
  before the outcome, so Integer arithmetic, a Decimal quotient and `round(…, half_even)`, a
  Timestamp moved by a Duration, and `coalesce` answer exactly as ESS's interpreter does. A zero
  divisor or an absent `Optional` leaves the field absent unless `coalesce` supplies a value; a
  result past a bound (an Integer past 64 bits, an instant past the year 9999) is never wrapped:
  the command determines no value and commits nothing. An expression in an error's fields stays
  refused by name, as ESS's interpreter computes none there.
- A view field read through a reference is served (`story:view-related-fields`). A field declared
  `field: {related: {via: <reference>, field: <field>}}` answered `501`; a downstream consumer's
  specification has 26 such views. It now answers what the row the reference names holds now (a
  stored field, its identity or its `state`), from that entity's component, another one included,
  and is left out where the reference is empty or names no row, a removed one included, as ESS's
  interpreter answers. A read loads each referenced row once by its identity (`Store::load`),
  however many rows name it, and never scans the referenced entity. Such a view is read on the
  host's thread, since a store's reading handle answers no row by identity. A filter that reads
  through a reference is still refused.
- Released binaries carry their dependency list (`story:release-dependency-list`). A downstream
  consumer's image scan found no Rust crates in the 0.13.0 binary, so no dependency of Spindle was
  checked for a known vulnerability. The release workflow now builds the four binaries with
  `cargo auditable` 0.7.7, which embeds the crate list in a `.dep-v0` section that `trivy`, `grype`
  and `cargo audit bin` read; each build fails when the tool is missing or when `rust-audit-info`
  finds a runtime crate of `spindle-cli`, the workspace crates and `eventlog-postgres` included,
  missing from the list. Each release page also carries one CycloneDX 1.5 SBOM,
  `b10x-spindle-<version>.cdx.json`, made by `cargo-cyclonedx` 0.5.9 for the four targets and listed
  in `SHA256SUMS`. Each job that builds an archive or the SBOM hashes it, and the publish job
  checks every file against that hash before it writes `SHA256SUMS` from them, so a file changed
  between its build and the release page fails the release.
- `GET /api/events?after=<cursor>&limit=<n>` answers a range (`story:events-range-read`). A
  downstream consumer reported that `GET /api/events` ignored every query, so a proxy that polled
  it read and filtered the whole log on every poll. The answer is `{"events": [...], "next":
  "<cursor>"}`: at most `n` events after the cursor, in the order the whole list has them.
  `next` is opaque, and a cursor taken before a restart or a reset still resumes after its last
  event. `limit` is 100 by default and 1,000 at most, and a larger one is answered with 1,000.
  A malformed `after` or `limit` is refused `400`. Each store is read only after the position the
  cursor marks for it, so a poll with the cursor at the end reads no event, however long the log
  is. Without `after` or `limit` the answer is `{"events": [...]}` as before. The request log
  records a console request's query string as `query`, at most 2,048 bytes as the path is.
  Only the cursor of an answered read and a numeric limit are kept there, and every other name
  and value is written `…`.

### Changed

- `GET /api/events` answers the events each component's store committed, read through the
  store's reading handle beside the host's thread, instead of a list the host kept in memory: a
  served run no longer grows with every event it publishes, and with `store.durable` bound to
  `sqlite` or `postgres` the answer is the same after a restart. Every store reads back the events
  it committed after a position, each with the command its write was attributed to
  (`Store::events_after`, `Store::attribute`, the memory, SQLite, PostgreSQL and IndexedDB
  stores); the host attributes each command with a sequence and merges its components' events
  back into the order it published them. `Host::published` and `Reader::published` now read the
  stores and answer a `Result`. An event no write of a row carries (an outcome without a subject,
  one that preserves its subject, or a set outcome that selected no row) is committed to its
  store on its own, conditioned on the command's scans as a write group is
  (`Store::commit_events`), and listed as before. A reset of a store the host was given
  (PostgreSQL) records in the store that its events so far are forgotten
  (`Store::forget_events`), so a host opened on it later lists none of them; where that record
  cannot be written the reset fails as when the store cannot be emptied. An event an earlier
  release committed is listed with an empty `command`.
- eventlog moves from 0.9.2 to 0.9.3, whose PostgreSQL store answers a batch of reads with one
  statement per 1,000 reads and appends a group in batched statements: a scan of 50,000
  instances, or one write of 20,000, no longer takes one round trip per instance. One command's
  writes stay one transaction, locking each instance it writes: on PostgreSQL's default
  `max_locks_per_transaction` (64) and `max_connections` (100), a command writing more than about
  7,000 instances at once is refused with SQLSTATE 53200 and writes nothing, the message naming
  `max_locks_per_transaction`, which raised to 256 lands one of 20,000.
- Store format, in the SQLite store as in the PostgreSQL one: each write's `spindle.written`
  event names the instances it leaves, each at its stream's version after the write (event schema
  2, `{"rows": {stream: version}, "removed": {stream: version}}`), so a scan of the PostgreSQL
  store reads the entity's stream and one event per instance rather than the whole log. The
  SQLite store reads and scans a file of any earlier release as before, folding the whole file
  into its resting rows once when it opens (below), and adds writes in the new form. A PostgreSQL log holding writes
  that name no instances (0.10.0's, which name none, and those before 0.7.0, which record no
  entity version) is scanned whole when its stream `spindle:log` `unnamed-writes` exists, or
  when the entity's stream holds such a write.
- `GET /__spindle/metrics` writes a series only once something was counted in it
  (`story:metrics-observed-series`). Every command, failure kind, event, binding and entity series
  was written from the start: a downstream consumer's model of 471 commands scraped 15,565 series,
  261 of them non-zero, 1.5 MB per scrape. A served model of 500 commands that has run none now
  writes one series, `spindle_resets_total`; a counted series keeps its value and stays written.
  `spindle_entity_rows` is written from the first scrape that finds the entity holding a row.
  `spindle_http_request_duration_seconds` is now labelled by `method` and `route` template, so a
  slow route shows in its own series, and the new gauge `spindle_metrics_scrape_duration_seconds`
  reports how long the scrape before took.
- A request log record's `reason` never holds a value the caller sent
  (`story:refusal-reason-bounded`). One refused command with an 800 KB mistyped field grew
  `GET /__spindle/requests` by 804,364 bytes. Now every value in the refusal text is written `…`,
  the field and the type it expected kept, and the reason is at most 512 bytes. A map key, struct
  member or input field name the caller sent is written `…` too, and a record's `path` is at most
  2,048 bytes, cut with `…`: a ring of 10,000 records of 404s for paths near 64 KiB held about
  640 MiB. The answer the caller receives keeps an unknown field's name; the kernel now quotes a
  map key or struct member the caller sent (`"key"` where it wrote `` `key` `` or `[key]`), and the
  ingress text it could not percent-decode (`found "…"` where it wrote ``found `…` ``).

### Fixed

- The SQLite store builds on eventlog 0.9.4, which takes rusqlite 0.41 and libsqlite3-sys 0.39.1:
  libsqlite3-sys 0.38.2 bundled an SQLite with advisory RUSTSEC-2026-0459.
- A removal decided on a scan is conditioned on it as a change is: an outcome that `deletes:` its
  instance behind a `when_related:` guard over a row set, and a set outcome (`instances:`) behind
  one, is refused as a conflict and writes nothing where another writer wrote a row of the
  scanned entity between the scan and the write. `Removal` carries the scanned entities
  (`Removal::scanned`), which `remove` and `commit_all` check in the transaction that writes, on
  the memory, SQLite, PostgreSQL and IndexedDB stores alike.

- A set outcome (`instances:`) is conditioned on the scan that selected its rows: where another
  writer wrote a row of the entity between that scan and the write (a grant given while every
  grant of the person is revoked), the outcome is refused as a conflict and writes nothing,
  rather than landing and leaving the new row unselected.

- A second `run` on the same SQLite state is refused (`story:state-directory-lock`). Two runs
  given one `--state` each kept their own outbox and event list over one database. Now `run
  --state <path>` takes an exclusive advisory lock before it opens any store and holds it until it
  exits: on `spindle.lock` inside the directory where it serves HTTP, on `<file>.lock` beside the
  file where it reads stdin. A second run on the same state exits 1 before opening anything,
  naming the state and the id of the process that holds it. The operating system releases the
  lock with the process, after a SIGKILL too. Reading stdin, `run` names its lock file after the
  path the state resolves to, so a second run given a symbolic link to a held state file exits 1
  too. On Linux it also locks the database file itself, so a second run given a hard link exits 1
  as well; macOS takes no lock on the database file, where an `flock` excludes SQLite's own
  byte-range locks and SQLite would find its database locked.
- A served command checks its caller's grant before it reads the request
  (`story:command-grant-before-request`), as ESS's served application does. A fixed actor whose
  `may:` does not name the command, or a proven one, is refused `403` "not granted" naming that
  actor before the body, the query string or the path is read, so a malformed request from an
  ungranted caller no longer answers `400`. A granted caller's malformed request still answers
  `400`, and the transport limits (`411`, `413`, `408`) are still answered first. On a run whose
  requests prove their caller, a path variable is read only after the caller and its grant are
  checked, for a view too, so a request that proves no caller answers `403` whatever its path holds.
- A run serving HTTP without `--fixed-actor` or `--trusted-caller-header` refuses a read-granted
  view and a command some actor's `may:` names with ESS's standard refusal for a request
  authenticated as no caller, `403` `{"refused": "not granted", "actor": null}`
  (`story:unauthenticated-grant-refusals`). Such a view was read for anyone and such a command run
  as no actor. Nothing is read or written. A view and a command no actor's grant names are served
  as before. The grant is checked before anything else this ingress does not serve, so a granted
  command that reads its caller answers this `403` too, not a `501` naming the attributes it reads.
  The same holds wherever a model is served over HTTP with no caller: the Service Worker
  (`spindle-wasm`) and each deployed component process's ingress refuse every such view and
  command `403`. Neither takes `--fixed-actor` or a trusted-caller header, so neither can grant
  one; serve a model whose commands and views a caller must be granted with `b10x-spindle run`
  and `--fixed-actor` or `--trusted-caller-header` (behind a proxy that sets it).
- A view read on a SQLite state costs the rows of its entity, not the whole log
  (`story:view-read-without-log-scan`). Every `Store::scan` of a SQLite store read the tenant's
  whole feed and kept the events of the entity asked for, and 0.13.0 only moved that read beside
  the writer; a downstream consumer on 0.12.1 measured two filtered view reads over entities of
  tens of rows at about 480 ms each on a 142 MB state, while `GET /api/events` took 16 ms. The
  SQLite store now keeps each entity's rows as they rest, with their stream versions and the
  entity's version: folded from one read of its log when it opens, then from only the events
  appended since, read after each write it lands and before each scan or count, so a write by
  another handle on the same file is still seen. Scans and counts, on the store and on its reading
  handle, answer from them; the reading handle's connections share them, a read takes the rows of
  its entity as they stand at one position of the feed, and new events are folded in under a lock
  held only while they are applied, copying an entity only while a read still holds it. So a read
  still sees a write group whole or not at all, a group longer than one page of the feed included,
  and the reading pool's cap is unchanged. A reset or a restart opens a new store, which reads its
  log again. Memory holds the rows that rest, never the history: a log of 1,300 rows of 1 MiB,
  1,200 of them then written again small, holds 111 MiB once opened, scanned and counted; reading
  it in order holds, for a moment, the rows resting at each point of it (1.3 GiB here, as a scan
  of it did before). `SqliteStore::events_read` answers how many events of its log a store has
  read: the whole log once when it opens, then only what was appended. The fold relies on SQLite's
  single writer, under which no event lands behind a feed position already read; a PostgreSQL
  store keeps its scan by the entity's stream. Measured in a debug build on a machine at load 30 to 80, a state of 100,091 events
  whose read entity holds 40 rows: a view read took 1.51 s on the store and 1.31 s on its reading
  handle at the median of 5 before, and 0.45 ms and 0.34 ms after; opening it takes one read of the
  log (2.5 s), which the host already paid when it opened a store to count its rows. Every scan
  and count, on the store, its reading handle and a second handle on the file, equals a read of the
  whole feed after each step of drawn writes, refused groups, restarts and resets, on a new file and
  on one the 0.10.0 release binary wrote.
- `state move --from <dir>` takes the directory's lock (`spindle.lock`, the one `run --state`
  holds) before it reads a file, and holds it to its end. It read the files of a directory a
  running `run --state` was still writing; now, while a run holds the directory, it exits 1 naming
  the directory and the run's process id, before it connects to the database. The lock file a run
  leaves behind is no component's file and does not refuse a later move.
- A view with a field read through a reference (`related:`) is read beside the writer like every
  other view read (`story:view-related-fields`, `story:concurrent-reads`). Since related fields
  landed, such a view was answered on the host's thread, behind every command, because a reading
  handle could scan but not load a row by its identity; a downstream consumer reads 26 such views
  per request. A reading handle now gives a position of its store for some of its entities
  (`Reading::at`, `Position`), whose scans and loads answer the store as one write left it, so a
  read still sees a write group whole or not at all: the memory store takes the entities under one
  lock, SQLite takes them from its resting rows at one position of the feed, and PostgreSQL reads
  through the store's own pool, within `database_pool`, and holds only where no entity it read
  moved meanwhile, the view read again from a new position otherwise, up to 8 times.
- The `reason` of an unmet binding input in the request log (`unmet[].reason`,
  `spindle.observe.UnmetInput`) is bounded as a record's `reason` is
  (`story:refusal-reason-bounded`). It copied the host's text verbatim, so the value the event
  carried was logged (`"member-0042" is not a library.notices.ReaderRef`) and an 800 KB value grew
  one record by 800 KB. Every value in it is now written `…`, and it is at most 512 bytes, the
  binding, the input and the type kept.

## [0.13.0] - 2026-11-11

### Fixed

- A served `run` answers reads while a command runs (`story:concurrent-reads`). Every request was
  handed to the host's thread and answered one at a time, so a view read, the readiness probe
  `GET /api/model` and `GET /__spindle/metrics` waited behind any command, a slow external call
  included; a downstream consumer's probe got no answer during one slow request. Now commands, the
  reset, binding deliveries and ticks still run on the host's thread, one at a time in arrival
  order, and every read is answered beside it on a thread of its own: a view, `GET /api/model`,
  `GET /api/events`, `GET /__spindle/metrics` and `GET /__spindle/topology`
  (`GET /__spindle/requests` and `/__spindle/ui` were already answered by the listener). A view
  reads its store through the store's new reading handle (`Store::reading`): SQLite reads the file
  on connections of its own under its write-ahead log, at most one per processor (at least 2), at
  most 2 kept idle, all driven by one runtime; the memory store keeps each entity's rows as one
  value a write replaces, copying only an entity a read still holds. So a read sees the store as
  the last write that landed left it, never part of one, and neither waits for the other. The row
  counts a reader reads move a whole write group at once, and the events it lists are kept in
  chunks of 256, so a command never copies more than one chunk for a read. A reset hands readers
  the emptied stores, and their counts, only once its world is loaded. The trusted-caller proof and the read
  grants are checked on the read path as on the host's thread. A store with no reading handle, the
  browser's IndexedDB store, is still read on the host's thread. `spindle_host::Reader` reads a
  host from any thread, and `Listener::bind_reading` serves through one; the store contract gains
  a case every reading handle passes. `Clock` is now `Send + Sync`, and `Host::published` and
  `Host::emitted` answer owned values. Measured in debug builds on a machine at load 30 to 40, with
  a command held 2 s by its external call: a view read and `GET /api/model` took 2.0 s at the median
  before (each waited for the command) and 2.7 ms and 3.3 ms after on the memory store, 5.1 ms and
  5.4 ms on SQLite; `GET /__spindle/requests` took 0.9 ms before and 2.2 ms after. Two view reads
  of a store whose every scan takes 600 ms, sent together, answered the slower of the two in
  1.20 s at the median with every read on the host's thread, as before, and 602 ms after.

## [0.12.1] - 2026-11-11

### Fixed

- `GET /__spindle/metrics` no longer scans any store, and requests are no longer held behind it
  (`story:metrics-read-without-scan`). Since 0.10.0 every read counted `spindle_entity_rows` by
  scanning each entity's store, and a scan of a SQLite store reads its whole log, so one read
  cost the log once per entity, on the host's thread, while every request waited. A downstream
  consumer measured 42 s per read (504 commands, 125 entities, a few thousand rows) and its
  readiness probe timed out during each. The host now keeps each entity's rows: counted once when
  a store opens (one read of a SQLite store's log for all its entities, `Store::counts`), then
  moved by each write that lands, so the gauge answers what it did, a reset included, without a
  read. The kept counts see only the writes this process lands: one Spindle process writes a state
  directory, and a second writer on the same store (another process on the same SQLite files)
  moves rows the gauge shows only once the store is next opened. The gauge's `# HELP` line now
  says so ("kept from the writes this process lands and taken once from its store when the store
  opens"), the one line of the exposition whose bytes change. A family's series are found by an
  index instead of a search of every series, and each sample's labels are escaped once; every
  other byte of the exposition is as before. The regression test (500
  commands, 100 entities, 5,000 events on SQLite, a debug build on a heavily loaded machine)
  measured a read at 41.7 s to 56.3 s and a command sent during it at 53.4 s to 57.0 s before;
  after, five reads took 31 ms to 45 ms and the commands 40 ms to 73 ms. The read still runs on
  the host's thread, so a request that arrives during it waits for it, now for milliseconds.

## [0.12.0] - 2026-11-11

### Changed

- ESS 0.92.0: every ESS crate, `schema-contract` included, and CI's `ess` CLI move to tag 0.92.0; the
  specification requires ess 0.92 and the generated output is restamped. A specification in format
  `ess/34`, `ess/35` or `ess/36` is admitted, served over HTTP and conformed unchanged; before,
  `ess/36` was refused whole as `unsupported_format_version`. A format newer than `ess/36` is still
  refused whole, as ESS 0.92.0 refuses it.

## [0.11.0] - 2026-11-11

### Added

- `run` serving HTTP proves each request's caller with `--trusted-caller-header <name>`: a proxy
  in front of the ingress authenticates the request and sets that header to one JSON object,
  `{"actor": "<qualified actor>", "caller": {"<attribute>": <value>}}`, bound against the model as
  `--fixed-caller` is. The proxy is trusted by a shared secret: `--trusted-caller-secret <file>`
  is required with the header option (exit 1 without it, or for a file that cannot be read or is
  empty), and the proxy sends the secret on every request in the header `Spindle-Proxy-Secret`. A
  command that reads its caller then runs for that caller instead of answering `501`, and a
  read-granted view is read as it. A request for a command or view without the caller header,
  with it twice, with a value that names a member twice or proves no caller of the model, or
  without the right secret is refused `403` `{"refused": "not granted", "actor": null}` and writes
  nothing; no default caller is taken, and neither header's value nor the secret is ever printed.
  On such a run the console routes (`/api/*`) and `/__spindle/*` require the secret alone and are
  refused the same way without it. The ingress must be
  reachable only through that proxy, which must remove both headers from client requests. With
  `--fixed-actor` the run exits 1 (design § 11, `architecture-decision-record:http-caller-proof`).

- An event binding with a payload condition (`when: {event: X, where: …}`, ess/22) runs: the
  channel no longer refuses it at start ("a `where` condition is not supported by the channel").
  The condition is decided on each event's payload before the input is mapped, by ESS's own
  evaluator, as ESS 0.90.0's interpreter decides it: true invokes the command, false invokes
  nothing and reports nothing as failed, and a condition that reads an absent or `null` Optional
  field is unknown, invokes nothing and is reported as unmet binding input, with zero attempts,
  while the event's other bindings still run (`spindle_channel::Route::holds`,
  `MappingError::Undecided`).

## [0.10.0] - 2026-11-10

### Added

- An environment can bind an external cause to an HTTP call: `spindle-environment/1` takes
  optional `[http.<name>]` tables (`method`, `url`, `timeout_ms`, `secret_env`), and an
  `external.<command>.<outcome>` key of `provides` bound to `<name>` is answered by one request to
  that call each time `run` asks the cause, on stdin and served. The request is a
  `spindle-external/1` JSON body (capability, command, outcome, cause, the command's input); a
  `2xx` reply `{"holds": true | false}` answers it, and a timeout, a failure or any other reply
  fails the command `undecided` with nothing written (`501` when served). The secret is read from
  the environment variable `secret_env` names and sent as `Authorization: Bearer`; an unset
  variable exits 1 before the first request. `http://` only; a URL carrying userinfo and a
  `timeout_ms` outside 1 to 30000 are refused when the document is read, and a reply body is read
  up to 64 KiB. A call blocks the served run's one command thread until its reply or its timeout.
  A served run whose every external cause is bound to a call needs neither `--decisions` nor
  `--answer-default`. Under `--observe` a cause a call answers is listed and counted with the new
  `AnswerSource` `call` (`spindle.observe`), and one whose call failed as `unanswered`. A document
  without `[http]` reads as before.

- `run --observe` answers `GET /__spindle/topology` with a `spindle.observe.Topology`
  (`spindle-topology/1`), derived from the loaded specification: the ingress, the specification,
  its domains, their commands, views and events, each external cause and each binding, joined by
  `routes`, `contains`, `asks`, `emits`, `carries` and `invokes` edges. Every route has its
  `routes` edge, one answered `501` too. Each edge but `contains` carries live numbers read from
  the counters `GET /__spindle/metrics` exposes and the ring `GET /__spindle/requests` holds: a
  route's requests by status class and the sequences of its latest five, a cause's answers by
  source, the events a command appended, the envelopes a binding sent and the commands it invoked.
  Without `--observe` the route answers `404`. The panel at `GET /__spindle/ui` gains a topology
  screen that draws the graph top-down as SVG, the ingress first, and polls it.

- The kernel runs an `existing_instance:` outcome, which it refused before for every request of
  its command. Where the precedence plan places the branch, a creation whose identity the input
  names and a row already carries answers the branch's declared error and writes nothing;
  otherwise the creation takes the identity, as ESS 0.90.0's interpreter selects it. A command
  whose creations take their identities from different input fields beside `existing_instance:`
  stays refused by name: ESS reads such a command only after selecting the accepting branch.

### Fixed

- A command guard ordering a timestamp against `now` (`when: due_at < now`, `due_at < now -
  3600s`) is decided on the kernel's clock, in `when:`, `when_subject:` and a `when_related:`
  over one row, as ESS's interpreter decides it. The run reads its clock once, the same instant a
  row-set selector reads. Before, such a guard was `Unknown` and the command answered
  `Undecidable`. An invariant is still read with no current time, as ESS reads it at rest.

## [0.9.0] - 2026-11-10

### Added

- `ess/` specifies the reset and the world it loads, in a new domain `spindle.console`
  (`ess/domains/console.yaml`): `World` and `WorldRow` (the body of `POST /api/reset`: rows, a
  decision table and a mint seed, every member optional, no other read), `ResetAnswer`
  (`{"emptied": [...]}`, `200`) and `ResetRefusal` (`{"refused"}`, `400` before anything changed,
  `500` once a store could not be emptied or written). `AnswerSource`'s `world` names the world's
  `decisions`. No behaviour changes.
- `GET /api/deployment` answers `delivery` for each event binding: `guarantee`, ESS's
  `at_least_once` or `at_most_once` as the binding declares it, and `attempts`, the most times
  Spindle invokes its command for one event (5 where the binding retries a failure, 1 otherwise;
  `spindle.observe.BindingDelivery`). A periodic binding answers none.
- `run --observe` lists in each request's `unmet` every event binding whose input an event
  published while the request was answered did not fill, so its command was not invoked: the
  binding, the component that accepts the command, the command and why
  (`spindle.observe.UnmetInput`). `spindle.observe.RequestRecord` gains a required `unmet`, empty
  when every input was met. Before, such a binding was only a line on stderr.

### Changed

- `spindle.deployment` checks an admission decision itself. `DecideAdmission` takes `requires`
  beside `bindings` and `missing`, all three the admitter's answer, and `OfferBundle` no longer
  takes `requires`; an offer to an environment nobody declared is `unknown-environment`
  (`EnvironmentUnknown`). A decision inconsistent with its own `requires` is `not-decided` before
  the deployment's state is read, a binding the environment's `provides` does not list is
  `not-provided`, a refusal naming missing a capability `provides` lists is `missing-provided`,
  and a consistent refusal its external cause does not confirm is `unconfirmed`,
  each `AdmissionDecisionInvalid`. `Deployment`'s `requires`, `bindings` and `missing` are
  `Optional`, absent until a decision sets them; `DeploymentState` publishes them with `bundle`
  and `environment_id`, and `EnvironmentState` is new. Before, `requires` was the offer's caller's, `admitted`
  never read `provides`, and `refused` took a refusal naming a capability that was not required.
- Spindle's own specification types what an environment provides as the environment document
  states it: a new domain `spindle.environment` declares `Provision`, a capability or `prefix.*`
  key (`CapabilityKey`) with its implementations, most preferred first, and
  `spindle.deployment.Environment.provides` and `DeclareEnvironment`'s `provides` input are a list
  of them instead of `Binding`s. `DeclareEnvironment` refuses a key listed twice (`key-twice`) or
  one with no implementation (`no-implementation`) as `ProvidesInvalid`. That an exact key wins
  over a prefix, and the longest prefix over a shorter one, is stated on `provides`.
- `spindle.placement` names every refusal `spindle-target` returns: `RefusalCause` gains
  `EnvironmentMismatch`, recorded by `PlaceInstance`'s own `environment-mismatch` outcome, and
  `not-admitted` no longer covers an admission decided for another environment. An instance
  refused for several reasons records the first in `RefusalCause`'s order, the order
  `spindle_target::place` reports them in (`Refusal::cause` names each). `LaunchInstance`'s
  `not-admitted` is decided by the deployment's state instead of asked as an external cause,
  and `PlanInstance` refuses a deployment no one offered (`unknown-deployment`). Admitting
  `ess/` now needs `external.spindle.placement.PlaceInstance.environment-mismatch` and no
  longer `external.spindle.placement.LaunchInstance.not-admitted`.
- `task check:conformance` declares no known failure: `conformance-known-failures.json` is
  removed and the task passes neither `--known-failing` nor `--accounting-out`, so every executed
  scenario must pass. `spindle.placement.LaunchInstance/outcome/not-admitted`, the one scenario it
  declared, is no longer synthesised: ESS's synthesis refuses it (`ESS-SYNTH-001`). The outcome
  is covered by the authored scenario `spindle.placement/authored/launch-not-admitted`, which
  passes. `conformance.toml` holds 79 executed scenarios, 0 skipped and 0 known failed.
  `conform --known-failing` is unchanged.

### Fixed

- `spindle-kernel` asks no decider before a related row the request names and no store holds is
  answered, unless a branch it could select has a held-state answer for the addressed row, or a
  guard read past an external branch is unknown: an unknown deployment sent to `PlanInstance` is
  `unknown-deployment` whatever the decider would say of `not-admitted`. Before, the look-ahead
  asked `not-admitted` first, and a decider with no answer for it made the request `Undecided`.
- `conform` without `--suite`, and `spindle_conformance::synthesize`, run the authored scenarios
  the specification's `ess-inputs.yaml` lists under `scenarios:`, compiled by ESS
  (`ess_conformance::authored`) into the synthesised suite as `ess conform synthesize --scenarios`
  files them, and print a listed scenario ESS refuses, exiting 1. `spindle_spec::load` reads them
  (`Specification::scenarios`), refusing a missing or symlinked entry as `ess` does, and
  `synthesize` takes the `Specification`. Before, both synthesised without them: on `ess/`, 78
  scenarios where `task check:conformance` runs 79.

## [0.8.0] - 2026-11-08

### Added

- `run --observe` records what answered each external cause asked while a request is answered:
  the request's `externals` lists every one, in the order asked, whether the routed command or a
  command an event binding invoked on the way asked it, each with its `command`, `outcome`,
  `holds` (absent when nothing answered) and `source`: `world` (the decision table of the last
  reset), `file` (`--decisions`), `default` (the default branch `--answer-default` takes) or
  `unanswered` (the command answered `501`). `GET /__spindle/metrics` counts them as
  `spindle_external_causes_total{command,outcome,source}`, so every cause counted during a
  request is listed in it; a cause asked outside any request, by a command a timer fired, is
  counted and listed in no record. `spindle.observe.ExternalAnswer` gains a required `command`.
  Without `--observe` the decider is not wrapped. Before, `externals` was always empty and the
  family had no sample.

### Fixed

- `Surface::new` refuses two components that claim one route template of one method on one
  listener, under ESS's rule (ESS-HTTP-010: templates that differ only in their variable names),
  answering `SurfaceError::Collision` naming both. Before, `GET /things/{id}` and
  `GET /things/{key}` were both accepted and the second could never be reached.
- A creation branch takes an addressed row no store holds beside `unknown_instance:` on every
  command, as ESS's interpreter does (`addressed_unknown`), where it takes its identity from the
  input field that names the row: the creation may take the row, and the `unknown_instance:` answer
  applies only where the branch selected acts on the missing row. Where that branch acts on it
  without creating it, outside row sets, the missing row is answered before any present-related
  refusal (`selected_subject_refusal`). Before, outside row sets, the kernel answered every absent
  addressed row with `unknown_instance:` before any creation was read. A command that declares no
  `unknown_instance:`, a delete among them, answers an absent addressed row, whether or not it
  declares a creation, with its one declared not-found refusal (an `external:` refusal whose error
  carries the identity's type), before its `wrong_state:`, as the interpreter does; an error field
  that refusal gives no source is left out rather than minted. Before, it failed as undeclared or
  answered `wrong_state:`.

## [0.7.0] - 2026-11-08

### Added

- The kernel runs a set outcome (`instances:`): it selects every row of the named entity whose
  filter holds, read from the store as it was before the outcome, and moves, updates or removes
  each in ascending order of identity, as ESS's interpreter does. A move skips a row resting
  outside its `from` states, and zero rows selected is a successful outcome. `{count: changed}`
  reads how many rows changed, and an event declared once per changed row (`each: changed`, with
  its `when_state:`) is published once for each, its `{changed: …}` fields read from that row as
  it was before. A `{generated: true}` value is minted afresh for each row written and each event
  occurrence, as ESS's interpreter mints them. Every row and every event of the outcome is one
  store write (`Store::commit_all`): all of them, or, where one row is refused, none. A filter
  that walks the rows of an entity stays refused. Before, every such outcome was refused as
  unsupported.
- `Store::commit_all` writes several instances, with their events, atomically: the memory store
  and the IndexedDB store in one transaction, SQLite in one `eventlog` append group. A store that
  does not provide it checks every named version first and writes back what it already wrote
  where a later write is still refused. The store contract (`spindle_kernel::contract`) holds it.
- `run --observe` answers `GET /__spindle/metrics` with the run's metrics in the Prometheus text
  format (`text/plain; version=0.0.4`), exactly the families of
  `ess/fixtures/observe/metrics/families.json`, in its order: `spindle_http_requests_total` by
  method, route template, operation, kind, class and status class (an unrouted request has an
  empty route, operation and kind; a method RFC 9110 does not register is counted as `other`),
  `spindle_http_request_duration_seconds`, the counters the host holds per component read at
  scrape time (commands by outcome and failure, command durations, events appended, bindings,
  binding retries, timer ticks, store rows and bytes written), `spindle_entity_rows` read from
  each store, `spindle_resets_total`, and `spindle_external_causes_total`, declared with no sample
  yet. Duration buckets are `DURATION_BOUNDS_US` in seconds; a command's duration histogram has
  buckets, a `_sum` and a `_count` equal to its `+Inf` bucket; a duration histogram of
  `spindle-telemetry` now also keeps the sum of its observed microseconds
  (`HistogramValue::sum_us`, 0 in a frame from an older sender); `spindle.observe.Histogram`
  gains `sum_us`. The scrape is answered on the host's thread and
  is not recorded. `POST /api/reset` keeps every counter and the request log: only
  `spindle_entity_rows` falls and `spindle_resets_total` rises. `Host::rows` reads the rows each
  entity holds; `Exchange::metrics` names a scrape.

### Fixed

- A command whose outcome a `when_related:` guard over a row set selected no longer commits when
  another writer wrote a row of that set's entity between the kernel's scan and its commit: the
  commit is refused as a conflict (`StoreError::Conflict`, identity `*`), so a guard such as "at
  most one active reservation per reader and book" holds with two writers on one store. `Store::scan`
  now answers a `Scan`, the rows and the entity's version, and a `Change` carries the scanned
  entities it is conditioned on (`Change::scanned`), checked by `commit` and `commit_all` alike.
  The memory store counts the writes of each entity; SQLite appends one `spindle.written` event to
  a stream per entity in the same append group; the IndexedDB store counts the writes one global
  scope made or heard. A set outcome's writes are not conditioned on a scan yet.

- `adversary_links::a_second_hello_before_the_link_opens_is_refused` writes both `hello` frames in
  one `write_all`, so it always meets the pending link's refusal; it had written them in two
  writes, which a runner could split around an accept and so land on the open-link case that
  `a_second_hello_on_an_open_link_is_refused_naming_the_peer` asserts. No host code changes.
- A literal path segment wins over a path variable: of two routes of one method that both match a
  request path, the one with a literal where the other has a variable answers, as ESS's dispatch
  order (`ess_gen::http::dispatch_order`, `ESS-HTTP-011`) defines, whatever order the routes are
  declared in. Before, the first route in path-text order answered, so a literal that sorts after
  `{`, such as `/things/~modules`, was shadowed by `/things/{id}`. A request segment is compared
  with a literal of a route an `ess-http/1` binding declares after decoding percent-encoded
  unreserved characters (RFC 3986 §6.2.2.2), as ESS's served application compares it, so
  `/things/%7Emodules` reaches `/things/~modules`; a variable's value is decoded as before. A 405
  names each method once, sorted, in its body and in `Allow`; before, a method two matching routes
  share was named twice.

## [0.6.0] - 2026-11-07

### Added

- `run --observe` answers `GET /__spindle/ui` with the observe panel: one HTML page holding its
  script and style inline, with a one-line banner of the request totals and the request list
  (a "not served only" toggle), read from `./requests` beside it. Any page may frame it: no
  `X-Frame-Options`, no `frame-ancestors`; its `Content-Security-Policy` allows its inline script
  and style and `connect-src 'self'` only. `ui/vite.panel.config.ts` builds it into
  `ui/dist-panel/panel.html` (`pnpm build:panel`, run by `task ui` and `task check:ui`), and
  `b10x-spindle` embeds it; without it the route answers `503`.
- A refusing outcome an `ess-http/1` binding answers with an error object, `body: {<member>:
  error}`, is served: at the bound status, an object whose `<member>` is the error's wire name,
  beside the error's fields under their wire names, as `ess generate --kind openapi --http <file>`
  publishes it, and nothing is written. Before, such an outcome answered `501`.
- A view's filter that reads a field of its source is answered with the rows ESS's interpreter
  answers: a field compared with a literal of its type by `==` or `!=`, or by `<`, `<=`, `>` or
  `>=` where ESS orders the type (`archived == false`, `priority >= 3`, `kind != "draft"`), a
  timestamp also against `now` read once on the host's clock; a field listed `in:` or `none_of:`
  literals; `defined(<field>)`; and `exists` or `forall` over a list field of the row, its element
  compared with a literal, a parameter or an outer binder. A filter ESS leaves unknown for a row,
  such as `note == "keep"` over a row without a note, answers `501` as ESS's interpreter refuses
  the read. Any other leaf still refuses the filter whole, naming it. Before, such a view answered
  `501`.
- The kernel decides an outcome guarded by `when_related:` over a row set: every row of the
  selection's entity is a candidate, selected where the `where:` predicate holds of it (its fields,
  identity and held state, `input.`, `subject.` and the decision's `now`), and the `exists`,
  `count` or `forall` test is decided over the selected rows as ESS's interpreter decides it. On a
  command that creates its addressed row beside a row set, an absent row is left to the row sets
  and then the creation, as ESS does. A row committed by a second writer between the read and the
  commit is not yet detected. Before, such a command was refused as unsupported.

### Changed

- ESS 0.90.0: every ESS crate, `schema-contract` included, and CI's `ess` CLI move to tag 0.90.0; the
  specification requires ess 0.90 and the generated output is restamped.

### Fixed

- A refusal answered with the default envelope keys its `payload` by the error fields' wire names,
  as the bound OpenAPI document publishes it; before, a field renamed on the wire was written under
  its declared name.

## [0.5.0] - 2026-11-07

### Added

- A Service Worker serves a specification: `spindle-wasm` compiles it in the browser, admits it on
  a browser environment and answers its routes and the console routes (`GET /api/model`,
  `GET /api/events`, `POST /api/reset`) through the router the loopback listener calls, so the
  answers are the listener's and no request leaves the browser. A component whose state is bound
  to `indexeddb` keeps it in a `BrowserStore` of its own, every write landed before the answer;
  external outcomes are answered from a decisions file. `examples/environments/browser.toml` binds
  `ingress.network` to `service-worker` and `external.*` to `decisions-file`. `web/` is the static
  site, a Vue shell that registers the worker; `task web` builds it, and `task check` builds it
  and runs the Service Worker in a headless browser.
- `Host::supplied` opens a host on stores it is given, by component, for state bound to an
  implementation the host has no store of its own for; a reset removes every instance they hold.
  The decisions file is read by `spindle_host::Answers`, which `run` now uses too.
- `run --observe [--observe-requests <N>]` records every request its listener answers outside
  `/__spindle/`, `404`, `405` and `400` included, and answers `GET /__spindle/requests` with the
  totals per class and the last N records, newest first; `?class=` filters them. The reserved
  prefix is matched before any specification route, and a specification route whose first literal
  segment is `__spindle` is refused at start.
- The kernel evaluates sets, payload and error values read from the row an input field names, read
  once per command before the outcome. References stored on the subject, chains and lists stay
  refused by name.
- Several `BrowserStore` handles on one database, in one page or across tabs and workers: handles
  in one scope share one copy, every write re-reads its record inside its IndexedDB transaction
  and aborts on a mismatch, and landed writes reach handles in other scopes over a
  `BroadcastChannel`. Across real tabs a stale write is refused on the next flush or call, not on
  the write itself.

### Changed

- ESS 0.89.0: every ESS crate, `schema-contract` included, and CI's `ess` CLI move to tag 0.89.0.

### Fixed

- The link refusal tests wait for the refusals they assert, bounded by 10 s, instead of a fixed
  300 ms.

## [0.4.0] - 2026-11-07

### Added

- A view's `List<T>` parameter that the path does not carry is read from the query string as ESS
  publishes it: the key repeated once per element (`tags=a&tags=b`), each element read as `T`,
  required unless the parameter is `Optional`. `tags=a,b` is the one element `a,b`. An empty
  element, an element of the wrong type and a required list left out answer `400` naming the
  parameter. The host reads a filter that quantifies over a list parameter with `exists` or
  `forall`, its element compared by `==` or `!=` with the state, the identity or a field.
  Before, such a view answered `501`.
- A view's `Decimal` parameter, scalar or list element, is read from the query string as the
  number its decimal text spells (`amount=3.25`), so the view answers the rows ESS's interpreter
  answers; text that is not a decimal literal answers `400` naming the parameter. Before, every
  such read answered `400`.
- A command outcome an `ess-http/1` binding answers with `body: response`, `body:
  response.<field>` or `body: none` is served: at the bound status, the command's response bare
  as `application/json`, the value of the one field bare, or no body and no `Content-Type`, as
  `ess generate --kind openapi --http <file>` publishes it. Before, such an outcome answered `501`.
  An outcome bound with headers, a redirect, the `text` encoding or an error object still answers
  `501` when a request selects it, now naming which of those it is.
- `spindle.observe` specifies the request log (`RequestLog`, format `spindle-requests/1`), the
  metric families (`MetricFamilies`) and the topology (`Topology`, format `spindle-topology/1`) a
  served run will answer under `/__spindle/`, projected to JSON Schema, `spindle-observe-api` and
  the UI's TypeScript types, with fixtures checked against their schemas. No route serves them yet.

### Changed

- The command's response in an HTTP answer is written as ESS publishes it: each field under its
  wire name (`naming: {wire: …}`), through struct members, list items, map values, newtypes and a
  union's payload, and each enum variant in its wire spelling. That holds for the envelope's
  `response` and for a `body: response` or `body: response.<field>` answer alike. Before, the
  envelope's `response` keyed each field by its declared name.
- An outcome the runtime does not run refuses only the request that selects it. A command with
  one such outcome (a value it would generate of a type the runtime does not mint, an identity it
  cannot mint, a response field it does not choose, an error field nothing fills, a refusal that
  changes its row (`compensates: true`), secondary effects (`affects:`)) keeps its HTTP route and
  answers every other outcome as declared; the request that selects that outcome answers `501`
  naming the outcome and what it lacks, and nothing is minted or written. `run` reading stdin
  refuses it the same way, as an `unsupported` failure line. An outcome an `ess-http/1` binding
  answers other than with its status and the envelope is refused the same way. `admit`, `run`'s
  startup and `run` reading stdin name each such outcome on stderr. A construct selection reads
  (a condition, an outcome's shape) still refuses the whole command, and so does a command none of
  whose outcomes runs; `admit` and `run` reading stdin name such a command once on stderr as not
  served, as `run` over HTTP does. New: `spindle_kernel::selects`, `spindle_kernel::takes`,
  `spindle_kernel::execute_refusing`, `spindle_host::Host::submit_refusing`,
  `spindle_ingress::Route::refused`.

## [0.3.0] - 2026-11-06

### Added

- `spindle_ingress::route` answers a request value on a host with a response value, with no
  listener, socket or async runtime; the loopback listener answers through it. Without the default
  `listener` feature `spindle-ingress` builds for `wasm32-unknown-unknown`, and the wasm build
  check covers it.
- `spindle-store-browser`: a `Store` on the browser's IndexedDB, `store.durable` bound to
  `indexeddb` (`examples/environments/browser.toml`). It answers versions, conflicts and removed
  instances as the memory store does, writes each commit in one IndexedDB transaction, and its
  state is read back after the database is reopened. `task check` runs its tests in a headless
  browser. The host does not open it yet.
- `spindle_kernel::contract`: the cases every `Store` passes, run on the memory, SQLite and
  IndexedDB stores.
- `b10x-spindle` builds for macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`), and each
  release carries both archives in `SHA256SUMS`. The in-process commands run there as on Linux;
  `deploy`'s telemetry peer check reads `LOCAL_PEERPID`, and the longest telemetry socket path
  follows the platform's `sun_path` (103 bytes on macOS). The microVM launchers, vsock and the
  `/proc` resource readings answer `Unsupported` off Linux, and a process instance gets no
  parent-death signal there. CI checks and lints both macOS targets and runs the in-process test
  lanes on a macOS runner; a release publishes only once that job is green on the tagged commit.

### Fixed

- A SQLite conflict names the version the change named (none for a creation) and no held version
  for an instance never written, as the memory store does, instead of eventlog's stream positions.

### Changed

- Each crate's integration tests build into one test binary, `tests/<crate>/main.rs` with one
  module per former `tests/*.rs` file; test names are unchanged under their module path. Two
  tests that hold process-wide state keep their own binary: telemetry's allocation test and
  QEMU's context-id test.

## [0.2.0] - 2026-11-06

### Added

- Outcomes that `deletes:` their subject run: the memory store keeps the removed version, SQLite
  appends a `spindle.removed` event, and a later write to the removed instance is a conflict. A
  `DELETE` route runs a deleting outcome. Set deletions (`instances: where`) stay refused.
- Response fields of record, union, map, list, enum and constrained newtype type answer a value of
  that type; one rule decides the answer, the refusal and the derived capabilities.
- `--fixed-actor` may repeat: each command runs as the first fixed actor whose grant names it, a
  view is read as the first that may read it, and anything no fixed actor may run answers 403 and
  writes nothing. The n-th `--fixed-caller` pairs with the n-th `--fixed-actor`.

### Changed

- A refusal answers every error field it declares: a field with no source is read from the refused
  row, left absent when optional, minted from its type, or refused by name before the command
  runs. Capability derivation requires `identity.mint` or `clock` for such fields.
- A `wrong_state` refusal for an identity with no record answers no payload, as the published
  schema describes.
- A view the ingress cannot route (a `List` parameter in the query string, a body other than
  `rows`) answers 501 naming the construct and is listed at startup, instead of refusing startup.
- ESS moved to 0.88.0.

## [0.1.0] - 2026-11-06

The first release: `b10x-spindle` loads an ESS specification, derives the capabilities it needs,
admits it against an environment, and runs it in one process, as processes joined by TCP channels,
or deployed from a placement with a live observe API and UI.

### Added

- Served views take parameters from the path or the query string, typed and checked as ESS's
  served application reads them; a read-granted view checks its reader before reading the request.
- Commands accept `form`- and `query`-encoded requests as their HTTP binding declares.
- Guards over one related row (`when_related:`), including external branches in the look-ahead.
- `spindle-spec`, `spindle-kernel` and `spindle-host` build for `wasm32-unknown-unknown` with
  `--no-default-features`; a command runs there on the memory store. SQLite is the default-on
  `sqlite` feature.
- A release process: `release.yml` builds `b10x-spindle` for Linux (x86_64 and aarch64) on a
  pushed `X.Y.Z` tag and attaches `b10x-spindle-X.Y.Z-<target>.tar.gz` archives and `SHA256SUMS`
  to the bot-created GitHub release. macOS builds are not released yet: the host crates use
  Linux-only APIs (`accept4` and `SOCK_CLOEXEC` in `spindle-telemetry`, the parent-death signal in
  `spindle-target`).
- `b10x-spindle deploy`: place and launch every instance of a `spindle-placement/1` document as
  processes, and serve the read-only observe API and the observe UI built into the binary on
  127.0.0.1.
- `b10x-spindle run` serves a network-reached component over HTTP on 127.0.0.1: routes, command
  responses, read-granted views, a caller fixed at start and a world loaded per reset.
- `b10x-spindle conform`: run the conformance suite ESS synthesises from a specification against
  every component in one process, with known failures and counts held by `task check`.
- A documentation site, built with Docusaurus under `website/` and published at
  <https://beyond10x.github.io/spindle/>: overview, getting started, concepts, the rally example,
  the CLI reference and the status page; and a logo.
- `spindle-docs` generates the CLI reference from the clap definitions and the status page from a
  capability list whose shipped items each name the test that holds them; `task check` fails on
  drift.
- `b10x-spindle metrics`: the counters and histograms one component reports, derived from the
  specification (`spindle-telemetry`); `play` prints the table's non-zero counters under the grid.
- `b10x-spindle run --decisions <file>` and `--answer-default`: answer external outcomes without a
  per-line `external` object.
- List views with a `state ==` filter and `order_by`; minted `Uuid` identities through the derived
  `identity.mint` capability (bound to `uuid-v4` in both example environments).
- Host-side CPU and memory sampling per launched instance, and its share of the request.
- The `spindle.observe` domain with generated JSON Schema, Rust (`spindle-observe-api`) and
  TypeScript types, and the observe UI in `ui/` (Vue 3) built and tested on fixture answers.
- A specification directory is read through its `ess-inputs.yaml` when it has one. ESS moved to
  0.87.0.
- `b10x-spindle play`: the rally example's three components in one process, redrawn in the
  terminal after every tick until a side wins (`spindle-host`).
- `b10x-spindle run`: admit a specification, then execute commands read as JSON lines on stdin
  (`spindle-kernel`, with memory and SQLite stores).
- `spindle-timer`: periodic bindings on an injected clock, with the ESS periodic profile.
- `spindle-channel`: event bindings between components, in process or over TCP, with an outbox and
  at-least-once redelivery.
- `spindle-target`: the deployment target contract, placement within a target's ceiling, and the
  process launcher. `spindle.placement` in Spindle's own specification.
- `spindle-qemu` and `spindle-firecracker`: microVM launchers without root, as libraries; their
  tests run on a host with KVM (`task check:vm`).
- The rally example specification, `examples/rally/ess`.
- `b10x-spindle check`, `capabilities` and `admit`: load and compile a specification with
  `ess-compiler`, derive its capabilities, and admit it against a `spindle-environment/1`
  document (exit 3 when refused).

### Changed

- The `b10x-spindle` command-line definitions moved to `spindle_cli::cli` so the documentation
  generator reads the same clap definitions the binary parses. The commands are unchanged.
