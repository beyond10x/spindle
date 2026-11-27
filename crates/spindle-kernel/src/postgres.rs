//! Instances on PostgreSQL, through `eventlog-postgres`: `store.durable` bound to `postgres`.
//!
//! [`PostgresStore`] is the [`LogStore`] the SQLite store is, on a PostgreSQL event store: each
//! instance one stream, its row in a `spindle.row` event, a removal a `spindle.removed` event,
//! and one `spindle.written` event per entity a write touches. Each component keeps its tables
//! under its own prefix ([`prefix`]), so the components of one deployment share a database
//! without reading each other's streams.
//!
//! eventlog admits a database in one of two ways, and so does this store:
//!
//! - **Isolated**, with no CA bundle: the host must be loopback or a Unix socket, the connection
//!   is plaintext, and opening creates the tables. For tests and a local database.
//! - **Hosted**, with a CA bundle ([`Database::ca`]): TLS (`sslmode=require`), the server
//!   verified against that bundle. The tables are created by [`PostgresStore::migrate`] with the
//!   role that owns the schema, and [`PostgresStore::open`] admits only a role that can run no
//!   DDL and whose `CONNECTION LIMIT` its pools can use, within the database's connection budget
//!   ([`Database::connections`]).
//!
//! A scan reads the entity's own stream page by page and then each instance's row at the version
//! that stream names, up to 1,000 instances and about 32 MiB of rows to one `read_many` call
//! ([`LogStore`]'s `scan`): plain reads of committed events, each bounded, so it takes no lock, never waits on a transaction anywhere on the server, and holds
//! in memory the instances whose row rests, not the log. Only a log holding writes that name no
//! instances is read whole, at one snapshot: those of a SQLite file written before 0.7.0, which
//! `state move` marks when it copies them in (it copies those of 0.7.0 to 0.10.0 naming theirs).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use eventlog_core::{
    BoxFuture, CaptureError, CaptureLimits, ConsistentTenantCapture, EventLogError, EventStore,
    Guard, ProjectionSpec, ProjectionStore, Projector, RecordedEvent, TenantId,
    validate_identifier,
};
use eventlog_postgres::{PoolOptions, PostgresConfig, PostgresEventStore};
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;

use crate::sqlite::{Log, LogStore, PREFIX, SharedReading, Visit, backend};
use crate::store::Reading;
use crate::store::StoreError;

/// Instances in one component's tables of a PostgreSQL database.
pub type PostgresStore = LogStore<PostgresEventStore>;

/// The connections a hosted open keeps out of the stores' share of the budget.
pub const RESERVED: usize = 1;

/// The longest table prefix eventlog admits.
const LONGEST: usize = 32;

/// Where and how a component's store reaches its database: the settings `database_url`,
/// `database_ca`, `database_schema`, `database_pool` and `database_connections`.
#[derive(Clone)]
pub struct Database {
    /// `database_url`: `postgres://user:password@host:port/database`. Never printed: it holds a
    /// password.
    pub url: String,
    /// `database_ca`: a PEM CA bundle the server is verified against. Without one the host must
    /// be loopback or a Unix socket.
    pub ca: Option<PathBuf>,
    /// `database_schema`: the schema holding the tables, `public` by default.
    pub schema: String,
    /// `database_pool`: connections per component store, 4 by default.
    pub pool: usize,
    /// `database_connections`: the database's connection budget available to this deployment,
    /// at least components × [`Database::pool`] + [`RESERVED`]. It is not the serving role's
    /// `CONNECTION LIMIT`, which is from 1 to components × [`Database::pool`]. A hosted open
    /// requires it.
    pub connections: Option<usize>,
}

impl Database {
    /// The database at `url`, with no CA bundle, the schema `public`, a pool of 4 and no budget.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ca: None,
            schema: "public".to_owned(),
            pool: PoolOptions::default().max_connections,
            connections: None,
        }
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Database")
            .field("ca", &self.ca)
            .field("schema", &self.schema)
            .field("pool", &self.pool)
            .field("connections", &self.connections)
            .finish_non_exhaustive()
    }
}

/// The table prefix of `component`'s store: `spindle_` and the component's name, each `-` as
/// `_` and each digit as `__` and the letter of its place in the alphabet (`0` as `a`), so that
/// two component names never share a prefix and eventlog's rule (lowercase ASCII and `_`, up to
/// 32 bytes) holds. A name that does not fit in 31 bytes so, or holds anything else, keeps its
/// first bytes and a tag of seven letters hashed from the whole name: 32 bytes, which no
/// readable prefix is.
#[must_use]
pub fn prefix(component: &str) -> String {
    let mut readable = format!("{PREFIX}_");
    let mut lossy = false;
    for character in component.chars() {
        match character {
            'a'..='z' => readable.push(character),
            '-' => readable.push('_'),
            '0'..='9' => {
                readable.push_str("__");
                readable.push(char::from(b'a' + (character as u8 - b'0')));
            }
            _ => lossy = true,
        }
    }
    if !lossy && readable.len() < LONGEST {
        return readable;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in component.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let mut tagged: String = readable.chars().take(LONGEST - 8).collect();
    tagged.push('_');
    for _ in 0..7 {
        tagged.push(char::from(b'a' + u8::try_from(hash % 26).unwrap_or(0)));
        hash /= 26;
    }
    tagged
}

impl LogStore<PostgresEventStore> {
    /// Creates `component`'s tables in `database`, or checks the ones there: what the role that
    /// owns the schema runs before any store is opened on a hosted database. Running it again
    /// changes nothing.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the database cannot be reached, refuses the role, or holds
    /// tables of another shape.
    pub fn migrate(database: &Database, component: &str) -> Result<(), StoreError> {
        let config = config(database, &prefix(component))?;
        // A migration of the same tables already running holds their lock until it commits: a
        // second one waits for it, as a release's migrate jobs started together do, rather than
        // give up after the two seconds a serving statement is allowed.
        let options = PoolOptions {
            max_connections: 1,
            acquisition_timeout: MIGRATION_WAIT,
            statement_timeout: MIGRATION_WAIT,
            lock_timeout: MIGRATION_WAIT,
            transaction_timeout: MIGRATION_WAIT * 2,
            ..PoolOptions::default()
        };
        runtime()?
            .block_on(PostgresEventStore::migrate(config, options, LEASES))
            .map_err(|error| unreachable_hint(error, database))
    }

    /// Opens `component`'s store in `database`, one of `sharing` stores of this process on it.
    ///
    /// Isolated, without [`Database::ca`]: the tables are created where absent. Hosted, with it:
    /// the tables must exist ([`PostgresStore::migrate`]); the role must run no DDL, have a
    /// connection limit from 1 to `sharing` × [`Database::pool`] and hold the rights eventlog
    /// requires; and `sharing` × [`Database::pool`] + [`RESERVED`] must fit
    /// [`Database::connections`].
    ///
    /// # Errors
    /// [`StoreError::Backend`] naming what was refused.
    pub fn open(database: &Database, component: &str, sharing: usize) -> Result<Self, StoreError> {
        let prefix = prefix(component);
        let config = config(database, &prefix)?;
        let place = place(database, &prefix);
        let leased = held().get(&place).copied();
        let options = PoolOptions {
            max_connections: database.pool,
            ..PoolOptions::default()
        };
        let runtime = runtime()?;
        let log = if database.ca.is_some() {
            let connections = database.connections.ok_or_else(|| {
                StoreError::Backend(format!(
                    "a hosted PostgreSQL needs database_connections, the database's connection \
                     budget available to this deployment: at least components {sharing} x \
                     database_pool {pool} + {RESERVED} reserved",
                    pool = database.pool,
                ))
            })?;
            runtime
                .block_on(PostgresEventStore::open(
                    config,
                    options,
                    connections,
                    sharing,
                    RESERVED,
                ))
                .map_err(|error| refused(error, database, &prefix, sharing))?
        } else {
            runtime
                .block_on(PostgresEventStore::local(config, options))
                .map_err(|error| unreachable_hint(error, database))?
        };
        let mut store = LogStore::on(log, Arc::new(runtime), None)?;
        if let Some(leased) = leased {
            store.fenced(component, leased.token, place)?;
        }
        Ok(store)
    }

    /// Every write of this store from now on lands only while `token` is the token of
    /// `component`'s lease ([`Fence`]), checked in the write's own transaction: what
    /// [`PostgresStore::open`] does where this process took that lease ([`Lease::take`]).
    fn fenced(&mut self, component: &str, token: u64, place: String) -> Result<(), StoreError> {
        let leased: Arc<dyn Projector> = Arc::new(Leased);
        self.call(self.log().register_inline(leased))
            .map_err(|error| {
                StoreError::Backend(format!(
                    "the lease of {component} cannot fence its writes: {error}; run `store \
                     migrate` of this release with the role that owns the schema"
                ))
            })?;
        let fence = Fence {
            component: component.to_owned(),
            token,
            tenant: self.tenant().clone(),
        };
        let named = component.to_owned();
        self.fence(Arc::new(fence), Arc::new(move || current(&named, &place)));
        Ok(())
    }
}

/// How long `store migrate` waits for another migration of the same tables, and for each of its
/// statements.
const MIGRATION_WAIT: Duration = Duration::from_secs(60);

/// The read model a component's lease is kept in, `<prefix>_p_lease`: one row, its body
/// `{"token": <n>, "pid": <the holder's session>, "until": <seconds since 1970>}`, which
/// [`Lease`] takes, renews and releases on a connection of its own, and which every write of a
/// store opened under it reads, locked, inside its own append transaction ([`Fence`]).
/// `store migrate` creates it beside the component's other tables.
const LEASE: ProjectionSpec = ProjectionSpec {
    name: "lease",
    indexed: &[],
};

/// Every read model a component's tables hold.
const LEASES: &[ProjectionSpec] = &[LEASE];

/// The key of the one row of [`LEASE`].
const LEASE_ROW: &str = "lease";

/// Drives [`LEASE`] inline, so a write's fence may read it locked in the write's transaction. No
/// event moves it: only [`Lease`] writes it.
struct Leased;

impl Projector for Leased {
    fn name(&self) -> &'static str {
        "spindle_lease"
    }

    fn projections(&self) -> &'static [ProjectionSpec] {
        LEASES
    }

    fn apply<'a>(
        &'a self,
        _event: &'a RecordedEvent,
        _store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async { Ok(()) })
    }

    fn fingerprint(&self) -> Option<String> {
        Some("spindle-lease/1".to_owned())
    }
}

/// The check every write of a store opened under its component's lease runs inside its own
/// append transaction: it reads the lease row locked, and admits the write only while the row
/// still names the token this process took. A process that took the lease after this one moved
/// the token, so a write of this one, paused past its lease or not, lands nothing.
struct Fence {
    component: String,
    token: u64,
    tenant: TenantId,
}

impl Guard for Fence {
    fn check<'a>(
        &'a self,
        store: &'a mut dyn ProjectionStore,
    ) -> BoxFuture<'a, Result<(), EventLogError>> {
        Box::pin(async move {
            let row = store
                .get_for_update(&LEASE, &self.tenant, LEASE_ROW)
                .await?;
            let held = row
                .as_ref()
                .and_then(|body| body.get("token"))
                .and_then(serde_json::Value::as_u64);
            if held == Some(self.token) {
                return Ok(());
            }
            let now = held.map_or_else(|| "no token".to_owned(), |held| format!("token {held}"));
            let code = format!(
                "fenced: the lease of {} was taken by another process ({now}, this process took \
                 token {}), so this process writes nothing more to its tables",
                self.component, self.token
            );
            FENCED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(|| code.clone());
            Err(EventLogError::GuardRefused { code })
        })
    }
}

/// The first refusal of a write by a [`Fence`] in this process: the lease it was taken under is
/// another process's from then on.
static FENCED: Mutex<Option<String>> = Mutex::new(None);

/// Why a write of this process was refused by its lease's fence, once one was: another process
/// took a lease this process wrote under, so it writes nothing more to those tables.
#[must_use]
pub fn fenced() -> Option<String> {
    FENCED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Where a component's tables are: the database's hosts and name, the schema and the prefix.
fn place(database: &Database, prefix: &str) -> String {
    let name = admitted(&database.url)
        .ok()
        .and_then(|config| config.get_dbname().map(str::to_owned))
        .unwrap_or_default();
    format!("{}/{name}/{}/{prefix}", hosts(database), database.schema)
}

/// Each lease this process took ([`Lease::take`]), by [`place`]: every store this process opens
/// on those tables afterwards writes under it ([`PostgresStore::open`]).
static HELD: Mutex<BTreeMap<String, Held>> = Mutex::new(BTreeMap::new());

fn held() -> std::sync::MutexGuard<'static, BTreeMap<String, Held>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A lease this process took.
#[derive(Clone, Copy)]
struct Held {
    token: u64,
    term: Duration,
    /// When the last take or renewal of it that landed began.
    renewed: Instant,
    /// Set once a renewal found it taken by another process.
    lost: bool,
}

/// Whether the lease of the tables at `place` is known still held by this process: no write of
/// it was refused by its fence, no renewal found it taken, and the last one that landed began
/// less than two thirds of its term ago. A process paused past its lease, once it resumes, reads
/// it as not known until its next renewal lands or finds it taken.
fn current(component: &str, place: &str) -> Result<(), StoreError> {
    if let Some(fenced) = fenced() {
        return Err(StoreError::Backend(fenced));
    }
    let held = held().get(place).copied();
    match held {
        Some(held) if held.lost => Err(StoreError::Backend(format!(
            "the lease of {component} was taken by another process, so this process reads its \
             tables for nothing more"
        ))),
        Some(held) if held.renewed.elapsed() < held.term.saturating_mul(2) / 3 => Ok(()),
        Some(held) => Err(StoreError::Backend(format!(
            "the lease of {component} was last renewed {:?} ago, so it is not known still held \
             until its next renewal",
            held.renewed.elapsed()
        ))),
        None => Err(StoreError::Backend(format!(
            "this process no longer holds the lease of {component}"
        ))),
    }
}

/// A process's hold on the leases of its components' tables (`story:several-writers-one-postgres`):
/// one connection of its own to the database, which reads and writes only the lease rows
/// ([`LEASE`]) and opens no store. A lease is free once its time ran out by the database's clock,
/// or once the session of the process that holds it has ended (a process killed); taking it moves
/// its token on, so each holder's token is greater than every earlier one. Every store this
/// process opens on a component's tables after taking its lease is fenced by that token
/// ([`PostgresStore::open`]).
pub struct Lease {
    database: Database,
    term: Duration,
    runtime: tokio::runtime::Runtime,
    client: Option<tokio_postgres::Client>,
    /// The components whose tables an isolated database was given ([`PostgresStore::migrate`]).
    prepared: BTreeSet<String>,
    /// Why the last statement could not reach the database, while the connection cannot be made
    /// or was closed ([`Lease::trouble`]).
    trouble: Option<String>,
    /// What takes did to the sessions of a process holding a lease row past the lease's end, not
    /// yet asked for ([`Lease::endings`]).
    endings: Vec<Ending>,
    /// Whether the last statement waited out its lock or statement timeout ([`Lease::waited`]).
    waited: bool,
}

/// What a take did where the lease had run out by its committed row while a session of another
/// process still held that row locked, inside a write paused or held up past the lease
/// ([`Lease::take`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ending {
    /// The server ended `sessions` sessions holding the lease row of `component`'s tables, and
    /// with them their transactions and every lock they held: the next take finds the row free.
    Ended {
        /// The component whose lease it is.
        component: String,
        /// How many sessions were ended.
        sessions: usize,
    },
    /// The server refused to end them, as for a role that is not a member of theirs: the take
    /// waits until the server ends their transaction itself (its idle-in-transaction timeout).
    Refused {
        /// The component whose lease it is.
        component: String,
        /// The server's answer.
        why: String,
    },
    /// The row is held past the lease's end, its holder not live, by no session this process can
    /// identify as holding it, so it ends none: the take waits until the row is let go.
    Unidentified {
        /// The component whose lease it is.
        component: String,
    },
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Lease")
            .field("database", &self.database)
            .field("term", &self.term)
            .finish_non_exhaustive()
    }
}

impl Lease {
    /// Connects to `database` for the leases of its components' tables, each held for `term`
    /// from its last renewal.
    ///
    /// # Errors
    /// [`StoreError::Backend`] naming the setting refused, a `term` not from 1 s to
    /// [`LEASE_MAX`], or the server that cannot be reached or refused the connection.
    pub fn connect(database: &Database, term: Duration) -> Result<Self, StoreError> {
        if term < Duration::from_secs(1) || term > LEASE_MAX {
            return Err(StoreError::Backend(format!(
                "a lease of {term:?} is not from 1 s to {} s",
                LEASE_MAX.as_secs()
            )));
        }
        let mut lease = Self {
            database: database.clone(),
            term,
            runtime: runtime()?,
            client: None,
            prepared: BTreeSet::new(),
            trouble: None,
            endings: Vec::new(),
            waited: false,
        };
        lease.connected()?;
        Ok(lease)
    }

    /// How long a lease is held from its last renewal.
    #[must_use]
    pub fn term(&self) -> Duration {
        self.term
    }

    /// Why the last ask could not reach the database, while the connection for the leases cannot
    /// be made or was closed: a take then reads the lease as held ([`Lease::take`]), so a process
    /// standing by says why it cannot take over. `None` once a statement reached it again.
    #[must_use]
    pub fn trouble(&self) -> Option<&str> {
        self.trouble.as_deref()
    }

    /// Whether the last ask waited out its lock or statement timeout, as a renewal queued behind
    /// a write of this process that holds the lease row: the database was reached and the lease
    /// row is held, by this process's write or another's.
    #[must_use]
    pub fn waited(&self) -> bool {
        self.waited
    }

    /// What takes since the last call did to sessions holding a lease row past the lease's end
    /// ([`Ending`]), oldest first.
    pub fn endings(&mut self) -> Vec<Ending> {
        std::mem::take(&mut self.endings)
    }

    /// Ends the sessions that hold the lease row of `component`'s tables, in `table`, locked, once
    /// the lease ran out by its committed row and its holder is not live ([`PAST`]): a take found
    /// the row locked past its lock wait, as by a write of a process paused inside it, which keeps
    /// the row and every lock it took until the server ends its transaction. Only a session whose
    /// transaction locked the row (the row's `xmax`, or a member of that multixact) and which
    /// holds a row lock's lock on `table` is ended ([`END`]); none while the lease runs or its
    /// holder renews. Where none is found, the row is held by sessions this process cannot
    /// identify ([`Ending::Unidentified`]).
    fn end_holders(&mut self, component: &str, table: &str) {
        let Some(client) = self.client.as_ref() else {
            return;
        };
        let tenant = crate::sqlite::TENANT;
        let term = self.term.as_secs_f64();
        let past = self.runtime.block_on(client.query_opt(
            &PAST.replace("{table}", table),
            &[&tenant, &LEASE_ROW, &term],
        ));
        let xmax: String = match past {
            Ok(Some(row)) => match row.try_get(0) {
                Ok(xmax) => xmax,
                Err(_) => return,
            },
            Ok(None) => return,
            Err(error) => {
                let why = self.failed(&error, component).to_string();
                self.endings.push(Ending::Refused {
                    component: component.to_owned(),
                    why,
                });
                return;
            }
        };
        // A plain transaction id is refused here, or names a multixact whose members hold no
        // lock on the lease table; either way the row's `xmax` is matched as it is too.
        let mut held = vec![xmax.clone()];
        if let Ok(rows) = self.runtime.block_on(client.query(MEMBERS, &[&xmax])) {
            held.extend(
                rows.iter()
                    .filter_map(|row| row.try_get::<_, String>(0).ok()),
            );
        }
        let answered = self.runtime.block_on(client.query(
            &END.replace("{table}", table),
            &[&tenant, &LEASE_ROW, &term, &xmax, &held],
        ));
        let ending = match answered {
            Ok(rows) => {
                let sessions = rows
                    .iter()
                    .filter(|row| row.try_get::<_, bool>(0).unwrap_or(false))
                    .count();
                if sessions == 0 {
                    Ending::Unidentified {
                        component: component.to_owned(),
                    }
                } else {
                    Ending::Ended {
                        component: component.to_owned(),
                        sessions,
                    }
                }
            }
            Err(error) => Ending::Refused {
                component: component.to_owned(),
                why: self.failed(&error, component).to_string(),
            },
        };
        self.endings.push(ending);
    }

    /// Makes the connection again where the last one was closed.
    fn connected(&mut self) -> Result<(), StoreError> {
        if self
            .client
            .as_ref()
            .is_none_or(tokio_postgres::Client::is_closed)
        {
            let client = session(&self.database, &self.runtime)?;
            // A statement waits for a write holding the lease row no longer than a third of the
            // term, so a renewal is tried again before the term runs out.
            let wait = (self.term / 3).max(Duration::from_millis(200)).as_millis();
            self.runtime
                .block_on(client.batch_execute(&format!(
                    "SET statement_timeout = {wait}; SET lock_timeout = {wait}"
                )))
                .map_err(|error| {
                    StoreError::Backend(format!(
                        "the connection for the leases did not take its timeouts: {error}"
                    ))
                })?;
            self.client = Some(client);
        }
        Ok(())
    }

    /// `error` of a statement on the lease of `component`'s tables.
    fn failed(&self, error: &tokio_postgres::Error, component: &str) -> StoreError {
        match error.as_db_error() {
            Some(refused)
                if refused.code() == &tokio_postgres::error::SqlState::UNDEFINED_TABLE =>
            {
                StoreError::Backend(format!(
                    "the lease of {component} has no table in the schema {}: run `store migrate` \
                     of this release with the role that owns the schema",
                    self.database.schema
                ))
            }
            Some(refused) => StoreError::Backend(format!(
                "PostgreSQL refused a statement on the lease of {component}: {} (SQLSTATE {})",
                refused.message(),
                refused.code().code()
            )),
            None => StoreError::Backend(format!(
                "the lease of {component} was not read or written: {error}"
            )),
        }
    }

    /// The lease table of `component`, created first on an isolated database.
    fn table(&mut self, component: &str) -> Result<String, StoreError> {
        if self.database.ca.is_none() && !self.prepared.contains(component) {
            PostgresStore::migrate(&self.database, component)?;
            self.prepared.insert(component.to_owned());
        }
        Ok(format!(
            "{}.{}_p_{}",
            self.database.schema,
            prefix(component),
            LEASE.name
        ))
    }

    /// Runs `step` on the lease of `component`'s tables, and answers the token its statement
    /// returned.
    fn run(&mut self, component: &str, step: Step) -> Result<Option<u64>, StoreError> {
        let table = self.table(component)?;
        let term = self.term.as_secs_f64();
        let (statement, token) = match step {
            Step::Take => (TAKE, None),
            Step::Renew(token) => (RENEW, Some(token)),
            Step::Release(token) => (RELEASE, Some(token)),
        };
        let statement = statement.replace("{table}", &table);
        let token = token.map(|token| i64::try_from(token).unwrap_or(i64::MAX));
        let tenant = crate::sqlite::TENANT;
        let parameters: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = match (&step, &token) {
            (Step::Take, _) => vec![&tenant, &LEASE_ROW, &term],
            (Step::Renew(_), Some(token)) => vec![&tenant, &LEASE_ROW, &term, token],
            (_, Some(token)) => vec![&tenant, &LEASE_ROW, token],
            (_, None) => vec![&tenant, &LEASE_ROW],
        };
        // A take on a connection that cannot be made again, as one the server refuses for now,
        // reads the lease as held: a standby stands by and connects again for its next ask.
        if let Err(error) = self.connected() {
            self.trouble = Some(error.to_string());
            self.waited = false;
            return match step {
                Step::Take => Ok(None),
                _ => Err(error),
            };
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| StoreError::Backend("no connection for the lease".to_owned()))?;
        let answered = self
            .runtime
            .block_on(client.query_opt(&statement, &parameters));
        self.waited = answered.as_ref().is_err_and(busy);
        // A statement that ran, or waited out its timeout, reached the database.
        if answered.as_ref().map_or_else(busy, |_| true) {
            self.trouble = None;
        }
        match answered {
            Ok(row) => Ok(row
                .and_then(|row| row.try_get::<_, i64>(0).ok())
                .and_then(|token| u64::try_from(token).ok())),
            // A take that waited out its lock or statement timeout found the row locked by the
            // holder's write, which holds it for its whole transaction: the lease is held, unless
            // it ran out, where the sessions holding the row are ended for the next take.
            Err(error) if matches!(step, Step::Take) && busy(&error) => {
                self.end_holders(component, &table);
                Ok(None)
            }
            Err(error) => {
                let failed = self.failed(&error, component);
                let closed = self
                    .client
                    .as_ref()
                    .is_some_and(tokio_postgres::Client::is_closed);
                if closed || dropped(&error) {
                    self.client = None;
                    self.trouble = Some(failed.to_string());
                    // A take whose connection was closed, as by the server, reads the lease as
                    // held; the next ask connects again.
                    if matches!(step, Step::Take) {
                        return Ok(None);
                    }
                }
                Err(failed)
            }
        }
    }

    /// Takes the lease of `component`'s tables where it is free: answers its new token, or
    /// `None` while another process holds it, and also where the take waited out its timeout or
    /// its connection was closed or refused, which the next take makes again.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where the lease cannot be read or written.
    pub fn take(&mut self, component: &str) -> Result<Option<u64>, StoreError> {
        let began = Instant::now();
        let taken = self.run(component, Step::Take)?;
        if let Some(token) = taken {
            held().insert(
                place(&self.database, &prefix(component)),
                Held {
                    token,
                    term: self.term,
                    renewed: began,
                    lost: false,
                },
            );
        }
        Ok(taken)
    }

    /// Renews the lease of `component`'s tables held under `token` for another term: `false`
    /// once another process took it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where the lease cannot be read or written.
    pub fn renew(&mut self, component: &str, token: u64) -> Result<bool, StoreError> {
        let began = Instant::now();
        let renewed = self.run(component, Step::Renew(token))?.is_some();
        if let Some(held) = held().get_mut(&place(&self.database, &prefix(component)))
            && held.token == token
        {
            if renewed {
                held.renewed = began;
            } else {
                held.lost = true;
            }
        }
        Ok(renewed)
    }

    /// Ends the lease of `component`'s tables held under `token` now, so a process standing by
    /// takes it at its next poll; nothing where another process took it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where the lease cannot be written.
    pub fn release(&mut self, component: &str, token: u64) -> Result<(), StoreError> {
        held().remove(&place(&self.database, &prefix(component)));
        self.run(component, Step::Release(token)).map(drop)
    }
}

/// The longest lease a process may hold from its last renewal: a day.
pub const LEASE_MAX: Duration = Duration::from_secs(86_400);

/// Whether `error` is a statement that waited out its lock or statement timeout.
fn busy(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_some_and(|refused| {
        refused.code() == &tokio_postgres::error::SqlState::LOCK_NOT_AVAILABLE
            || refused.code() == &tokio_postgres::error::SqlState::QUERY_CANCELED
    })
}

/// Whether `error` ended the statement's connection rather than the statement: no answer from
/// the server, a connection failure (SQLSTATE class 08) or the server ending the session (57P,
/// as `pg_terminate_backend` does).
fn dropped(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_none_or(|refused| {
        let code = refused.code().code();
        code.starts_with("08") || code.starts_with("57P")
    })
}

/// What [`Lease::run`] does to a lease row.
enum Step {
    Take,
    Renew(u64),
    Release(u64),
}

/// Takes the lease row where it is absent, it was released, its time ran out by the database's
/// clock while its holder is not live, or the session that holds it has ended; answers the new
/// token. A holder is live while the session the row names (its lease connection) runs a
/// statement, as a renewal waiting for the row's lock held by the holder's own write, or ran one
/// within the last half term: a process renewing, if held up, rather than one paused. `$1` the
/// tenant, `$2` the row's key, `$3` the term in seconds.
const TAKE: &str = "INSERT INTO {table} AS lease (tenant_id, row_key, body) \
     VALUES ($1, $2, jsonb_build_object('token', 1, 'pid', pg_backend_pid(), \
     'until', extract(epoch FROM clock_timestamp())::float8 + $3::float8)) \
     ON CONFLICT (tenant_id, row_key) DO UPDATE SET body = jsonb_build_object( \
     'token', (lease.body->>'token')::bigint + 1, 'pid', pg_backend_pid(), \
     'until', extract(epoch FROM clock_timestamp())::float8 + $3::float8) \
     WHERE (lease.body->>'until')::float8 = 0 \
     OR ((lease.body->>'until')::float8 <= extract(epoch FROM clock_timestamp())::float8 \
     AND NOT EXISTS (SELECT 1 FROM pg_stat_activity live \
     WHERE live.pid = (lease.body->>'pid')::int AND live.pid <> pg_backend_pid() \
     AND (live.state <> 'idle' \
     OR live.state_change > clock_timestamp() - make_interval(secs => $3::float8 / 2)))) \
     OR NOT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = (lease.body->>'pid')::int) \
     RETURNING (body->>'token')::bigint";

/// Moves the lease's end a term past now where it still names the token and this session, the
/// one that took it: a lease whose session ended is free ([`TAKE`]), so a process whose lease
/// connection was closed renews nothing on the next one and gives the lease up. `$3` the term in
/// seconds, `$4` the token.
const RENEW: &str = "UPDATE {table} SET body = jsonb_set(body, '{until}', \
     to_jsonb(extract(epoch FROM clock_timestamp())::float8 + $3::float8)) \
     WHERE tenant_id = $1 AND row_key = $2 AND (body->>'token')::bigint = $4::bigint \
     AND (body->>'pid')::int = pg_backend_pid() \
     RETURNING (body->>'token')::bigint";

/// The lease row's `xmax` as text where the lease ran out by its committed row, which a plain read
/// answers without waiting for the row's lock, and its holder is not live; no row otherwise. A
/// holder is live while the session the row names, its lease connection, runs a statement (as a
/// renewal waiting for the row's lock, held by the holder's own write) or ran one within the last
/// half term: a process renewing, if held up, rather than one paused. `$1` the tenant, `$2` the
/// row's key, `$3` the term in seconds.
const PAST: &str = "SELECT lease.xmax::text FROM {table} AS lease \
     WHERE tenant_id = $1 AND row_key = $2 \
     AND (body->>'until')::float8 <= extract(epoch FROM clock_timestamp())::float8 \
     AND NOT EXISTS (SELECT 1 FROM pg_stat_activity live \
     WHERE live.pid = (lease.body->>'pid')::int AND (live.state <> 'idle' \
     OR live.state_change > clock_timestamp() - make_interval(secs => $3::float8 / 2)))";

/// The transaction ids a multixact `xmax`, `$1`, locks the row for. Where `$1` is a plain
/// transaction id the server refuses it or, where a multixact of that number exists, answers
/// that one's members, which [`END`] then matches to no holder of the lease row.
const MEMBERS: &str = "SELECT xid::text FROM pg_get_multixact_members($1::text::xid)";

/// Ends the sessions holding the lease row where the lease still ran out by its committed row,
/// its `xmax` is still `$4` and its holder is still not live ([`PAST`]); answers one row for each,
/// `true` where it was ended. A session holding the row is one that holds the exclusive lock on
/// a transaction id in `$5` (the row's `xmax`, or the members of that multixact) and a row lock's
/// lock on the lease table (`RowShareLock` as `FOR UPDATE` and `FOR SHARE` take, or
/// `RowExclusiveLock` as a write takes): never this session, one of another table, one that
/// holds only a read lock on this one, or the session the row names unless it holds the row.
/// `$1` the tenant, `$2` the row's key, `$3` the term in seconds.
const END: &str = "WITH lease AS (SELECT 1 FROM {table} AS lease \
     WHERE tenant_id = $1 AND row_key = $2 AND lease.xmax::text = $4 \
     AND (body->>'until')::float8 <= extract(epoch FROM clock_timestamp())::float8 \
     AND NOT EXISTS (SELECT 1 FROM pg_stat_activity live \
     WHERE live.pid = (lease.body->>'pid')::int AND (live.state <> 'idle' \
     OR live.state_change > clock_timestamp() - make_interval(secs => $3::float8 / 2)))), \
     holders AS (SELECT DISTINCT held.pid FROM lease, pg_locks held \
     JOIN pg_locks used ON used.pid = held.pid AND used.locktype = 'relation' \
     AND used.relation = '{table}'::regclass AND used.granted \
     AND used.mode IN ('RowShareLock', 'RowExclusiveLock') \
     WHERE held.locktype = 'transactionid' AND held.transactionid::text = ANY($5::text[]) \
     AND held.mode = 'ExclusiveLock' AND held.granted AND held.pid <> pg_backend_pid()) \
     SELECT pg_terminate_backend(pid) FROM holders";

/// Ends the lease now where it still names the token, `$3`.
const RELEASE: &str = "UPDATE {table} SET body = jsonb_set(body, '{until}', '0') \
     WHERE tenant_id = $1 AND row_key = $2 AND (body->>'token')::bigint = $3::bigint \
     RETURNING (body->>'token')::bigint";

/// The whole log, read only to scan a log holding writes that name no instances (marked so, or
/// found in the entity's stream), at one snapshot taken once this component's appends in flight
/// have committed ([`ConsistentTenantCapture`]). Unlike eventlog's feed, which hides a commit
/// until every older write transaction on the server has ended, it holds every committed event,
/// so it never waits on a transaction of another component or database. It holds the log in
/// memory and eventlog's publication lock while it reads.
impl Log for PostgresEventStore {
    /// Only this release and `state move` write a PostgreSQL store: the move copies a write of
    /// 0.7.0 to 0.10.0 naming its instances, and marks a log it copies a write from before 0.7.0
    /// into.
    const UNMARKED: bool = false;

    /// eventlog's feed answers an event only once every transaction older than its append has
    /// ended anywhere on the server, and no event after one it withholds.
    const WITHHOLDS: bool = true;

    fn each_event(
        &self,
        runtime: &tokio::runtime::Runtime,
        tenant: &TenantId,
        visit: &mut Visit<'_>,
    ) -> Result<(), StoreError> {
        let limits = CaptureLimits {
            max_events: u64::MAX,
            max_blobs: u64::MAX,
            max_projection_rows: u64::MAX,
            max_payload_bytes: u64::MAX,
        };
        let captured = match runtime.block_on(self.capture_tenant(tenant, &[], limits)) {
            // Appends do not record the tenant's stream identity, and a capture needs one: the
            // first scan of a log records it, once, and captures again.
            Err(CaptureError::TenantIdentityMissing) => {
                runtime
                    .block_on(self.stream_identity(tenant))
                    .map_err(backend)?;
                runtime.block_on(self.capture_tenant(tenant, &[], limits))
            }
            other => other,
        }
        .map_err(|error| StoreError::Backend(format!("reading the log: {error}")))?;
        for event in captured.events {
            visit(event)?;
        }
        Ok(())
    }

    /// Reads through the store's own pool and runtime ([`LogStore::shared`]): each read takes a
    /// connection of the pool for one statement, as the writer does between its statements, so
    /// a read holds no lock and never the writer, and the store opens no connection beyond its
    /// pool's `database_pool`. At most `database_pool` - 1 reads run at once (1 with a pool of
    /// 1), so a connection is always left for the writer and the pool's queue never fills; a
    /// read beyond that waits its turn ([`SharedReading`]).
    fn reading(store: &LogStore<Self>) -> Option<Arc<dyn Reading>> {
        let pool = store.log().pool_status().max_connections;
        Some(Arc::new(SharedReading::new(
            store.shared(),
            pool.saturating_sub(1),
        )))
    }
}

/// A connection of its own to `database`, made as its stores make theirs: TLS with
/// `sslmode=require`, the server verified against `database_ca`, where it names one; plaintext to
/// a loopback or Unix-socket host otherwise. Its settings are refused as a store's are. The
/// connection is driven on `runtime`, while a call blocks on it.
///
/// # Errors
/// [`StoreError::Backend`] naming the setting refused, or the server that cannot be reached or
/// refused the connection.
pub(crate) fn session(
    database: &Database,
    runtime: &tokio::runtime::Runtime,
) -> Result<tokio_postgres::Client, StoreError> {
    config(database, PREFIX)?;
    let mut connection = admitted(&database.url)?;
    connection.connect_timeout(PoolOptions::default().connect_timeout);
    let failed = |error: tokio_postgres::Error| match error.as_db_error() {
        Some(refused) => StoreError::Backend(format!(
            "PostgreSQL refused the connection: {} (SQLSTATE {})",
            refused.message(),
            refused.code().code()
        )),
        None => unreachable_hint(EventLogError::Backend(error.to_string()), database),
    };
    let client = match &database.ca {
        Some(ca) => {
            connection.ssl_mode(tokio_postgres::config::SslMode::Require);
            let tls = rustls::ClientConfig::builder()
                .with_root_certificates(roots(ca)?)
                .with_no_client_auth();
            let (client, driver) = runtime
                .block_on(connection.connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls)))
                .map_err(failed)?;
            runtime.spawn(driver);
            client
        }
        None => {
            connection.ssl_mode(tokio_postgres::config::SslMode::Disable);
            let (client, driver) = runtime
                .block_on(connection.connect(tokio_postgres::NoTls))
                .map_err(failed)?;
            runtime.spawn(driver);
            client
        }
    };
    Ok(client)
}

/// The configuration eventlog admits `database` with: verified against its CA bundle where it
/// names one, isolated otherwise. Each setting eventlog would refuse is refused here first,
/// naming it.
fn config(database: &Database, prefix: &str) -> Result<PostgresConfig, StoreError> {
    admitted(&database.url)?;
    validate_identifier("a schema name", &database.schema).map_err(|error| {
        StoreError::Backend(format!(
            "database_schema {:?} is not admitted: {error}",
            database.schema
        ))
    })?;
    match &database.ca {
        Some(ca) => PostgresConfig::verified(&database.url, &database.schema, prefix, roots(ca)?)
            .map_err(backend),
        None => PostgresConfig::isolated(&database.url, prefix)
            .map_err(|error| {
                StoreError::Backend(format!(
                    "{error}: without database_ca, database_url must name a loopback or \
                     Unix-socket host; a hosted PostgreSQL is reached over TLS, verified against \
                     the PEM CA bundle database_ca names"
                ))
            })?
            .with_schema(&database.schema)
            .map_err(backend),
    }
}

/// `url` parsed, where it is a PostgreSQL connection URL naming a host, printing no part of it.
///
/// The parser takes everything before the first `@` after `://` for the user and password,
/// wherever that `@` is, and what follows it for the host. So a URL holding any `@` but one
/// that ends its user info in the authority (between `://` and the next `/`, `?` or `#`) is
/// refused before it is parsed: a user name, password or query string holding an `@` that is
/// not percent-encoded, whose rest the parser would take for the host, or which would name a
/// host by a part of a password. Every part the parser reads is percent-decoded, so `%40`
/// carries an `@` anywhere.
fn admitted(url: &str) -> Result<tokio_postgres::Config, StoreError> {
    const FORM: &str = "postgres://user:password@host:port/database";
    if let Some((_, rest)) = url.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let ats = rest.matches('@').count();
        if ats > 1 || (ats == 1 && !authority.contains('@')) {
            return Err(StoreError::Backend(format!(
                "database_url holds an @ other than the one ending its user info before the \
                 host: percent-encode each @ in the user name, the password and the query string \
                 as %40 ({FORM})"
            )));
        }
    }
    let parsed = url.parse::<tokio_postgres::Config>().map_err(|_| {
        StoreError::Backend(format!(
            "database_url is not a PostgreSQL connection URL: {FORM}"
        ))
    })?;
    if parsed.get_hosts().is_empty() && parsed.get_hostaddrs().is_empty() {
        return Err(StoreError::Backend(format!(
            "database_url names no host: {FORM}"
        )));
    }
    Ok(parsed)
}

/// The hosts and ports `database_url` names, as `host:port`: never its user or password, nor a
/// host holding an `@`, which may be the rest of a password.
fn hosts(database: &Database) -> String {
    const UNNAMED: &str = "the host database_url names";
    let Ok(config) = admitted(&database.url) else {
        return UNNAMED.to_owned();
    };
    let ports = config.get_ports();
    let named = config
        .get_hosts()
        .iter()
        .enumerate()
        .map(|(index, host)| {
            let host = match host {
                tokio_postgres::config::Host::Tcp(name) => name.clone(),
                #[cfg(unix)]
                tokio_postgres::config::Host::Unix(path) => path.display().to_string(),
            };
            if host.contains('@') {
                return None;
            }
            let port = ports
                .get(index)
                .or_else(|| ports.first())
                .copied()
                .unwrap_or(5432);
            Some(format!("{host}:{port}"))
        })
        .collect::<Option<Vec<_>>>()
        .filter(|named| !named.is_empty());
    named.map_or_else(|| UNNAMED.to_owned(), |named| named.join(", "))
}

/// `error` from opening or migrating `database`, and where no connection was made, where it
/// went and, with `database_ca`, that a certificate it did not issue fails the same way.
fn unreachable_hint(error: EventLogError, database: &Database) -> StoreError {
    let unreachable = match &error {
        EventLogError::Backend(why) => !why.contains("SQLSTATE"),
        EventLogError::Deadline { .. } => true,
        _ => false,
    };
    if !unreachable {
        return backend(error);
    }
    let hosts = hosts(database);
    StoreError::Backend(match &database.ca {
        Some(ca) => format!(
            "{error}: no TLS connection to PostgreSQL at {hosts} verified against database_ca \
             {}: the server is down or unreachable, or its certificate was not issued by a CA \
             in database_ca or does not name the host",
            ca.display()
        ),
        None => format!(
            "{error}: no connection to PostgreSQL at {hosts}: the server is down or unreachable"
        ),
    })
}

/// Every certificate of the PEM bundle at `path`.
fn roots(path: &Path) -> Result<RootCertStore, StoreError> {
    let unreadable = |error: &dyn std::fmt::Display| {
        StoreError::Backend(format!(
            "database_ca {} is not a PEM CA bundle: {error}",
            path.display()
        ))
    };
    let mut roots = RootCertStore::empty();
    for certificate in CertificateDer::pem_file_iter(path).map_err(|error| unreadable(&error))? {
        let certificate = certificate.map_err(|error| unreadable(&error))?;
        roots.add(certificate).map_err(|error| unreadable(&error))?;
    }
    if roots.is_empty() {
        return Err(StoreError::Backend(format!(
            "database_ca {} holds no certificate",
            path.display()
        )));
    }
    Ok(roots)
}

/// Why a hosted open was refused, with what eventlog admits a serving role with.
fn refused(error: EventLogError, database: &Database, prefix: &str, sharing: usize) -> StoreError {
    let EventLogError::Invalid(why) = error else {
        return unreachable_hint(error, database);
    };
    let share = sharing.saturating_mul(database.pool);
    StoreError::Backend(format!(
        "PostgreSQL refused the serving role for the tables {prefix}_*: {why}. A serving role \
         holds no DDL right (no CREATE on the schema {schema}, no object it owns there, no \
         membership of another role, none of SUPERUSER, CREATEROLE, CREATEDB, REPLICATION and \
         BYPASSRLS: the owner role's URL is for `store migrate`, never `run`); has a CONNECTION \
         LIMIT from 1 to {share} (components {sharing} x database_pool {pool}), which is not \
         database_connections, the database's budget for this deployment, at least {budget}; \
         holds SELECT, INSERT, UPDATE and DELETE on every {prefix}_* table but \
         {prefix}_schema_version, {prefix}_projection_registry and {prefix}_settlement_floor, on \
         which it holds SELECT and no INSERT, UPDATE, DELETE or TRUNCATE, USAGE on the sequence \
         of {prefix}_events and TEMP on the database; and finds the tables of this release's \
         `store migrate`",
        schema = database.schema,
        pool = database.pool,
        budget = share.saturating_add(RESERVED),
    ))
}

/// The runtime a store's calls block on: one thread, with I/O and timers.
fn runtime() -> Result<tokio::runtime::Runtime, StoreError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| StoreError::Backend(error.to_string()))
}
