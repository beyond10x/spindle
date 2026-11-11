# Changelog

This file follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and Spindle
follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Bare tags such as `0.1.0`
are Spindle's releases.

## [Unreleased]

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
  lock with the process, after a SIGKILL too. Reading stdin, `run` also locks the database file
  itself and names its lock file after the path the state resolves to, so a second run given a
  symbolic or hard link to a held state file exits 1 too.
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
