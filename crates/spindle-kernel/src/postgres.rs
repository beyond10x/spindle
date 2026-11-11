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
//! that stream names ([`LogStore`]'s `scan`): plain reads of committed events, each one bounded
//! statement, so it takes no lock, never waits on a transaction anywhere on the server, and holds
//! in memory the instances whose row rests, not the log. Only a log holding writes that name no
//! instances is read whole, at one snapshot: those of a SQLite file written before 0.7.0, which
//! `state move` marks when it copies them in (it copies those of 0.7.0 to 0.10.0 naming theirs).

use std::path::{Path, PathBuf};
use std::time::Duration;

use eventlog_core::{
    CaptureError, CaptureLimits, ConsistentTenantCapture, EventLogError, EventStore, TenantId,
    validate_identifier,
};
use eventlog_postgres::{PoolOptions, PostgresConfig, PostgresEventStore};
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;

use crate::sqlite::{Log, LogStore, PREFIX, Visit, backend};
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
            .block_on(PostgresEventStore::migrate(config, options, &[]))
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
        LogStore::on(log, runtime)
    }
}

/// How long `store migrate` waits for another migration of the same tables, and for each of its
/// statements.
const MIGRATION_WAIT: Duration = Duration::from_secs(60);

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
}

/// A connection of its own to `database`, made as its stores make theirs: TLS with
/// `sslmode=require`, the server verified against `database_ca`, where it names one; plaintext to
/// a loopback or Unix-socket host otherwise. Its settings are refused as a store's are. The
/// connection is driven on `runtime`, while a call blocks on it.
///
/// # Errors
/// [`StoreError::Backend`] naming the setting refused, or the server that cannot be reached or
/// refused the connection.
#[cfg(feature = "sqlite")]
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
/// An authority (between `://` and the next `/`, `?` or `#`) holding more than one `@` is
/// refused before it is parsed: a password holding an `@` that is not percent-encoded, whose
/// rest a parser would take for the host.
fn admitted(url: &str) -> Result<tokio_postgres::Config, StoreError> {
    const FORM: &str = "postgres://user:password@host:port/database";
    if let Some((_, rest)) = url.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.matches('@').count() > 1 {
            return Err(StoreError::Backend(format!(
                "database_url holds more than one @ before its host: percent-encode each @ in \
                 the user name and password as %40 ({FORM})"
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
         holds SELECT, INSERT, UPDATE and DELETE on every {prefix}_* table, USAGE on the \
         sequence of {prefix}_events and TEMP on the database; and finds the tables `store \
         migrate` created",
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
